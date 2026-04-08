//! Cross-field relationship discovery via conditional entropy and PMI.
//!
//! Given a [`Document`], this module discovers statistical relationships
//! between pairs of scalar fields by treating repeated objects (e.g. array
//! elements) as records and computing joint distributions over field values.

use std::collections::BTreeMap;

use vajra_types::path::WildcardPath;
use vajra_types::Document;

use crate::entropy::shannon_entropy_from_counts;

// ---------------------------------------------------------------------------
// Core information-theoretic functions
// ---------------------------------------------------------------------------

/// Compute conditional entropy H(Y|X) from joint observation counts.
///
/// H(Y|X) = -Σ p(x,y) * log2(p(y|x))
///
/// Low H(Y|X) means X strongly predicts Y.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn conditional_entropy(joint_counts: &BTreeMap<(String, String), u64>, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }

    // Marginal counts for X.
    let mut x_counts: BTreeMap<&str, u64> = BTreeMap::new();
    for ((x, _), &count) in joint_counts {
        *x_counts.entry(x.as_str()).or_insert(0) += count;
    }

    let total_f = total as f64;
    let mut h = 0.0_f64;

    for ((x, _y), &joint_c) in joint_counts {
        if joint_c == 0 {
            continue;
        }
        let x_c = x_counts.get(x.as_str()).copied().unwrap_or(0);
        if x_c == 0 {
            continue;
        }
        let p_xy = joint_c as f64 / total_f;
        let p_y_given_x = joint_c as f64 / x_c as f64;
        h -= p_xy * p_y_given_x.log2();
    }

    if h < 0.0 {
        0.0
    } else {
        h
    }
}

/// Compute pointwise mutual information for a single (x, y) co-occurrence.
///
/// PMI(x,y) = log2(P(x,y) / (P(x) * P(y)))
///
/// Positive = co-occur more than chance. Negative = avoid each other.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn pmi(joint_count: u64, x_count: u64, y_count: u64, total: u64) -> f64 {
    if joint_count == 0 || x_count == 0 || y_count == 0 || total == 0 {
        return 0.0;
    }
    let total_f = total as f64;
    let p_xy = joint_count as f64 / total_f;
    let p_x = x_count as f64 / total_f;
    let p_y = y_count as f64 / total_f;

    let denom = p_x * p_y;
    if denom <= 0.0 {
        return 0.0;
    }

    (p_xy / denom).log2()
}

// ---------------------------------------------------------------------------
// Relationship discovery
// ---------------------------------------------------------------------------

/// A discovered statistical relationship between two fields.
#[derive(Debug, Clone)]
pub struct FieldRelationship {
    /// The "predictor" field.
    pub field_x: WildcardPath,
    /// The "predicted" field.
    pub field_y: WildcardPath,
    /// Conditional entropy H(Y|X).
    pub conditional_entropy: f64,
    /// Average PMI across observed value pairs.
    pub mean_pmi: f64,
    /// Normalised relationship strength: 1 - H(Y|X)/H(Y), clamped to [0,1].
    pub relationship_strength: f64,
}

