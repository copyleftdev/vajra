//! Labelled feature evaluation: how well does each field separate the classes?
//!
//! Given a corpus carrying a ground-truth label, the first question is always
//! "which fields actually distinguish the classes, and how well?" Every
//! ingredient already exists elsewhere in this crate — this module assembles
//! them into one ranked, comparable report.
//!
//! Three deliberate choices, all learned from getting them wrong first:
//!
//! - **Mutual information is the universal ranking key.** It is symmetric, in
//!   bits, and defined for every field type, so it ranks numeric and
//!   categorical features on one scale.
//! - **AUC and Cliff's delta are reported only where they are defined**:
//!   numeric fields against a two-class label. Inventing an ordering for
//!   unordered categories would produce a comparable-looking number that means
//!   nothing.
//! - **Class balance and baseline entropy are always reported.** Separation
//!   figures are uninterpretable without them, and precision computed on a
//!   corpus does not transfer to a population with a different prevalence.

use std::collections::{BTreeMap, BTreeSet};

use vajra_types::Document;

use crate::entropy::shannon_entropy_from_counts;
use crate::relationships::{bin_values, collect_scalars, extract_records, BinStrategy};
use vajra_types::path::WildcardPath;

/// Bucket standing in for "this field was not present on this record".
///
/// Absence is evaluated as a value rather than by dropping the row. Dropping
/// would give each feature its own subset of records — and therefore its own
/// label balance and its own achievable MI ceiling — making the numbers
/// incomparable across features, which is the whole point of this report.
/// Absence is also genuinely informative in its own right.
const ABSENT: &str = "(absent)";

/// Cliff's delta between two numeric samples, in `[-1, 1]`.
///
/// delta = ( #{(a,b) : a > b} - #{(a,b) : a < b} ) / (|A| * |B|)
///
/// Relates to the AUC of the two samples as `delta = 2 * AUC - 1`. Computed
/// from sorted values with advancing cursors, so it is O(n log n) rather than
/// the O(n*m) pairwise definition. Returns 0.0 for an empty sample, matching
/// "no detectable difference".
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn cliffs_delta(a: &mut [f64], b: &mut [f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    // For each value in `a`, count how many of `b` are strictly below and
    // strictly above it by advancing two cursors over the sorted `b`.
    let mut below = 0usize;
    let mut at_or_below = 0usize;
    let mut greater = 0u128;
    let mut less = 0u128;

    for &av in a.iter() {
        while below < b.len() && b[below] < av {
            below += 1;
        }
        if at_or_below < below {
            at_or_below = below;
        }
        while at_or_below < b.len() && b[at_or_below] <= av {
            at_or_below += 1;
        }
        greater += below as u128;
        less += (b.len() - at_or_below) as u128;
    }

    let total = (a.len() as u128) * (b.len() as u128);
    if total == 0 {
        return 0.0;
    }
    let delta = (greater as f64 - less as f64) / total as f64;
    delta.clamp(-1.0, 1.0)
}

/// Whether a field's values admit an ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Every observed value parsed as a finite number.
    Numeric,
    /// Unordered values — AUC and Cliff's delta are undefined.
    Categorical,
}

impl FieldKind {
    /// Stable string form for output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Categorical => "categorical",
        }
    }
}

/// The best single decision rule found for one field, and what it would cost.
#[derive(Debug, Clone)]
pub struct OperatingPoint {
    /// Human-readable rule, e.g. `>= 42` or `== "denied"`.
    pub rule: String,
    /// True-positive rate at that rule.
    pub tpr: f64,
    /// False-positive rate at that rule.
    pub fpr: f64,
    /// Youden's J = TPR - FPR. The criterion the rule was chosen by.
    pub youden_j: f64,
    /// Precision at the assumed prevalence, if one was supplied.
    pub precision_at_base_rate: Option<f64>,
}

