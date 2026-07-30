//! Full drift analysis between two JSON documents.
//!
//! Combines structural path diffing, type change detection,
//! distributional drift metrics (JSD and Wasserstein), and
//! severity classification into a unified drift report.

use std::collections::BTreeMap;

use vajra_stats::{cliffs_delta, FrequencyCounter};
use vajra_types::document::Document;
use vajra_types::json_type::JsonType;
use vajra_types::path::WildcardPath;
use vajra_types::traits::{DriftDetector, DriftReport, TypeChange};

use crate::jsd::jensen_shannon_divergence;
use crate::path_diff::{jaccard_similarity, path_diff, PathDiffResult};
use crate::wasserstein::wasserstein_1d;

/// Default JSD threshold above which distributional drift is reported.
const JSD_THRESHOLD: f64 = 0.05;

/// Default Wasserstein threshold above which distributional drift is reported.
const WASSERSTEIN_THRESHOLD: f64 = 0.1;

/// A single distributional drift observation at a path.
#[derive(Debug, Clone)]
pub struct DistributionalDrift {
    /// The path where drift was detected.
    pub path: String,
    /// Which metric was used.
    pub metric: DriftMetric,
    /// The metric value, in the metric's own units.
    ///
    /// **Not comparable across metrics.** Jensen-Shannon divergence is bounded
    /// to `[0,1]`; Wasserstein distance is in the units of the underlying field,
    /// so a byte-count path can report a value in the hundreds of thousands
    /// while a boolean path reports 0.48. Rank by [`Self::effect_size`] instead.
    pub value: f64,
    /// Unit-free magnitude of the difference between the two groups, in
    /// `[0,1]`, comparable across paths and metrics.
    ///
    /// - Numeric paths: |Cliff's delta|, a rank-based non-parametric effect
    ///   size. 0 means the two samples are stochastically indistinguishable,
    ///   1 means every value in one group exceeds every value in the other.
    /// - Categorical paths: the Jensen-Shannon divergence itself, which is
    ///   already bounded to `[0,1]`.
    ///
    /// Both are 0 for identical distributions and 1 for maximal separation, so
    /// they rank together. They are not the same quantity, and this field is a
    /// magnitude for ordering rather than an estimate of one specific statistic.
    pub effect_size: f64,
}

/// Which distance metric detected the drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftMetric {
    /// Jensen-Shannon Divergence (for categorical/string values).
    JensenShannonDivergence,
    /// Wasserstein distance (for numeric values).
    WassersteinDistance,
}

/// Severity classification for drift between two documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriftSeverity {
    /// No detectable drift.
    None,
    /// Cosmetic changes only (e.g., value distribution shifts below threshold).
    Low,
    /// Additive changes (new paths, no removals or type changes).
    Medium,
    /// Type changes or significant distribution shifts.
    High,
    /// Removed paths or major structural changes.
    Critical,
}

/// A rich drift report with full details beyond the trait's [`DriftReport`].
#[derive(Debug, Clone)]
pub struct FullDriftReport {
    /// Structural path diff.
    pub path_diff: PathDiffResult,
    /// Paths where the dominant type changed.
    pub type_changes: Vec<TypeChange>,
    /// Distributional drift observations above threshold.
    pub distributional_drifts: Vec<DistributionalDrift>,
    /// Jaccard similarity of the two path sets (0.0 = disjoint, 1.0 = identical).
    pub structural_similarity: f64,
    /// Overall severity classification.
    pub severity: DriftSeverity,
}

/// Drift analyzer implementing the [`DriftDetector`] trait.
#[derive(Debug, Clone, Copy, Default)]
pub struct DriftAnalyzer;

impl DriftDetector for DriftAnalyzer {
    fn compare(&self, lhs: &Document, rhs: &Document) -> DriftReport {
        let diff = path_diff(lhs.trie(), rhs.trie());
        let type_changes = compute_type_changes(lhs, rhs, &diff.shared);

        DriftReport {
            added_paths: diff.added.iter().map(|p| p.as_str()).collect(),
            removed_paths: diff.removed.iter().map(|p| p.as_str()).collect(),
            type_changes,
        }
    }
}

