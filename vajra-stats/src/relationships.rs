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

/// How numeric paths are discretised before relationships are computed.
///
/// Conditional entropy over a continuous field is degenerate: when nearly every
/// value is unique, each X value maps to exactly one Y, so H(Y|X) is 0 and the
/// relationship looks perfect regardless of whether one exists. Discretising
/// first is what makes the measure meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinStrategy {
    /// Use raw values as-is.
    None,
    /// Equal-frequency buckets, labelled `q0`..`q{n-1}`.
    Quantile(usize),
    /// Equal-width buckets over the observed range, labelled `w0`..`w{n-1}`.
    EqualWidth(usize),
}

impl Default for BinStrategy {
    fn default() -> Self {
        Self::Quantile(5)
    }
}

/// Discretise one path's values.
///
/// Returns `None` — meaning "leave the values alone" — when the path is not
/// numeric, when any value is non-finite, or when its distinct count already
/// fits in `n` buckets. That last guard matters: binning a field with fewer
/// distinct values than buckets can only merge categories that were already
/// separable, losing resolution for nothing.
fn bin_values(values: &[String], strategy: BinStrategy) -> Option<Vec<String>> {
    let (n, equal_width) = match strategy {
        BinStrategy::None => return None,
        BinStrategy::Quantile(n) => (n, false),
        BinStrategy::EqualWidth(n) => (n, true),
    };
    if n < 2 {
        return None;
    }

    let mut parsed = Vec::with_capacity(values.len());
    for v in values {
        let f: f64 = v.parse().ok()?;
        if !f.is_finite() {
            return None;
        }
        parsed.push(f);
    }

    // NaN was excluded above, so partial_cmp is a total order here.
    let mut sorted = parsed.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut distinct = sorted.clone();
    distinct.dedup();
    if distinct.len() <= n {
        return None;
    }

    if equal_width {
        let (min, max) = (sorted[0], sorted[sorted.len() - 1]);
        let width = (max - min) / n as f64;
        if width <= 0.0 {
            return None;
        }
        return Some(
            parsed
                .iter()
                .map(|&v| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let b = (((v - min) / width) as usize).min(n - 1);
                    format!("w{b}")
                })
                .collect(),
        );
    }

    // Equal-frequency cuts at fixed indices, deduped so repeated values do not
    // produce empty buckets. bucket(v) = #{ cut : v > cut }.
    let mut cuts: Vec<f64> = (1..n).map(|i| sorted[i * sorted.len() / n]).collect();
    cuts.dedup();
    if cuts.is_empty() {
        return None;
    }
    Some(
        parsed
            .iter()
            .map(|&v| {
                let b = cuts.iter().filter(|&&c| v > c).count();
                format!("q{b}")
            })
            .collect(),
    )
}

/// A discovered statistical relationship between two fields, in one direction.
///
/// Both directions of every pair are reported, because
/// `relationship_strength` is **not symmetric**: it normalises by H(Y), so
/// `x -> y` and `y -> x` generally differ. Filtering on one of `field_x` /
/// `field_y` therefore selects a direction, not a subset of the pairs.
///
/// Use [`Self::mutual_information`] when comparing across pairs — it is
/// symmetric and in bits, so it is not affected by which field is the
/// predictor or by the fields' differing entropies.
#[derive(Debug, Clone)]
pub struct FieldRelationship {
    /// The "predictor" field.
    pub field_x: WildcardPath,
    /// The "predicted" field.
    pub field_y: WildcardPath,
    /// Conditional entropy H(Y|X), in bits.
    pub conditional_entropy: f64,
    /// Average PMI across observed value pairs.
    pub mean_pmi: f64,
    /// Normalised relationship strength: 1 - H(Y|X)/H(Y), clamped to [0,1].
    ///
    /// Direction-dependent — see the type-level note.
    pub relationship_strength: f64,
    /// Mutual information I(X;Y) = H(Y) - H(Y|X), in bits.
    ///
    /// Symmetric: identical for both directions of a pair.
    pub mutual_information: f64,
    /// Whether `field_x`'s values were discretised before analysis.
    pub field_x_binned: bool,
    /// Whether `field_y`'s values were discretised before analysis.
    pub field_y_binned: bool,
}

/// Discover relationships between field pairs, discretising numeric paths with
/// the default strategy ([`BinStrategy::Quantile`] of 5).
///
/// See [`discover_relationships_binned`] to choose the strategy.
#[must_use]
pub fn discover_relationships(doc: &Document, top_k: usize) -> Vec<FieldRelationship> {
    discover_relationships_binned(doc, top_k, BinStrategy::default())
}

