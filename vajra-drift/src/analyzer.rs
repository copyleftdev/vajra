//! Full drift analysis between two JSON documents.
//!
//! Combines structural path diffing, type change detection,
//! distributional drift metrics (JSD and Wasserstein), and
//! severity classification into a unified drift report.

use std::collections::BTreeMap;

use vajra_stats::FrequencyCounter;
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
    /// The metric value.
    pub value: f64,
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
                    drifts.push(DistributionalDrift {
                        path: path.as_str(),
                        metric: DriftMetric::WassersteinDistance,
                        value: dist,
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
                            value: jsd,
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
}