/// Compute type changes for shared paths by comparing dominant types.
fn compute_type_changes(
    lhs: &Document,
    rhs: &Document,
    shared: &[WildcardPath],
) -> Vec<TypeChange> {
    let mut changes = Vec::new();

    for path in shared {
        let lhs_node = lhs.trie().get(path);
        let rhs_node = rhs.trie().get(path);

        if let (Some(ln), Some(rn)) = (lhs_node, rhs_node) {
            let lhs_type = ln.metadata.dominant_type();
            let rhs_type = rn.metadata.dominant_type();

            if let (Some(lt), Some(rt)) = (lhs_type, rhs_type) {
                if lt != rt {
                    changes.push(TypeChange {
                        path: path.as_str(),
                        from: lt.to_string(),
                        to: rt.to_string(),
                    });
                }
            }
        }
    }

    changes
}

/// Build per-path value frequency distributions from a document.
fn build_frequency_map(doc: &Document) -> BTreeMap<WildcardPath, BTreeMap<String, u64>> {
    let mut fc = FrequencyCounter::new();
    fc.count_document(doc);
    fc.counts().clone()
}

/// Check whether a path is predominantly numeric in both documents.
fn is_numeric_path(lhs: &Document, rhs: &Document, path: &WildcardPath) -> bool {
    let lhs_type = lhs
        .trie()
        .get(path)
        .and_then(|n| n.metadata.dominant_type());
    let rhs_type = rhs
        .trie()
        .get(path)
        .and_then(|n| n.metadata.dominant_type());

    matches!(
        (lhs_type, rhs_type),
        (
            Some(JsonType::Integer | JsonType::Float),
            Some(JsonType::Integer | JsonType::Float)
        )
    )
}

/// Extract numeric values from a frequency map for a given path.
fn extract_numeric_values(freq: &BTreeMap<String, u64>) -> Vec<f64> {
    let mut values = Vec::new();
    for (key, &count) in freq {
        if let Ok(v) = key.parse::<f64>() {
            for _ in 0..count {
                values.push(v);
            }
        }
    }
    values
}

/// Convert a frequency map to a normalized probability distribution.
///
/// Returns a pair of aligned probability vectors over the union of all keys,
/// using deterministic `BTreeMap` ordering.
fn to_probability_distributions(
    lhs_freq: &BTreeMap<String, u64>,
    rhs_freq: &BTreeMap<String, u64>,
) -> (Vec<f64>, Vec<f64>) {
    // Collect the union of all keys in deterministic order.
    let mut all_keys: BTreeMap<&str, ()> = BTreeMap::new();
    for k in lhs_freq.keys() {
        all_keys.insert(k.as_str(), ());
    }
    for k in rhs_freq.keys() {
        all_keys.insert(k.as_str(), ());
    }

    let lhs_total: u64 = lhs_freq.values().sum();
    let rhs_total: u64 = rhs_freq.values().sum();

    if lhs_total == 0 || rhs_total == 0 {
        return (Vec::new(), Vec::new());
    }

    let lhs_total_f = lhs_total as f64;
    let rhs_total_f = rhs_total as f64;

    let mut p = Vec::with_capacity(all_keys.len());
    let mut q = Vec::with_capacity(all_keys.len());

    for key in all_keys.keys() {
        let lhs_count = lhs_freq.get(*key).copied().unwrap_or(0);
        let rhs_count = rhs_freq.get(*key).copied().unwrap_or(0);
        p.push(lhs_count as f64 / lhs_total_f);
        q.push(rhs_count as f64 / rhs_total_f);
    }

    (p, q)
}