/// How well one field separates the classes.
#[derive(Debug, Clone)]
pub struct FeatureSeparation {
    /// The field.
    pub path: String,
    /// Whether the field is ordered.
    pub kind: FieldKind,
    /// Observations of this field.
    pub count: usize,
    /// Distinct observed values.
    pub distinct_values: usize,
    /// Fraction of labelled records where the field was present.
    ///
    /// AUC uses only these rows, so a low coverage means the AUC describes a
    /// subset. The entropy measures cover every record, treating absence as a
    /// value.
    pub coverage: f64,
    /// Whether the field's values were discretised for the entropy measures.
    pub binned: bool,
    /// Area under the ROC curve. `None` unless numeric with a two-class label.
    pub auc: Option<f64>,
    /// |2*AUC - 1| — unit-free, 0 = indistinguishable, 1 = perfect. `None`
    /// under the same conditions as `auc`.
    pub separation: Option<f64>,
    /// H(label | field), in bits. Always defined.
    pub conditional_entropy: f64,
    /// 1 - H(label|field)/H(label), clamped to [0,1]. Always defined.
    pub relationship_strength: f64,
    /// I(field; label), in bits. Always defined — **the cross-type ranking key**.
    pub mutual_information: f64,
    /// Best single-rule operating point, when the label has two classes.
    pub operating_point: Option<OperatingPoint>,
}

/// A labelled-separation report over a document.
#[derive(Debug, Clone)]
pub struct SeparationReport {
    /// The label field analysed.
    pub label_field: String,
    /// Records that carried a label.
    pub labelled_records: usize,
    /// Class -> count, sorted by class name.
    pub classes: BTreeMap<String, usize>,
    /// H(label), in bits — the uncertainty any feature is trying to reduce.
    pub baseline_entropy: f64,
    /// Whether the label has exactly two classes. AUC-based columns require it.
    pub binary: bool,
    /// The positive class, when binary: whichever name sorts first.
    pub positive_class: Option<String>,
    /// Assumed prevalence used for `precision_at_base_rate`, if supplied.
    pub base_rate: Option<f64>,
    /// Features, ranked by mutual information descending.
    pub features: Vec<FeatureSeparation>,
}

/// Errors that prevent a separation analysis.
#[derive(Debug, thiserror::Error)]
pub enum SeparationError {
    /// The document held no records to analyse.
    #[error(
        "no records found: separation analysis needs repeated objects (e.g. an array or NDJSON)"
    )]
    NoRecords,
    /// No record carried the label field.
    #[error("label field '{0}' not found in any record")]
    LabelMissing(String),
    /// A label with one class carries no information to explain.
    #[error("label field '{0}' has only one class ('{1}'): nothing to separate")]
    SingleClass(String, String),
    /// The supplied prevalence is not a probability.
    #[error("base rate must be in (0.0, 1.0), got {0}")]
    InvalidBaseRate(f64),
    /// The requested positive class is not one of the observed classes.
    #[error("positive class '{0}' not found; observed classes: {1:?}")]
    UnknownPositiveClass(String, Vec<String>),
}