/// Discover relationships between field pairs in a document.
///
/// Only considers the `top_k` most frequent scalar paths (by observation
/// count). Returns relationships sorted by strength descending.
#[must_use]
pub fn discover_relationships(doc: &Document, top_k: usize) -> Vec<FieldRelationship> {
    let records = extract_records(doc.value());
    if records.is_empty() {
        return Vec::new();
    }

    // Collect per-path value lists from records.
    let mut path_values: BTreeMap<WildcardPath, Vec<String>> = BTreeMap::new();
    for record in &records {
        let mut record_vals: BTreeMap<WildcardPath, String> = BTreeMap::new();
        collect_scalars(record, &WildcardPath::root(), &mut record_vals);
        for (path, value) in record_vals {
            path_values.entry(path).or_default().push(value);
        }
    }

    // Rank paths by total observation count and keep top_k.
    let mut ranked: Vec<(WildcardPath, Vec<String>)> = path_values.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    ranked.truncate(top_k);

    if ranked.len() < 2 {
        return Vec::new();
    }

    let num_records = records.len();
    let mut results = Vec::new();

    for i in 0..ranked.len() {
        for j in (i + 1)..ranked.len() {
            let (path_x, vals_x) = &ranked[i];
            let (path_y, vals_y) = &ranked[j];

            // Both must have the same number of observations as records to
            // form a meaningful joint distribution.
            let n = vals_x.len().min(vals_y.len()).min(num_records);
            if n == 0 {
                continue;
            }

            // Build joint and marginal counts.
            let mut joint: BTreeMap<(String, String), u64> = BTreeMap::new();
            let mut x_marginal: BTreeMap<String, u64> = BTreeMap::new();
            let mut y_marginal: BTreeMap<String, u64> = BTreeMap::new();

            for k in 0..n {
                let x_val = vals_x[k].clone();
                let y_val = vals_y[k].clone();
                *joint.entry((x_val.clone(), y_val.clone())).or_insert(0) += 1;
                *x_marginal.entry(x_val).or_insert(0) += 1;
                *y_marginal.entry(y_val).or_insert(0) += 1;
            }

            #[allow(clippy::cast_possible_truncation)]
            let total = n as u64;

            // H(Y|X)
            let h_y_given_x = conditional_entropy(&joint, total);

            // H(Y) for normalisation
            let y_counts: Vec<u64> = y_marginal.values().copied().collect();
            let h_y = shannon_entropy_from_counts(&y_counts);

            // Mean PMI
            let mut pmi_sum = 0.0_f64;
            let mut pmi_count = 0u64;
            for ((x_val, y_val), &jc) in &joint {
                let xc = x_marginal.get(x_val).copied().unwrap_or(0);
                let yc = y_marginal.get(y_val).copied().unwrap_or(0);
                let p = pmi(jc, xc, yc, total);
                pmi_sum += p;
                pmi_count += 1;
            }
            let mean_pmi = if pmi_count > 0 {
                pmi_sum / pmi_count as f64
            } else {
                0.0
            };

            // Relationship strength: 1 - H(Y|X)/H(Y), clamped to [0,1].
            let strength = if h_y > 0.0 {
                (1.0 - h_y_given_x / h_y).clamp(0.0, 1.0)
            } else {
                // H(Y)=0 means Y is constant → perfectly predictable.
                1.0
            };

            results.push(FieldRelationship {
                field_x: path_x.clone(),
                field_y: path_y.clone(),
                conditional_entropy: h_y_given_x,
                mean_pmi,
                relationship_strength: strength,
            });
        }
    }

    // Sort by strength descending.
    results.sort_by(|a, b| {
        b.relationship_strength
            .partial_cmp(&a.relationship_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

// ---------------------------------------------------------------------------
// Record extraction helpers
// ---------------------------------------------------------------------------

/// Extract "records" from a JSON value.
///
/// - If the value is an array of objects, each element is a record.
/// - If the value is a single object, treat it as one record.
/// - Otherwise (bare scalar, array of scalars) return an empty vec.
fn extract_records(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => {
            let obj_records: Vec<&serde_json::Value> =
                arr.iter().filter(|v| v.is_object()).collect();
            if !obj_records.is_empty() {
                return obj_records;
            }
            // Array of non-objects: no records.
            Vec::new()
        }
        serde_json::Value::Object(_) => {
            // Check if any child is an array of objects (nested records).
            if let Some(arr) = find_deepest_object_array(value) {
                return arr;
            }
            // Single object treated as one record.
            vec![value]
        }
        _ => Vec::new(),
    }
}

/// Find the first array-of-objects in an object's children.
fn find_deepest_object_array(value: &serde_json::Value) -> Option<Vec<&serde_json::Value>> {
    if let serde_json::Value::Object(map) = value {
        for child in map.values() {
            if let serde_json::Value::Array(arr) = child {
                let objs: Vec<&serde_json::Value> = arr.iter().filter(|v| v.is_object()).collect();
                if !objs.is_empty() {
                    return Some(objs);
                }
            }
        }
    }
    None
}

/// Collect all scalar (leaf) values in a JSON value, keyed by wildcard path.
fn collect_scalars(
    value: &serde_json::Value,
    path: &WildcardPath,
    out: &mut BTreeMap<WildcardPath, String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let child_path = path.push_key(key);
                collect_scalars(child, &child_path, out);
            }
        }
        serde_json::Value::Array(arr) => {
            // For arrays within a record, we skip nested arrays to keep it
            // simple in this phase — only direct scalar children matter.
            let child_path = path.push_array_wildcard();
            for child in arr {
                collect_scalars(child, &child_path, out);
            }
        }
        scalar => {
            let s = scalar_to_string(scalar);
            out.insert(path.clone(), s);
        }
    }
}