/// Compute distributional drifts for shared paths.
fn compute_distributional_drifts(
    lhs: &Document,
    rhs: &Document,
    shared: &[WildcardPath],
    lhs_freq: &BTreeMap<WildcardPath, BTreeMap<String, u64>>,
    rhs_freq: &BTreeMap<WildcardPath, BTreeMap<String, u64>>,
) -> Vec<DistributionalDrift> {
    let mut drifts = Vec::new();

    for path in shared {
        let lhs_f = lhs_freq.get(path);
        let rhs_f = rhs_freq.get(path);

        // Skip paths without frequency data (containers).
        let (Some(lf), Some(rf)) = (lhs_f, rhs_f) else {
            continue;
        };

        if lf.is_empty() || rf.is_empty() {
            continue;
        }

        if is_numeric_path(lhs, rhs, path) {
            // Use Wasserstein distance for numeric paths.
            let mut a = extract_numeric_values(lf);
            let mut b = extract_numeric_values(rf);

            if !a.is_empty() && !b.is_empty() {
                let dist = wasserstein_1d(&mut a, &mut b);
                if dist > WASSERSTEIN_THRESHOLD {
                    // Wasserstein is in field units; Cliff's delta gives a
                    // unit-free magnitude that ranks against JSD.
                    let delta = cliffs_delta(&mut a, &mut b);
                    drifts.push(DistributionalDrift {
                        path: path.as_str(),
                        metric: DriftMetric::WassersteinDistance,
                        value: dist,
                        effect_size: delta.abs(),
                    });
                }
            }
        } else {
            // Use JSD for categorical paths.
            let (p, q) = to_probability_distributions(lf, rf);

            if !p.is_empty() {
                if let Ok(jsd) = jensen_shannon_divergence(&p, &q) {
                    if jsd > JSD_THRESHOLD {
                        drifts.push(DistributionalDrift {
                            path: path.as_str(),
                            metric: DriftMetric::JensenShannonDivergence,
                            // JSD is already bounded to [0,1].
                            value: jsd,
                            effect_size: jsd,
                        });
                    }
                }
            }
        }
    }

    drifts
}

/// Classify the overall drift severity based on the collected evidence.
fn classify_severity(report: &FullDriftReport) -> DriftSeverity {
    // Critical: any removed paths (excluding root, which is always shared).
    if !report.path_diff.removed.is_empty() {
        return DriftSeverity::Critical;
    }

    // High: type changes or significant distributional drifts.
    if !report.type_changes.is_empty() {
        return DriftSeverity::High;
    }

    // High: large distributional drifts.
    let has_major_drift = report.distributional_drifts.iter().any(|d| match d.metric {
        DriftMetric::JensenShannonDivergence => d.value > 0.3,
        DriftMetric::WassersteinDistance => d.value > 1.0,
    });
    if has_major_drift {
        return DriftSeverity::High;
    }

    // Medium: added paths.
    if !report.path_diff.added.is_empty() {
        return DriftSeverity::Medium;
    }

    // Low: minor distributional drifts detected.
    if !report.distributional_drifts.is_empty() {
        return DriftSeverity::Low;
    }

    // None: no drift detected.
    DriftSeverity::None
}