/// Evaluate how well every scalar field separates the classes of `label_field`.
///
/// `label_field` is matched against the trailing component of each record's
/// paths, so both `label` and `$.label` select a top-level `label` key.
///
/// # Errors
///
/// Returns [`SeparationError`] if there are no records, the label is absent or
/// constant, or `base_rate` is not a probability.
pub fn separation_analysis(
    doc: &Document,
    label_field: &str,
    base_rate: Option<f64>,
    positive: Option<&str>,
) -> Result<SeparationReport, SeparationError> {
    if let Some(rate) = base_rate {
        if !(rate > 0.0 && rate < 1.0) {
            return Err(SeparationError::InvalidBaseRate(rate));
        }
    }

    let records = extract_records(doc.value());
    if records.is_empty() {
        return Err(SeparationError::NoRecords);
    }

    let wanted = label_field.trim_start_matches("$.");

    // Per-record (label, field values). Records without a label are skipped:
    // an unlabelled record cannot contribute to a supervised comparison.
    let mut labels: Vec<String> = Vec::new();
    let mut columns: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();

    for record in &records {
        let mut values: BTreeMap<WildcardPath, String> = BTreeMap::new();
        collect_scalars(record, &WildcardPath::root(), &mut values);

        let label = values
            .iter()
            .find(|(path, _)| path_matches(&path.as_str(), wanted))
            .map(|(_, v)| v.clone());
        let Some(label) = label else { continue };

        labels.push(label);
        let row_index = labels.len() - 1;
        for (path, value) in values {
            let key = path.as_str();
            if path_matches(&key, wanted) {
                continue;
            }
            let column = columns.entry(key).or_default();
            // Backfill records where this field was absent.
            while column.len() < row_index {
                column.push(None);
            }
            column.push(Some(value));
        }
        for column in columns.values_mut() {
            while column.len() < labels.len() {
                column.push(None);
            }
        }
    }

    if labels.is_empty() {
        return Err(SeparationError::LabelMissing(label_field.to_owned()));
    }

    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    for label in &labels {
        *classes.entry(label.clone()).or_insert(0) += 1;
    }
    if classes.len() < 2 {
        let only = classes.keys().next().cloned().unwrap_or_default();
        return Err(SeparationError::SingleClass(label_field.to_owned(), only));
    }

    let class_counts: Vec<u64> = classes.values().map(|&c| c as u64).collect();
    let baseline_entropy = shannon_entropy_from_counts(&class_counts);
    let binary = classes.len() == 2;
    // The positive class decides what TPR, FPR and precision *mean*, so it must
    // not be left to alphabetical accident when the caller cares. Default to
    // the first class by name for determinism; `positive` overrides.
    let positive_class = match (binary, positive) {
        (_, Some(name)) => {
            if !classes.contains_key(name) {
                return Err(SeparationError::UnknownPositiveClass(
                    name.to_owned(),
                    classes.keys().cloned().collect(),
                ));
            }
            Some(name.to_owned())
        }
        (true, None) => classes.keys().next().cloned(),
        (false, None) => None,
    };

    let mut features: Vec<FeatureSeparation> = columns
        .into_iter()
        .filter_map(|(path, values)| {
            evaluate_feature(
                &path,
                &values,
                &labels,
                baseline_entropy,
                positive_class.as_deref(),
                base_rate,
            )
        })
        .collect();

    // Mutual information descending, then path, so ordering is fully specified.
    features.sort_by(|a, b| {
        b.mutual_information
            .partial_cmp(&a.mutual_information)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    Ok(SeparationReport {
        label_field: label_field.to_owned(),
        labelled_records: labels.len(),
        classes,
        baseline_entropy,
        binary,
        positive_class,
        base_rate,
        features,
    })
}

/// Whether a JSONPath's trailing key equals `wanted`.
fn path_matches(path: &str, wanted: &str) -> bool {
    path.rsplit('.').next().is_some_and(|last| last == wanted)
}

#[allow(clippy::cast_precision_loss)]
fn evaluate_feature(
    path: &str,
    values: &[Option<String>],
    labels: &[String],
    baseline_entropy: f64,
    positive_class: Option<&str>,
    base_rate: Option<f64>,
) -> Option<FeatureSeparation> {
    // Rows where the field was observed, for AUC and the numeric view.
    let observed: Vec<(&str, &str)> = values
        .iter()
        .zip(labels.iter())
        .filter_map(|(v, l)| v.as_deref().map(|v| (v, l.as_str())))
        .collect();
    if observed.is_empty() {
        return None;
    }
    let coverage = observed.len() as f64 / labels.len() as f64;
    let distinct: BTreeSet<&str> = observed.iter().map(|(v, _)| *v).collect();

    // Numeric only if every observed value parses as a finite number.
    let numeric: Option<Vec<f64>> = observed
        .iter()
        .map(|(v, _)| v.parse::<f64>().ok().filter(|f| f.is_finite()))
        .collect();
    let kind = if numeric.is_some() {
        FieldKind::Numeric
    } else {
        FieldKind::Categorical
    };

    // Entropy measures run over *every* labelled record, with absence as a
    // value and numeric fields discretised. Without the first, each feature
    // gets its own label balance and MI ceiling; without the second, a
    // near-unique continuous field trivially "determines" the label.
    let observed_strings: Vec<String> = observed.iter().map(|(v, _)| (*v).to_owned()).collect();
    let binned_observed = bin_values(&observed_strings, BinStrategy::default());
    let binned = binned_observed.is_some();
    let mut cursor = 0usize;
    let full: Vec<(String, &str)> = values
        .iter()
        .zip(labels.iter())
        .map(|(v, l)| {
            let cell = if v.is_some() {
                let out = binned_observed
                    .as_ref()
                    .map_or_else(|| observed_strings[cursor].clone(), |b| b[cursor].clone());
                cursor += 1;
                out
            } else {
                ABSENT.to_owned()
            };
            (cell, l.as_str())
        })
        .collect();

    let mut joint: BTreeMap<(&str, &str), u64> = BTreeMap::new();
    let mut feature_counts: BTreeMap<&str, u64> = BTreeMap::new();
    for (v, l) in &full {
        *joint.entry((v.as_str(), *l)).or_insert(0) += 1;
        *feature_counts.entry(v.as_str()).or_insert(0) += 1;
    }
    let total = full.len() as f64;
    let mut conditional_entropy = 0.0_f64;
    for ((v, _), &count) in &joint {
        let fc = feature_counts.get(v).copied().unwrap_or(0);
        if fc == 0 || count == 0 {
            continue;
        }
        let p_joint = count as f64 / total;
        let p_cond = count as f64 / fc as f64;
        conditional_entropy -= p_joint * p_cond.log2();
    }
    conditional_entropy = conditional_entropy.max(0.0);

    // baseline_entropy is H(label) over all labelled records — the same
    // denominator for every feature, which is what makes these comparable.
    let mutual_information = (baseline_entropy - conditional_entropy).max(0.0);
    let relationship_strength = if baseline_entropy > 0.0 {
        (1.0 - conditional_entropy / baseline_entropy).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let pairs = observed;

    let mut auc = None;
    let mut separation = None;
    if let (Some(nums), Some(pos)) = (numeric.as_ref(), positive_class) {
        let mut positives: Vec<f64> = Vec::new();
        let mut negatives: Vec<f64> = Vec::new();
        for (value, (_, label)) in nums.iter().zip(pairs.iter()) {
            if *label == pos {
                positives.push(*value);
            } else {
                negatives.push(*value);
            }
        }
        if !positives.is_empty() && !negatives.is_empty() {
            let delta = cliffs_delta(&mut positives, &mut negatives);
            auc = Some((delta + 1.0) / 2.0);
            separation = Some(delta.abs());
        }
    }

    let operating_point = positive_class
        .and_then(|pos| best_operating_point(&pairs, numeric.as_deref(), pos, base_rate));

    Some(FeatureSeparation {
        path: path.to_owned(),
        kind,
        count: pairs.len(),
        distinct_values: distinct.len(),
        coverage,
        binned,
        auc,
        separation,
        conditional_entropy,
        relationship_strength,
        mutual_information,
        operating_point,
    })
}

/// Find the single rule that maximises Youden's J, and price it.
///
/// A separation score says how much signal a field carries; it does not say what
/// acting on it would cost. This computes the best available decision rule and,
/// given an assumed prevalence, the precision it would actually deliver — which
/// is usually far lower than a balanced corpus suggests.
#[allow(clippy::cast_precision_loss)]
fn best_operating_point(
    pairs: &[(&str, &str)],
    numeric: Option<&[f64]>,
    positive_class: &str,
    base_rate: Option<f64>,
) -> Option<OperatingPoint> {
    let n_pos = pairs.iter().filter(|(_, l)| *l == positive_class).count();
    let n_neg = pairs.len() - n_pos;
    if n_pos == 0 || n_neg == 0 {
        return None;
    }

    let price = |tpr: f64, fpr: f64, rule: String| {
        let precision_at_base_rate = base_rate.map(|p| {
            let tp = tpr * p;
            let fp = fpr * (1.0 - p);
            if tp + fp > 0.0 {
                tp / (tp + fp)
            } else {
                0.0
            }
        });
        OperatingPoint {
            rule,
            tpr,
            fpr,
            youden_j: tpr - fpr,
            precision_at_base_rate,
        }
    };

    // Candidates are ranked by |J| because a strongly *negative* J is just as
    // informative — the field predicts the negative class. But reporting such a
    // rule as-is would hand the caller a rule worse than doing the opposite, so
    // the complement is emitted instead, with the rates swapped accordingly.
    let mut best: Option<(OperatingPoint, String)> = None;
    let mut consider = |candidate: OperatingPoint, complement: String| {
        let better = best
            .as_ref()
            .is_none_or(|(b, _)| candidate.youden_j.abs() > b.youden_j.abs());
        if better {
            best = Some((candidate, complement));
        }
    };

    if let Some(nums) = numeric {
        // Sweep the distinct observed values as `>=` thresholds.
        let thresholds: BTreeSet<String> = nums.iter().map(|v| format!("{v:?}")).collect();
        let mut cuts: Vec<f64> = thresholds.iter().filter_map(|s| s.parse().ok()).collect();
        cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for cut in cuts {
            let mut tp = 0usize;
            let mut fp = 0usize;
            for (value, (_, label)) in nums.iter().zip(pairs.iter()) {
                if *value >= cut {
                    if *label == positive_class {
                        tp += 1;
                    } else {
                        fp += 1;
                    }
                }
            }
            consider(
                price(
                    tp as f64 / n_pos as f64,
                    fp as f64 / n_neg as f64,
                    format!(">= {cut}"),
                ),
                format!("< {cut}"),
            );
        }
    } else {
        // Each distinct value as a `==` indicator.
        let values: BTreeSet<&str> = pairs.iter().map(|(v, _)| *v).collect();
        for value in values {
            let mut tp = 0usize;
            let mut fp = 0usize;
            for (v, label) in pairs {
                if v == &value {
                    if *label == positive_class {
                        tp += 1;
                    } else {
                        fp += 1;
                    }
                }
            }
            consider(
                price(
                    tp as f64 / n_pos as f64,
                    fp as f64 / n_neg as f64,
                    format!("== \"{value}\""),
                ),
                format!("!= \"{value}\""),
            );
        }
    }

    let (point, complement) = best?;
    if point.youden_j >= 0.0 {
        return Some(point);
    }
    // Invert: the complement of a rule has the complementary rates.
    let tpr = 1.0 - point.tpr;
    let fpr = 1.0 - point.fpr;
    let precision_at_base_rate = base_rate.map(|p| {
        let tp = tpr * p;
        let fp = fpr * (1.0 - p);
        if tp + fp > 0.0 {
            tp / (tp + fp)
        } else {
            0.0
        }
    });
    Some(OperatingPoint {
        rule: complement,
        tpr,
        fpr,
        youden_j: tpr - fpr,
        precision_at_base_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn parse(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
    }

    /// `score` rises with the positive class; `noise` is unrelated.
    fn labelled_doc() -> String {
        let rows: Vec<String> = (0..200)
            .map(|i| {
                let positive = i % 2 == 0;
                let score = if positive { 60 + i % 40 } else { i % 40 };
                let noise = i % 7;
                let label = if positive { "aaa_pos" } else { "zzz_neg" };
                format!(r#"{{"score": {score}, "noise": {noise}, "label": "{label}"}}"#)
            })
            .collect();
        format!("[{}]", rows.join(","))
    }

    #[test]
    fn reports_class_balance_and_baseline_entropy() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", None, None)?;

        assert_eq!(report.labelled_records, 200);
        assert_eq!(report.classes.get("aaa_pos"), Some(&100));
        assert_eq!(report.classes.get("zzz_neg"), Some(&100));
        // Balanced binary label -> exactly 1 bit.
        assert!((report.baseline_entropy - 1.0).abs() < EPS);
        assert!(report.binary);
        assert_eq!(report.positive_class.as_deref(), Some("aaa_pos"));
        Ok(())
    }

    #[test]
    fn separating_feature_outranks_noise() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", None, None)?;

        let score = report
            .features
            .iter()
            .find(|f| f.path.ends_with("score"))
            .ok_or("missing score")?;
        let noise = report
            .features
            .iter()
            .find(|f| f.path.ends_with("noise"))
            .ok_or("missing noise")?;

        assert!(
            score.mutual_information > noise.mutual_information,
            "score {} should beat noise {}",
            score.mutual_information,
            noise.mutual_information
        );
        // Ranked by MI, so `score` comes first.
        assert!(report.features[0].path.ends_with("score"));
        Ok(())
    }

    #[test]
    fn numeric_feature_gets_auc_and_separation() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", None, None)?;
        let score = report
            .features
            .iter()
            .find(|f| f.path.ends_with("score"))
            .ok_or("missing score")?;

        assert_eq!(score.kind, FieldKind::Numeric);
        let auc = score.auc.ok_or("auc missing for numeric feature")?;
        let sep = score.separation.ok_or("separation missing")?;
        // score is fully separating: positives are all >= 60, negatives < 40.
        assert!((auc - 1.0).abs() < EPS, "expected AUC 1.0, got {auc}");
        assert!((sep - 1.0).abs() < EPS);
        Ok(())
    }

    /// AUC has no meaning for unordered values, so it must be withheld rather
    /// than fabricated.
    #[test]
    fn categorical_feature_has_no_auc() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(
            r#"[{"kind": "alpha", "label": "aaa_pos"}, {"kind": "beta", "label": "zzz_neg"},
                {"kind": "alpha", "label": "aaa_pos"}, {"kind": "beta", "label": "zzz_neg"}]"#,
        )?;
        let report = separation_analysis(&doc, "label", None, None)?;
        let kind = report
            .features
            .iter()
            .find(|f| f.path.ends_with("kind"))
            .ok_or("missing kind")?;

        assert_eq!(kind.kind, FieldKind::Categorical);
        assert!(kind.auc.is_none(), "AUC must not be invented");
        assert!(kind.separation.is_none());
        // But the entropy measures are still defined and maximal here.
        assert!((kind.mutual_information - 1.0).abs() < EPS);
        Ok(())
    }

    /// A three-class label makes AUC undefined but leaves the entropy measures
    /// intact.
    #[test]
    fn multiclass_label_withholds_auc() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(
            r#"[{"x": 1, "label": "a"}, {"x": 2, "label": "b"}, {"x": 3, "label": "c"},
                {"x": 4, "label": "a"}, {"x": 5, "label": "b"}, {"x": 6, "label": "c"}]"#,
        )?;
        let report = separation_analysis(&doc, "label", None, None)?;

        assert!(!report.binary);
        assert!(report.positive_class.is_none());
        assert_eq!(report.classes.len(), 3);
        for f in &report.features {
            assert!(f.auc.is_none(), "{} should have no AUC", f.path);
            assert!(f.operating_point.is_none());
        }
        Ok(())
    }

    #[test]
    fn base_rate_prices_the_operating_point() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", Some(1e-4), None)?;
        let score = report
            .features
            .iter()
            .find(|f| f.path.ends_with("score"))
            .ok_or("missing score")?;
        let op = score.operating_point.as_ref().ok_or("no operating point")?;

        assert!((op.tpr - 1.0).abs() < EPS, "perfect recall available");
        assert!(op.fpr.abs() < EPS, "with no false positives on this corpus");
        // FPR is 0 here, so precision stays 1 even at low prevalence.
        let precision = op.precision_at_base_rate.ok_or("no priced precision")?;
        assert!((precision - 1.0).abs() < EPS);
        Ok(())
    }

    /// The point of the flag: a feature that looks good on a balanced corpus
    /// becomes near-useless at realistic prevalence once it has any FPR.
    #[test]
    fn base_rate_exposes_corpus_optimism() -> Result<(), Box<dyn std::error::Error>> {
        // 10% FPR, 100% TPR.
        let mut rows: Vec<String> = Vec::new();
        for i in 0..100 {
            rows.push(r#"{"flag": 1, "label": "aaa_pos"}"#.to_owned());
            let flag = i32::from(i < 10);
            rows.push(format!(r#"{{"flag": {flag}, "label": "zzz_neg"}}"#));
        }
        let doc = parse(&format!("[{}]", rows.join(",")))?;
        let report = separation_analysis(&doc, "label", Some(1e-4), None)?;
        let flag = report
            .features
            .iter()
            .find(|f| f.path.ends_with("flag"))
            .ok_or("missing flag")?;
        let op = flag.operating_point.as_ref().ok_or("no operating point")?;

        assert!((op.tpr - 1.0).abs() < EPS);
        assert!((op.fpr - 0.10).abs() < 1e-6, "fpr {}", op.fpr);
        let precision = op.precision_at_base_rate.ok_or("no priced precision")?;
        assert!(
            precision < 0.01,
            "at 1e-4 prevalence a 10% FPR is near-useless, got {precision}"
        );
        Ok(())
    }

    #[test]
    fn rejects_single_class_label() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(r#"[{"x": 1, "label": "only"}, {"x": 2, "label": "only"}]"#)?;
        assert!(matches!(
            separation_analysis(&doc, "label", None, None),
            Err(SeparationError::SingleClass(_, _))
        ));
        Ok(())
    }

    #[test]
    fn rejects_missing_label() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(r#"[{"x": 1}, {"x": 2}]"#)?;
        assert!(matches!(
            separation_analysis(&doc, "label", None, None),
            Err(SeparationError::LabelMissing(_))
        ));
        Ok(())
    }

    #[test]
    fn rejects_invalid_base_rate() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        for bad in [0.0, 1.0, -0.5, 2.0] {
            assert!(matches!(
                separation_analysis(&doc, "label", Some(bad), None),
                Err(SeparationError::InvalidBaseRate(_))
            ));
        }
        Ok(())
    }

    #[test]
    fn accepts_jsonpath_style_label_field() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let a = separation_analysis(&doc, "label", None, None)?;
        let b = separation_analysis(&doc, "$.label", None, None)?;
        assert_eq!(a.labelled_records, b.labelled_records);
        assert_eq!(a.features.len(), b.features.len());
        Ok(())
    }

    #[test]
    fn is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let a = separation_analysis(&doc, "label", Some(0.01), None)?;
        let b = separation_analysis(&doc, "label", Some(0.01), None)?;
        let paths = |r: &SeparationReport| -> Vec<String> {
            r.features.iter().map(|f| f.path.clone()).collect()
        };
        assert_eq!(paths(&a), paths(&b));
        for (x, y) in a.features.iter().zip(b.features.iter()) {
            assert!((x.mutual_information - y.mutual_information).abs() < EPS);
        }
        Ok(())
    }

    /// A field observed on only some records must still be measured against the
    /// full label distribution, or its MI would be computed against a different
    /// ceiling than every other feature's.
    #[test]
    fn mutual_information_never_exceeds_baseline_entropy() -> Result<(), Box<dyn std::error::Error>>
    {
        // `sparse` appears on a third of records, with a skewed local balance.
        let rows: Vec<String> = (0..90)
            .map(|i| {
                let label = if i % 3 == 0 { "aaa_pos" } else { "zzz_neg" };
                if i % 3 == 0 {
                    format!(r#"{{"sparse": {i}, "label": "{label}"}}"#)
                } else {
                    format!(r#"{{"label": "{label}"}}"#)
                }
            })
            .collect();
        let doc = parse(&format!("[{}]", rows.join(",")))?;
        let report = separation_analysis(&doc, "label", None, None)?;

        for f in &report.features {
            assert!(
                f.mutual_information <= report.baseline_entropy + EPS,
                "{} MI {} exceeds baseline {}",
                f.path,
                f.mutual_information,
                report.baseline_entropy
            );
        }
        Ok(())
    }

    #[test]
    fn coverage_is_reported_for_sparse_fields() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(
            r#"[{"always": 1, "sometimes": 5, "label": "aaa_pos"},
                {"always": 2, "label": "zzz_neg"},
                {"always": 3, "label": "aaa_pos"},
                {"always": 4, "label": "zzz_neg"}]"#,
        )?;
        let report = separation_analysis(&doc, "label", None, None)?;
        let always = report
            .features
            .iter()
            .find(|f| f.path.ends_with("always"))
            .ok_or("missing always")?;
        let sometimes = report
            .features
            .iter()
            .find(|f| f.path.ends_with("sometimes"))
            .ok_or("missing sometimes")?;
        assert!((always.coverage - 1.0).abs() < EPS);
        assert!((sometimes.coverage - 0.25).abs() < EPS);
        Ok(())
    }

    /// Continuous fields must be discretised for the entropy measures, or a
    /// near-unique field trivially "determines" the label.
    #[test]
    fn continuous_field_is_binned_not_degenerate() -> Result<(), Box<dyn std::error::Error>> {
        let rows: Vec<String> = (0..200)
            .map(|i| {
                let label = if i % 2 == 0 { "aaa_pos" } else { "zzz_neg" };
                format!(r#"{{"unique": {i}, "label": "{label}"}}"#)
            })
            .collect();
        let doc = parse(&format!("[{}]", rows.join(",")))?;
        let report = separation_analysis(&doc, "label", None, None)?;
        let unique = report
            .features
            .iter()
            .find(|f| f.path.ends_with("unique"))
            .ok_or("missing unique")?;

        assert!(unique.binned, "a 200-distinct-value field must be binned");
        assert!(
            unique.mutual_information < 0.2,
            "an unrelated unique field must not look informative, got MI {}",
            unique.mutual_information
        );
        Ok(())
    }

    #[test]
    fn positive_class_can_be_chosen() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", None, Some("zzz_neg"))?;
        assert_eq!(report.positive_class.as_deref(), Some("zzz_neg"));

        // MI is symmetric, so the direction must not change it.
        let other = separation_analysis(&doc, "label", None, Some("aaa_pos"))?;
        for (a, b) in report.features.iter().zip(other.features.iter()) {
            assert_eq!(a.path, b.path);
            assert!((a.mutual_information - b.mutual_information).abs() < EPS);
        }
        Ok(())
    }

    #[test]
    fn unknown_positive_class_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        assert!(matches!(
            separation_analysis(&doc, "label", None, Some("nope")),
            Err(SeparationError::UnknownPositiveClass(_, _))
        ));
        Ok(())
    }

    /// When a field predicts the *negative* class, the reported rule must be the
    /// complement — otherwise the caller is handed a rule worse than its own
    /// inverse.
    #[test]
    fn negatively_predictive_field_reports_the_complement() -> Result<(), Box<dyn std::error::Error>>
    {
        // `flag` is 1 for the negative class and 0 for the positive one.
        let rows: Vec<String> = (0..100)
            .map(|i| {
                if i % 2 == 0 {
                    r#"{"flag": 0, "label": "aaa_pos"}"#.to_owned()
                } else {
                    r#"{"flag": 1, "label": "zzz_neg"}"#.to_owned()
                }
            })
            .collect();
        let doc = parse(&format!("[{}]", rows.join(",")))?;
        let report = separation_analysis(&doc, "label", None, Some("aaa_pos"))?;
        let flag = report
            .features
            .iter()
            .find(|f| f.path.ends_with("flag"))
            .ok_or("missing flag")?;
        let op = flag.operating_point.as_ref().ok_or("no operating point")?;

        assert!(
            op.youden_j > 0.0,
            "the emitted rule must be the useful direction, got J {}",
            op.youden_j
        );
        assert!(
            op.rule.starts_with('<'),
            "expected a complement rule, got {:?}",
            op.rule
        );
        assert!((op.tpr - 1.0).abs() < EPS);
        assert!(op.fpr.abs() < EPS);
        Ok(())
    }

    #[test]
    fn label_field_is_excluded_from_features() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse(&labelled_doc())?;
        let report = separation_analysis(&doc, "label", None, None)?;
        assert!(
            report.features.iter().all(|f| !f.path.ends_with("label")),
            "the label must not be scored against itself"
        );
        Ok(())
    }
}