/// Discover relationships between field pairs in a document.
///
/// Only considers the `top_k` most frequent scalar paths (by observation
/// count). Returns relationships sorted by strength descending.
///
/// Numeric paths are discretised according to `bins` before the joint
/// distributions are built; `field_x_binned` / `field_y_binned` record which
/// paths this applied to. Pass [`BinStrategy::None`] to analyse raw values,
/// bearing in mind that continuous fields then yield degenerate perfect
/// relationships.
#[must_use]
pub fn discover_relationships_binned(
    doc: &Document,
    top_k: usize,
    bins: BinStrategy,
) -> Vec<FieldRelationship> {
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

    // Discretise numeric paths before building joint distributions.
    let mut binned = vec![false; ranked.len()];
    for (idx, (_, values)) in ranked.iter_mut().enumerate() {
        if let Some(bucketed) = bin_values(values, bins) {
            *values = bucketed;
            binned[idx] = true;
        }
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

            // The pair loop only visits (i, j) with j > i, so without an
            // explicit reverse pass each unordered pair would be reported in
            // exactly one direction — chosen by observation-count rank, and
            // for the common case of equal counts by path order. Since
            // `relationship_strength` normalises by H(Y), that made the
            // reported strength depend on field naming. Emit both directions.
            let joint_swapped: BTreeMap<(String, String), u64> = joint
                .iter()
                .map(|((x, y), &c)| ((y.clone(), x.clone()), c))
                .collect();

            let h_y_given_x = conditional_entropy(&joint, total);
            let h_x_given_y = conditional_entropy(&joint_swapped, total);

            let x_counts: Vec<u64> = x_marginal.values().copied().collect();
            let y_counts: Vec<u64> = y_marginal.values().copied().collect();
            let h_x = shannon_entropy_from_counts(&x_counts);
            let h_y = shannon_entropy_from_counts(&y_counts);

            // I(X;Y) = H(Y) - H(Y|X) = H(X) - H(X|Y). Both forms are equal in
            // exact arithmetic; average them so floating-point error does not
            // make the two emitted rows disagree.
            let mutual_information = (((h_y - h_y_given_x) + (h_x - h_x_given_y)) / 2.0).max(0.0);

            // Mean PMI is symmetric: PMI(x,y) == PMI(y,x).
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

            // Relationship strength: 1 - H(target|predictor)/H(target),
            // clamped to [0,1]. H(target)=0 means the target is constant and
            // so perfectly predictable.
            let strength = |h_cond: f64, h_target: f64| {
                if h_target > 0.0 {
                    (1.0 - h_cond / h_target).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            };

            results.push(FieldRelationship {
                field_x: path_x.clone(),
                field_y: path_y.clone(),
                conditional_entropy: h_y_given_x,
                mean_pmi,
                relationship_strength: strength(h_y_given_x, h_y),
                mutual_information,
                field_x_binned: binned[i],
                field_y_binned: binned[j],
            });
            results.push(FieldRelationship {
                field_x: path_y.clone(),
                field_y: path_x.clone(),
                conditional_entropy: h_x_given_y,
                mean_pmi,
                relationship_strength: strength(h_x_given_y, h_x),
                mutual_information,
                field_x_binned: binned[j],
                field_y_binned: binned[i],
            });
        }
    }

    // Sort by strength descending, breaking ties on the path pair so the
    // ordering is fully specified rather than dependent on insertion order.
    // Emitting both directions makes equal-strength ties common.
    results.sort_by(|a, b| {
        b.relationship_strength
            .partial_cmp(&a.relationship_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.field_x.as_str().cmp(&b.field_x.as_str()))
            .then_with(|| a.field_y.as_str().cmp(&b.field_y.as_str()))
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

    fn parse_doc(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
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
    fn discover_correlated_fields() -> Result<(), Box<dyn std::error::Error>> {
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
        )?;
        let rels = discover_relationships(&doc, 50);
        assert!(!rels.is_empty());
        // The top relationship should have high strength.
        assert!(
            rels[0].relationship_strength > 0.9,
            "expected strong relationship, got {}",
            rels[0].relationship_strength
        );
        Ok(())
    }

    #[test]
    fn discover_independent_fields() -> Result<(), Box<dyn std::error::Error>> {
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
        )?;
        let rels = discover_relationships(&doc, 50);
        assert!(!rels.is_empty());
        // Independence → low strength.
        assert!(
            rels[0].relationship_strength < 0.2,
            "expected weak relationship, got {}",
            rels[0].relationship_strength
        );
        Ok(())
    }

    #[test]
    fn discover_empty_document() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc("{}")?;
        let rels = discover_relationships(&doc, 50);
        assert!(rels.is_empty());
        Ok(())
    }

    #[test]
    fn discover_single_field() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"[{"a": 1}, {"a": 2}]"#)?;
        let rels = discover_relationships(&doc, 50);
        assert!(
            rels.is_empty(),
            "single field should produce no relationships"
        );
        Ok(())
    }

    #[test]
    fn discover_single_object_record() -> Result<(), Box<dyn std::error::Error>> {
        // Single object is treated as one record.
        let doc = parse_doc(r#"{"x": "hello", "y": "world"}"#)?;
        let rels = discover_relationships(&doc, 50);
        // With only one record, H(Y|X)=0 and H(Y)=0, so strength = 1.0.
        assert!(!rels.is_empty());
        assert!(
            (rels[0].relationship_strength - 1.0).abs() < EPS,
            "expected strength 1.0 for single record, got {}",
            rels[0].relationship_strength
        );
        Ok(())
    }

    #[test]
    fn discover_array_of_scalars_returns_empty() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"[1, 2, 3]"#)?;
        let rels = discover_relationships(&doc, 50);
        assert!(rels.is_empty());
        Ok(())
    }

    #[test]
    fn discover_sorted_by_strength() -> Result<(), Box<dyn std::error::Error>> {
        // Three fields: city->country is strong, size is independent.
        let doc = parse_doc(
            r#"[
                {"city": "Paris", "country": "France", "size": "big"},
                {"city": "Paris", "country": "France", "size": "small"},
                {"city": "Berlin", "country": "Germany", "size": "big"},
                {"city": "Berlin", "country": "Germany", "size": "small"}
            ]"#,
        )?;
        let rels = discover_relationships(&doc, 50);
        // Results should be sorted descending by strength.
        for w in rels.windows(2) {
            assert!(
                w[0].relationship_strength >= w[1].relationship_strength,
                "results not sorted by strength"
            );
        }
        Ok(())
    }

    #[test]
    fn discover_respects_top_k_limit() -> Result<(), Box<dyn std::error::Error>> {
        // With top_k=1, only one field is considered → no pairs.
        let doc = parse_doc(
            r#"[
                {"a": 1, "b": 2, "c": 3},
                {"a": 4, "b": 5, "c": 6}
            ]"#,
        )?;
        let rels = discover_relationships(&doc, 1);
        assert!(rels.is_empty());
        Ok(())
    }

    /// `coarse` is a deterministic function of `fine`, so the pair is
    /// asymmetric: H(coarse|fine)=0 but H(fine|coarse)>0.
    fn asymmetric_doc(first: &str, second: &str) -> String {
        let rows: Vec<String> = (0..300)
            .map(|i| format!(r#"{{"{first}": "v{}", "{second}": "c{}"}}"#, i % 6, i % 2))
            .collect();
        format!("[{}]", rows.join(","))
    }

    /// Regression: each unordered pair must be reported in *both* directions.
    /// The pair loop only visits (i, j) with j > i, so previously a consumer
    /// filtering on `field_y` silently saw half the pairs.
    #[test]
    fn discover_emits_both_directions() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&asymmetric_doc("fine", "coarse"))?;
        let rels = discover_relationships(&doc, 50);

        let forward = rels
            .iter()
            .filter(|r| {
                r.field_x.as_str().contains("fine") && r.field_y.as_str().contains("coarse")
            })
            .count();
        let reverse = rels
            .iter()
            .filter(|r| {
                r.field_x.as_str().contains("coarse") && r.field_y.as_str().contains("fine")
            })
            .count();
        assert_eq!(forward, 1, "fine -> coarse must be present");
        assert_eq!(reverse, 1, "coarse -> fine must be present");
        Ok(())
    }

    /// `relationship_strength` is direction-dependent, so both directions of an
    /// asymmetric pair must report different values.
    #[test]
    fn strength_differs_by_direction() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&asymmetric_doc("fine", "coarse"))?;
        let rels = discover_relationships(&doc, 50);

        let fwd = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("fine"))
            .ok_or("missing fine -> coarse")?;
        let rev = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("coarse"))
            .ok_or("missing coarse -> fine")?;

        assert!(
            (fwd.relationship_strength - 1.0).abs() < EPS,
            "fine determines coarse: expected strength 1.0, got {}",
            fwd.relationship_strength
        );
        assert!(
            rev.relationship_strength < 0.9,
            "coarse does not determine fine: expected < 0.9, got {}",
            rev.relationship_strength
        );
        Ok(())
    }

    /// Mutual information is symmetric, so it must be identical for both
    /// directions — this is the measure that is safe to compare across pairs.
    #[test]
    fn mutual_information_is_symmetric() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&asymmetric_doc("fine", "coarse"))?;
        let rels = discover_relationships(&doc, 50);

        let fwd = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("fine"))
            .ok_or("missing forward")?;
        let rev = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("coarse"))
            .ok_or("missing reverse")?;

        assert!(
            (fwd.mutual_information - rev.mutual_information).abs() < 1e-9,
            "MI must match across directions: {} vs {}",
            fwd.mutual_information,
            rev.mutual_information
        );
        assert!(fwd.mutual_information > 0.0, "correlated fields share info");
        Ok(())
    }

    /// The reported values must not depend on what the fields are called.
    /// Previously, path order decided which direction was emitted, so renaming
    /// a field changed its reported strength.
    #[test]
    fn results_do_not_depend_on_field_names() -> Result<(), Box<dyn std::error::Error>> {
        // "aaa" sorts before "zzz", so these two documents present the same
        // relationship with opposite path ordering.
        let a = parse_doc(&asymmetric_doc("aaa_fine", "zzz_coarse"))?;
        let b = parse_doc(&asymmetric_doc("zzz_fine", "aaa_coarse"))?;

        let pick = |rels: &[FieldRelationship], predictor: &str| -> Option<(f64, f64)> {
            rels.iter()
                .find(|r| r.field_x.as_str().contains(predictor))
                .map(|r| (r.relationship_strength, r.mutual_information))
        };

        let ra = discover_relationships(&a, 50);
        let rb = discover_relationships(&b, 50);

        let (sa, ma) = pick(&ra, "fine").ok_or("missing fine predictor in a")?;
        let (sb, mb) = pick(&rb, "fine").ok_or("missing fine predictor in b")?;
        assert!(
            (sa - sb).abs() < EPS,
            "strength changed with field names: {sa} vs {sb}"
        );
        assert!((ma - mb).abs() < 1e-9, "MI changed with field names");
        Ok(())
    }

    /// A continuous predictor with noise: `score` influences `outcome` but does
    /// not determine it.
    ///
    /// `score` is unique per record, which is what makes the unbinned analysis
    /// degenerate — every distinct score maps to exactly one outcome.
    fn continuous_doc() -> String {
        let rows: Vec<String> = (0..400)
            .map(|i| {
                let score = f64::from(i) / 400.0 - 0.5;
                // Deterministic pseudo-noise so the fixture is reproducible.
                let jitter = f64::from((i * 37) % 11) / 11.0 - 0.5;
                let outcome = if score + jitter * 0.8 > 0.0 {
                    "hit"
                } else {
                    "miss"
                };
                format!(r#"{{"score": {score:.6}, "outcome": "{outcome}"}}"#)
            })
            .collect();
        format!("[{}]", rows.join(","))
    }

    /// Without binning, a near-unique continuous field trivially "determines"
    /// everything: each distinct X maps to one Y, so H(Y|X)=0.
    #[test]
    fn unbinned_continuous_field_is_degenerate() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&continuous_doc())?;
        let rels = discover_relationships_binned(&doc, 50, BinStrategy::None);
        let r = rels
            .iter()
            .find(|r| {
                r.field_x.as_str().contains("score") && r.field_y.as_str().contains("outcome")
            })
            .ok_or("missing score -> outcome")?;
        assert!(
            (r.relationship_strength - 1.0).abs() < EPS,
            "expected degenerate 1.0 without binning, got {}",
            r.relationship_strength
        );
        assert!(!r.field_x_binned);
        Ok(())
    }

    /// With binning the same pair reports a real, non-degenerate association.
    #[test]
    fn binning_makes_continuous_field_measurable() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&continuous_doc())?;
        let rels = discover_relationships_binned(&doc, 50, BinStrategy::Quantile(5));
        let r = rels
            .iter()
            .find(|r| {
                r.field_x.as_str().contains("score") && r.field_y.as_str().contains("outcome")
            })
            .ok_or("missing score -> outcome")?;
        assert!(r.field_x_binned, "score should be reported as binned");
        assert!(
            r.relationship_strength > 0.0 && r.relationship_strength < 0.95,
            "expected a real association, got {}",
            r.relationship_strength
        );
        Ok(())
    }

    /// The default strategy bins, so `discover_relationships` must not report
    /// the degenerate result.
    #[test]
    fn default_strategy_bins() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&continuous_doc())?;
        let rels = discover_relationships(&doc, 50);
        let r = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("score"))
            .ok_or("missing score")?;
        assert!(r.field_x_binned);
        Ok(())
    }

    /// A numeric field whose distinct count already fits in the bucket count is
    /// left alone — binning it could only merge separable categories.
    #[test]
    fn low_cardinality_numeric_is_not_binned() -> Result<(), Box<dyn std::error::Error>> {
        let rows: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"flag": {}, "other": "v{}"}}"#, i % 2, i % 3))
            .collect();
        let doc = parse_doc(&format!("[{}]", rows.join(",")))?;
        let rels = discover_relationships_binned(&doc, 50, BinStrategy::Quantile(5));
        let r = rels
            .iter()
            .find(|r| r.field_x.as_str().contains("flag"))
            .ok_or("missing flag")?;
        assert!(!r.field_x_binned, "2 distinct values must not be binned");
        Ok(())
    }

    /// Non-numeric fields are never binned, regardless of cardinality.
    #[test]
    fn string_field_is_not_binned() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&asymmetric_doc("fine", "coarse"))?;
        let rels = discover_relationships_binned(&doc, 50, BinStrategy::Quantile(5));
        assert!(
            rels.iter().all(|r| !r.field_x_binned && !r.field_y_binned),
            "string fields must not be binned"
        );
        Ok(())
    }

    #[test]
    fn equal_width_and_quantile_differ() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&continuous_doc())?;
        let q = discover_relationships_binned(&doc, 50, BinStrategy::Quantile(5));
        let w = discover_relationships_binned(&doc, 50, BinStrategy::EqualWidth(5));
        let pick = |rels: &[FieldRelationship]| -> Option<f64> {
            rels.iter()
                .find(|r| r.field_x.as_str().contains("score"))
                .map(|r| r.mutual_information)
        };
        let (qm, wm) = (pick(&q).ok_or("q")?, pick(&w).ok_or("w")?);
        assert!(qm > 0.0 && wm > 0.0, "both strategies find association");
        Ok(())
    }

    /// A bucket count below 2 is meaningless and must leave values untouched
    /// rather than collapsing every record into one bucket.
    #[test]
    fn bucket_count_below_two_does_not_bin() {
        let values: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        assert!(bin_values(&values, BinStrategy::Quantile(1)).is_none());
        assert!(bin_values(&values, BinStrategy::Quantile(0)).is_none());
    }

    /// Non-finite values make quantile boundaries meaningless, so such a column
    /// is treated as non-numeric rather than silently mis-bucketed.
    #[test]
    fn non_finite_values_are_not_binned() {
        let mut values: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        values.push("NaN".to_owned());
        assert!(bin_values(&values, BinStrategy::Quantile(5)).is_none());
        let mut values2: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        values2.push("inf".to_owned());
        assert!(bin_values(&values2, BinStrategy::Quantile(5)).is_none());
    }

    /// Repeated values must not produce more buckets than there are distinct
    /// cut points.
    #[test]
    fn quantile_binning_handles_heavy_ties() -> Result<(), Box<dyn std::error::Error>> {
        // 90% zeros, then 1..10 — quantile cuts collapse.
        let mut values: Vec<String> = vec!["0".to_owned(); 90];
        values.extend((1..=10).map(|i| i.to_string()));
        let binned =
            bin_values(&values, BinStrategy::Quantile(5)).ok_or("heavy-tie column should bin")?;
        assert_eq!(binned.len(), values.len());
        let distinct: BTreeMap<&str, ()> = binned.iter().map(|s| (s.as_str(), ())).collect();
        assert!(distinct.len() >= 2, "ties must still separate some records");
        assert!(distinct.len() <= 5, "must not exceed the bucket count");
        // Every zero lands in the same bucket.
        let zero_buckets: BTreeMap<&str, ()> =
            binned[..90].iter().map(|s| (s.as_str(), ())).collect();
        assert_eq!(zero_buckets.len(), 1, "equal values share a bucket");
        Ok(())
    }

    #[test]
    fn binning_is_deterministic() {
        let values: Vec<String> = (0..200).map(|i| (i * 7 % 200).to_string()).collect();
        let a = bin_values(&values, BinStrategy::Quantile(5));
        let b = bin_values(&values, BinStrategy::Quantile(5));
        assert_eq!(a, b);
    }

    #[test]
    fn discover_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(&asymmetric_doc("fine", "coarse"))?;
        let first = discover_relationships(&doc, 50);
        let second = discover_relationships(&doc, 50);

        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.field_x.as_str(), b.field_x.as_str());
            assert_eq!(a.field_y.as_str(), b.field_y.as_str());
            assert!((a.relationship_strength - b.relationship_strength).abs() < EPS);
        }
        Ok(())
    }
}