/// Perform a full drift analysis between two documents.
///
/// This produces a [`FullDriftReport`] with structural path diff,
/// type changes, distributional drift metrics, structural similarity,
/// and an overall severity classification.
#[must_use]
pub fn full_drift(lhs: &Document, rhs: &Document) -> FullDriftReport {
    let diff = path_diff(lhs.trie(), rhs.trie());
    let type_changes = compute_type_changes(lhs, rhs, &diff.shared);
    let structural_similarity = jaccard_similarity(lhs.trie(), rhs.trie());

    let lhs_freq = build_frequency_map(lhs);
    let rhs_freq = build_frequency_map(rhs);
    let distributional_drifts =
        compute_distributional_drifts(lhs, rhs, &diff.shared, &lhs_freq, &rhs_freq);

    let mut report = FullDriftReport {
        path_diff: diff,
        type_changes,
        distributional_drifts,
        structural_similarity,
        severity: DriftSeverity::None, // placeholder
    };

    report.severity = classify_severity(&report);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
    }

    #[test]
    fn identical_docs_no_drift() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(r#"{"name": "Alice", "age": 30}"#)?;
        let report = full_drift(&doc, &doc);

        assert!(report.path_diff.added.is_empty());
        assert!(report.path_diff.removed.is_empty());
        assert!(report.type_changes.is_empty());
        assert!(report.distributional_drifts.is_empty());
        assert_eq!(report.severity, DriftSeverity::None);
        Ok(())
    }

    #[test]
    fn identical_docs_similarity_one() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(r#"{"a": 1, "b": 2}"#)?;
        let report = full_drift(&doc, &doc);
        assert!(
            (report.structural_similarity - 1.0).abs() < f64::EPSILON,
            "identical docs should have similarity 1.0, got {}",
            report.structural_similarity
        );
        Ok(())
    }

    #[test]
    fn added_field_medium_severity() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse(r#"{"a": 1}"#)?;
        let rhs = parse(r#"{"a": 1, "b": 2}"#)?;
        let report = full_drift(&lhs, &rhs);

        assert_eq!(report.path_diff.added.len(), 1);
        assert!(report.path_diff.removed.is_empty());
        assert_eq!(report.severity, DriftSeverity::Medium);
        Ok(())
    }

    #[test]
    fn removed_field_critical_severity() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse(r#"{"a": 1, "b": 2}"#)?;
        let rhs = parse(r#"{"a": 1}"#)?;
        let report = full_drift(&lhs, &rhs);

        assert!(report.path_diff.added.is_empty());
        assert_eq!(report.path_diff.removed.len(), 1);
        assert_eq!(report.severity, DriftSeverity::Critical);
        Ok(())
    }

    #[test]
    fn type_change_high_severity() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse(r#"{"x": 42}"#)?;
        let rhs = parse(r#"{"x": "hello"}"#)?;
        let report = full_drift(&lhs, &rhs);

        assert_eq!(report.type_changes.len(), 1);
        assert_eq!(report.type_changes[0].path, "$.x");
        assert_eq!(report.type_changes[0].from, "integer");
        assert_eq!(report.type_changes[0].to, "string");
        assert_eq!(report.severity, DriftSeverity::High);
        Ok(())
    }

    #[test]
    fn disjoint_docs_similarity_low() -> Result<(), Box<dyn std::error::Error>> {
        // Both have root ($) in common, but all other paths differ.
        let lhs = parse(r#"{"a": 1}"#)?;
        let rhs = parse(r#"{"b": 2}"#)?;
        let report = full_drift(&lhs, &rhs);

        // Paths: lhs has {$, $.a}, rhs has {$, $.b}
        // intersection = {$} = 1, union = {$, $.a, $.b} = 3
        // Jaccard = 1/3
        assert!(
            (report.structural_similarity - 1.0 / 3.0).abs() < 1e-12,
            "expected ~0.333, got {}",
            report.structural_similarity
        );
        Ok(())
    }

    #[test]
    fn drift_detector_trait_impl() -> Result<(), Box<dyn std::error::Error>> {
        let analyzer = DriftAnalyzer;
        let lhs = parse(r#"{"a": 1, "b": 2}"#)?;
        let rhs = parse(r#"{"a": 1, "c": 3}"#)?;
        let report = analyzer.compare(&lhs, &rhs);

        assert_eq!(report.added_paths, vec!["$.c"]);
        assert_eq!(report.removed_paths, vec!["$.b"]);
        Ok(())
    }

    #[test]
    fn distributional_drift_jsd_categorical() -> Result<(), Box<dyn std::error::Error>> {
        // Two arrays with different value distributions.
        let lhs = parse(r#"["a", "a", "a", "b"]"#)?;
        let rhs = parse(r#"["b", "b", "b", "a"]"#)?;
        let report = full_drift(&lhs, &rhs);

        // The distributions at $[*] are [0.75, 0.25] vs [0.25, 0.75] (for "a", "b").
        // JSD should be nonzero.
        let jsd_drifts: Vec<_> = report
            .distributional_drifts
            .iter()
            .filter(|d| d.metric == DriftMetric::JensenShannonDivergence)
            .collect();

        assert!(
            !jsd_drifts.is_empty(),
            "should detect JSD drift for different categorical distributions"
        );
        Ok(())
    }

    #[test]
    fn no_distributional_drift_identical_values() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(r#"["x", "y", "z"]"#)?;
        let report = full_drift(&doc, &doc);
        assert!(report.distributional_drifts.is_empty());
        Ok(())
    }

    #[test]
    fn numeric_wasserstein_drift() -> Result<(), Box<dyn std::error::Error>> {
        // Numeric values shifted significantly.
        let lhs = parse(r#"[1, 2, 3, 4, 5]"#)?;
        let rhs = parse(r#"[101, 102, 103, 104, 105]"#)?;
        let report = full_drift(&lhs, &rhs);

        let w_drifts: Vec<_> = report
            .distributional_drifts
            .iter()
            .filter(|d| d.metric == DriftMetric::WassersteinDistance)
            .collect();

        assert!(
            !w_drifts.is_empty(),
            "should detect Wasserstein drift for shifted numeric distributions"
        );
        // The Wasserstein distance should be ~100.
        assert!(
            w_drifts[0].value > 90.0,
            "expected large Wasserstein distance, got {}",
            w_drifts[0].value
        );
        Ok(())
    }

    #[test]
    fn probability_distribution_construction() {
        let mut lhs_freq = BTreeMap::new();
        lhs_freq.insert("a".to_owned(), 3_u64);
        lhs_freq.insert("b".to_owned(), 1);

        let mut rhs_freq = BTreeMap::new();
        rhs_freq.insert("a".to_owned(), 1_u64);
        rhs_freq.insert("c".to_owned(), 1);

        let (p, q) = to_probability_distributions(&lhs_freq, &rhs_freq);

        // Keys union: a, b, c (3 elements).
        assert_eq!(p.len(), 3);
        assert_eq!(q.len(), 3);

        // p: a=3/4, b=1/4, c=0/4
        assert!((p[0] - 0.75).abs() < 1e-12);
        assert!((p[1] - 0.25).abs() < 1e-12);
        assert!((p[2] - 0.0).abs() < 1e-12);

        // q: a=1/2, b=0/2, c=1/2
        assert!((q[0] - 0.5).abs() < 1e-12);
        assert!((q[1] - 0.0).abs() < 1e-12);
        assert!((q[2] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn empty_docs_no_crash() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse("{}")?;
        let rhs = parse("{}")?;
        let report = full_drift(&lhs, &rhs);
        assert_eq!(report.severity, DriftSeverity::None);
        assert!((report.structural_similarity - 1.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn complex_nested_drift() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse(
            r#"{
                "users": [
                    {"name": "Alice", "score": 95},
                    {"name": "Bob", "score": 88}
                ]
            }"#,
        )?;
        let rhs = parse(
            r#"{
                "users": [
                    {"name": "Alice", "score": 95},
                    {"name": "Charlie", "score": 72}
                ],
                "version": 2
            }"#,
        )?;
        let report = full_drift(&lhs, &rhs);

        // $.version is added
        let added_strs: Vec<String> = report.path_diff.added.iter().map(|p| p.as_str()).collect();
        assert!(added_strs.contains(&"$.version".to_owned()));

        // Severity should be at least Medium (added paths)
        assert!(report.severity >= DriftSeverity::Medium);
        Ok(())
    }

    #[test]
    fn severity_ordering() {
        assert!(DriftSeverity::None < DriftSeverity::Low);
        assert!(DriftSeverity::Low < DriftSeverity::Medium);
        assert!(DriftSeverity::Medium < DriftSeverity::High);
        assert!(DriftSeverity::High < DriftSeverity::Critical);
    }

    // -----------------------------------------------------------------------
    // Cliff's delta
    // -----------------------------------------------------------------------

    const DELTA_EPS: f64 = 1e-12;

    fn delta(a: &[f64], b: &[f64]) -> f64 {
        let mut av = a.to_vec();
        let mut bv = b.to_vec();
        cliffs_delta(&mut av, &mut bv)
    }

    #[test]
    fn cliffs_delta_identical_samples_is_zero() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!(delta(&s, &s).abs() < DELTA_EPS);
    }

    #[test]
    fn cliffs_delta_complete_separation_is_one() {
        let a = [10.0, 11.0, 12.0];
        let b = [1.0, 2.0, 3.0];
        assert!(
            (delta(&a, &b) - 1.0).abs() < DELTA_EPS,
            "a strictly above b"
        );
        assert!((delta(&b, &a) + 1.0).abs() < DELTA_EPS, "sign reverses");
    }

    /// Hand-computed: a=[1,2], b=[1,2,3] gives 1 greater pair and 3 lesser,
    /// over 6 pairs, so delta = -1/3.
    #[test]
    fn cliffs_delta_matches_hand_computation() {
        let d = delta(&[1.0, 2.0], &[1.0, 2.0, 3.0]);
        assert!((d + 1.0 / 3.0).abs() < DELTA_EPS, "expected -1/3, got {d}");
    }

    /// All values tied across both samples: no pair is greater or lesser.
    #[test]
    fn cliffs_delta_all_ties_is_zero() {
        let d = delta(&[5.0, 5.0, 5.0], &[5.0, 5.0]);
        assert!(d.abs() < DELTA_EPS, "expected 0 for all ties, got {d}");
    }

    #[test]
    fn cliffs_delta_empty_sample_is_zero() {
        assert!(delta(&[], &[1.0, 2.0]).abs() < DELTA_EPS);
        assert!(delta(&[1.0, 2.0], &[]).abs() < DELTA_EPS);
    }

    #[test]
    fn cliffs_delta_is_bounded() {
        let a: Vec<f64> = (0..50).map(f64::from).collect();
        let b: Vec<f64> = (25..90).map(f64::from).collect();
        let d = delta(&a, &b);
        assert!((-1.0..=1.0).contains(&d), "delta out of range: {d}");
    }

    /// Order of the input slices must not matter beyond the sign.
    #[test]
    fn cliffs_delta_is_antisymmetric() {
        let a = [1.0, 5.0, 9.0, 2.0];
        let b = [4.0, 4.0, 7.0];
        assert!((delta(&a, &b) + delta(&b, &a)).abs() < DELTA_EPS);
    }

    // -----------------------------------------------------------------------
    // effect_size reporting
    // -----------------------------------------------------------------------

    /// Every reported drift must carry a unit-free effect size in [0,1], even
    /// when the raw metric value is orders of magnitude larger.
    #[test]
    fn effect_size_is_bounded_and_present() -> Result<(), Box<dyn std::error::Error>> {
        // `bytes` differs by ~100000 units; `flag` differs on a 0/1 scale.
        let lhs = parse(
            r#"[{"bytes": 10, "flag": 0}, {"bytes": 20, "flag": 0}, {"bytes": 30, "flag": 0}]"#,
        )?;
        let rhs = parse(
            r#"[{"bytes": 100010, "flag": 1}, {"bytes": 100020, "flag": 1}, {"bytes": 100030, "flag": 1}]"#,
        )?;
        let report = full_drift(&lhs, &rhs);
        assert!(
            !report.distributional_drifts.is_empty(),
            "expected drift on both paths"
        );
        for dd in &report.distributional_drifts {
            assert!(
                (0.0..=1.0).contains(&dd.effect_size),
                "{} effect_size {} out of [0,1]",
                dd.path,
                dd.effect_size
            );
        }
        let bytes = report
            .distributional_drifts
            .iter()
            .find(|d| d.path.contains("bytes"))
            .ok_or("missing bytes drift")?;
        assert!(
            bytes.value > 1000.0,
            "raw Wasserstein should be large: {}",
            bytes.value
        );
        assert!(
            (bytes.effect_size - 1.0).abs() < 1e-9,
            "disjoint numeric samples should have effect_size 1, got {}",
            bytes.effect_size
        );
        Ok(())
    }

    /// For categorical paths the effect size *is* the JSD, which is already
    /// bounded, so the two must agree exactly.
    #[test]
    fn categorical_effect_size_equals_jsd() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = parse(r#"[{"k": "a"}, {"k": "a"}, {"k": "b"}]"#)?;
        let rhs = parse(r#"[{"k": "c"}, {"k": "c"}, {"k": "d"}]"#)?;
        let report = full_drift(&lhs, &rhs);
        let cat = report
            .distributional_drifts
            .iter()
            .find(|d| d.metric == DriftMetric::JensenShannonDivergence)
            .ok_or("missing categorical drift")?;
        assert!((cat.effect_size - cat.value).abs() < DELTA_EPS);
        Ok(())
    }

    /// The ranking fix: a large-unit path must not outrank a genuinely more
    /// separated small-unit path once effect_size is used.
    #[test]
    fn effect_size_reorders_large_unit_paths() -> Result<(), Box<dyn std::error::Error>> {
        // `big` shifts a lot in absolute terms but the samples overlap heavily.
        // `small` shifts little in absolute terms but separates completely.
        let lhs = parse(
            r#"[{"big": 0, "small": 0.0}, {"big": 1000, "small": 0.1},
                {"big": 2000, "small": 0.2}, {"big": 3000, "small": 0.3}]"#,
        )?;
        let rhs = parse(
            r#"[{"big": 500, "small": 1.0}, {"big": 1500, "small": 1.1},
                {"big": 2500, "small": 1.2}, {"big": 3500, "small": 1.3}]"#,
        )?;
        let report = full_drift(&lhs, &rhs);
        let get = |name: &str| {
            report
                .distributional_drifts
                .iter()
                .find(|d| d.path.contains(name))
        };
        let big = get("big").ok_or("missing big")?;
        let small = get("small").ok_or("missing small")?;

        assert!(
            big.value > small.value,
            "raw values rank `big` first: {} vs {}",
            big.value,
            small.value
        );
        assert!(
            small.effect_size > big.effect_size,
            "effect_size must rank `small` first: {} vs {}",
            small.effect_size,
            big.effect_size
        );
        Ok(())
    }
}