/// Convert a scalar JSON value to a deterministic string representation.
fn scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Bool(b) => {
            if *b {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    fn parse_doc(json: &str) -> Document {
        vajra_core::parse_str(json).unwrap_or_else(|e| {
            panic!("parse failed: {e}");
        })
    }

    // ---- conditional entropy ----

    #[test]
    fn cond_entropy_deterministic_yields_zero() {
        // X determines Y perfectly: (a,1), (b,2). H(Y|X) = 0
        let mut joint = BTreeMap::new();
        joint.insert(("a".to_owned(), "1".to_owned()), 50u64);
        joint.insert(("b".to_owned(), "2".to_owned()), 50u64);
        let h = conditional_entropy(&joint, 100);
        assert!(h.abs() < EPS, "expected ~0, got {h}");
    }

    #[test]
    fn cond_entropy_independent_equals_marginal() {
        // X and Y are independent: uniform 2x2.
        // P(a,1)=P(a,2)=P(b,1)=P(b,2) = 0.25
        // H(Y|X) should equal H(Y) = 1.0
        let mut joint = BTreeMap::new();
        joint.insert(("a".to_owned(), "1".to_owned()), 25u64);
        joint.insert(("a".to_owned(), "2".to_owned()), 25u64);
        joint.insert(("b".to_owned(), "1".to_owned()), 25u64);
        joint.insert(("b".to_owned(), "2".to_owned()), 25u64);
        let h = conditional_entropy(&joint, 100);
        assert!((h - 1.0).abs() < EPS, "expected ~1.0 (= H(Y)), got {h}");
    }

    #[test]
    fn cond_entropy_partial_dependence() {
        // X = a always maps to Y = 1, X = b is split equally between 1 and 2.
        // P(a,1)=0.5, P(b,1)=0.25, P(b,2)=0.25
        // H(Y|X) = 0.5*(-1*log2(1)) + 0.25*(-log2(0.5)) + 0.25*(-log2(0.5))
        //        = 0 + 0.25*1 + 0.25*1 = 0.5
        let mut joint = BTreeMap::new();
        joint.insert(("a".to_owned(), "1".to_owned()), 50u64);
        joint.insert(("b".to_owned(), "1".to_owned()), 25u64);
        joint.insert(("b".to_owned(), "2".to_owned()), 25u64);
        let h = conditional_entropy(&joint, 100);
        assert!((h - 0.5).abs() < EPS, "expected ~0.5, got {h}");
    }

    #[test]
    fn cond_entropy_empty_returns_zero() {
        let joint: BTreeMap<(String, String), u64> = BTreeMap::new();
        assert!((conditional_entropy(&joint, 0)).abs() < EPS);
    }

    // ---- PMI ----

    #[test]
    fn pmi_positive_co_occurrence() {
        // joint=50, x=50, y=50, total=100
        // P(xy)=0.5, P(x)=0.5, P(y)=0.5 → PMI = log2(0.5/0.25) = 1.0
        let p = pmi(50, 50, 50, 100);
        assert!((p - 1.0).abs() < EPS, "expected ~1.0, got {p}");
    }

    #[test]
    fn pmi_independent_approximately_zero() {
        // joint=25, x=50, y=50, total=100
        // P(xy)=0.25, P(x)*P(y)=0.25 → PMI = 0
        let p = pmi(25, 50, 50, 100);
        assert!(p.abs() < EPS, "expected ~0, got {p}");
    }

    #[test]
    fn pmi_negative_avoidance() {
        // joint=5, x=50, y=50, total=100
        // P(xy)=0.05, P(x)*P(y)=0.25 → PMI = log2(0.05/0.25) = log2(0.2) < 0
        let p = pmi(5, 50, 50, 100);
        assert!(p < 0.0, "expected negative PMI, got {p}");
    }

    #[test]
    fn pmi_zero_joint_returns_zero() {
        assert!((pmi(0, 50, 50, 100)).abs() < EPS);
    }

    // ---- discover_relationships ----

    #[test]
    fn discover_correlated_fields() {
        // city determines country perfectly.
        let doc = parse_doc(
            r#"[
                {"city": "Paris", "country": "France"},
                {"city": "Paris", "country": "France"},
                {"city": "Berlin", "country": "Germany"},
                {"city": "Berlin", "country": "Germany"},
                {"city": "Tokyo", "country": "Japan"},
                {"city": "Tokyo", "country": "Japan"}
            ]"#,
        );
        let rels = discover_relationships(&doc, 50);
        assert!(!rels.is_empty());
        // The top relationship should have high strength.
        assert!(
            rels[0].relationship_strength > 0.9,
            "expected strong relationship, got {}",
            rels[0].relationship_strength
        );
    }

    #[test]
    fn discover_independent_fields() {
        // Interleave values so that fields are roughly independent.
        let doc = parse_doc(
            r#"[
                {"x": "a", "y": "1"},
                {"x": "b", "y": "2"},
                {"x": "a", "y": "2"},
                {"x": "b", "y": "1"},
                {"x": "a", "y": "1"},
                {"x": "b", "y": "2"},
                {"x": "a", "y": "2"},
                {"x": "b", "y": "1"}
            ]"#,
        );
        let rels = discover_relationships(&doc, 50);
        assert!(!rels.is_empty());
        // Independence → low strength.
        assert!(
            rels[0].relationship_strength < 0.2,
            "expected weak relationship, got {}",
            rels[0].relationship_strength
        );
    }

    #[test]
    fn discover_empty_document() {
        let doc = parse_doc("{}");
        let rels = discover_relationships(&doc, 50);
        assert!(rels.is_empty());
    }

    #[test]
    fn discover_single_field() {
        let doc = parse_doc(r#"[{"a": 1}, {"a": 2}]"#);
        let rels = discover_relationships(&doc, 50);
        assert!(
            rels.is_empty(),
            "single field should produce no relationships"
        );
    }

    #[test]
    fn discover_single_object_record() {
        // Single object is treated as one record.
        let doc = parse_doc(r#"{"x": "hello", "y": "world"}"#);
        let rels = discover_relationships(&doc, 50);
        // With only one record, H(Y|X)=0 and H(Y)=0, so strength = 1.0.
        assert!(!rels.is_empty());
        assert!(
            (rels[0].relationship_strength - 1.0).abs() < EPS,
            "expected strength 1.0 for single record, got {}",
            rels[0].relationship_strength
        );
    }

    #[test]
    fn discover_array_of_scalars_returns_empty() {
        let doc = parse_doc(r#"[1, 2, 3]"#);
        let rels = discover_relationships(&doc, 50);
        assert!(rels.is_empty());
    }

    #[test]
    fn discover_sorted_by_strength() {
        // Three fields: city->country is strong, size is independent.
        let doc = parse_doc(
            r#"[
                {"city": "Paris", "country": "France", "size": "big"},
                {"city": "Paris", "country": "France", "size": "small"},
                {"city": "Berlin", "country": "Germany", "size": "big"},
                {"city": "Berlin", "country": "Germany", "size": "small"}
            ]"#,
        );
        let rels = discover_relationships(&doc, 50);
        // Results should be sorted descending by strength.
        for w in rels.windows(2) {
            assert!(
                w[0].relationship_strength >= w[1].relationship_strength,
                "results not sorted by strength"
            );
        }
    }

    #[test]
    fn discover_respects_top_k_limit() {
        // With top_k=1, only one field is considered → no pairs.
        let doc = parse_doc(
            r#"[
                {"a": 1, "b": 2, "c": 3},
                {"a": 4, "b": 5, "c": 6}
            ]"#,
        );
        let rels = discover_relationships(&doc, 1);
        assert!(rels.is_empty());
    }
}
