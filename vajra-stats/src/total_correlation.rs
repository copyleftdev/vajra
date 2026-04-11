//! Total correlation: multivariate dependency beyond pairwise.
//!
//! Total correlation (multi-information) measures the total statistical
//! dependency across **all** fields simultaneously:
//!
//! ```text
//! TC(X₁,...,Xₙ) = Σ H(Xᵢ) - H(X₁,...,Xₙ)
//! ```
//!
//! TC = 0 means all fields are independent. High TC means the schema has
//! deep internal structure. Catches what pairwise analysis misses: fields
//! that are independent in pairs but jointly constrained.
//!
//! Reference: Watanabe, S. (1960). "Information theoretical analysis of
//!            multivariate correlation."

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::entropy::shannon_entropy_from_counts;

/// Result of total correlation analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TotalCorrelationResult {
    /// Total correlation in bits.
    pub tc_bits: f64,
    /// Normalized TC: tc_bits / Σ H(Xᵢ), in [0, 1].
    /// 0 = fully independent, 1 = fully redundant.
    pub normalized_tc: f64,
    /// Number of fields analyzed.
    pub field_count: usize,
    /// Joint entropy H(X₁,...,Xₙ).
    pub joint_entropy: f64,
    /// Sum of marginal entropies Σ H(Xᵢ).
    pub sum_marginal_entropies: f64,
}

impl Default for TotalCorrelationResult {
    fn default() -> Self {
        Self {
            tc_bits: 0.0,
            normalized_tc: 0.0,
            field_count: 0,
            joint_entropy: 0.0,
            sum_marginal_entropies: 0.0,
        }
    }
}

/// Compute total correlation from field-value records.
///
/// Each record is a `BTreeMap<String, String>` mapping field names to values.
/// Only fields present in every record are considered.
///
/// Returns a zeroed result for empty input or fewer than 2 fields.
#[must_use]
pub fn total_correlation(records: &[BTreeMap<String, String>]) -> TotalCorrelationResult {
    if records.is_empty() {
        return TotalCorrelationResult::default();
    }

    // Find fields present in all records
    let all_fields: Vec<String> = {
        let mut field_counts: BTreeMap<String, usize> = BTreeMap::new();
        for record in records {
            for key in record.keys() {
                *field_counts.entry(key.clone()).or_insert(0) += 1;
            }
        }
        field_counts
            .into_iter()
            .filter(|(_, count)| *count == records.len())
            .map(|(key, _)| key)
            .collect()
    };

    if all_fields.len() < 2 {
        return TotalCorrelationResult {
            field_count: all_fields.len(),
            ..TotalCorrelationResult::default()
        };
    }

    // Limit to at most 8 fields for tractability (joint entropy is exponential)
    let fields: Vec<&str> = all_fields.iter().map(|s| s.as_str()).take(8).collect();

    total_correlation_for_fields(records, &fields)
}

/// Compute total correlation for a specific set of fields.
///
/// Allows the caller to select which fields to analyze.
#[must_use]
pub fn total_correlation_for_fields(
    records: &[BTreeMap<String, String>],
    fields: &[&str],
) -> TotalCorrelationResult {
    if records.is_empty() || fields.len() < 2 {
        return TotalCorrelationResult {
            field_count: fields.len(),
            ..TotalCorrelationResult::default()
        };
    }

    let n_fields = fields.len();

    // 1. Compute marginal entropies H(Xᵢ) for each field
    let mut marginal_entropies = Vec::with_capacity(n_fields);
    for &field in fields {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for record in records {
            if let Some(val) = record.get(field) {
                *counts.entry(val.clone()).or_insert(0) += 1;
            }
        }
        let count_vec: Vec<u64> = counts.into_values().collect();
        marginal_entropies.push(shannon_entropy_from_counts(&count_vec));
    }

    let sum_marginals: f64 = marginal_entropies.iter().sum();

    // 2. Compute joint entropy H(X₁,...,Xₙ)
    //    Key: concatenation of all field values (deterministic via BTreeMap field order)
    let mut joint_counts: BTreeMap<Vec<String>, u64> = BTreeMap::new();
    for record in records {
        let key: Vec<String> = fields
            .iter()
            .map(|&f| record.get(f).cloned().unwrap_or_default())
            .collect();
        *joint_counts.entry(key).or_insert(0) += 1;
    }
    let joint_count_vec: Vec<u64> = joint_counts.into_values().collect();
    let joint_entropy = shannon_entropy_from_counts(&joint_count_vec);

    // 3. TC = Σ H(Xᵢ) - H(X₁,...,Xₙ)
    let tc = sum_marginals - joint_entropy;

    // TC should be ≥ 0 theoretically; clamp for floating-point noise
    let tc_clamped = if tc < 0.0 { 0.0 } else { tc };

    let normalized = if sum_marginals > 0.0 {
        (tc_clamped / sum_marginals).clamp(0.0, 1.0)
    } else {
        0.0
    };

    TotalCorrelationResult {
        tc_bits: tc_clamped,
        normalized_tc: normalized,
        field_count: n_fields,
        joint_entropy,
        sum_marginal_entropies: sum_marginals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn make_records(data: &[&[(&str, &str)]]) -> Vec<BTreeMap<String, String>> {
        data.iter()
            .map(|fields| {
                fields
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect()
            })
            .collect()
    }

    // ---- total_correlation ----

    #[test]
    fn empty_returns_zero() {
        let result = total_correlation(&[]);
        assert!(result.tc_bits.abs() < EPS);
        assert_eq!(result.field_count, 0);
    }

    #[test]
    fn single_field_returns_zero() {
        let records = make_records(&[&[("a", "1")], &[("a", "2")]]);
        let result = total_correlation(&records);
        assert!(result.tc_bits.abs() < EPS);
    }

    #[test]
    fn independent_fields_near_zero() {
        // a and b are independent: all 4 combinations equally likely
        let records = make_records(&[
            &[("a", "0"), ("b", "0")],
            &[("a", "0"), ("b", "1")],
            &[("a", "1"), ("b", "0")],
            &[("a", "1"), ("b", "1")],
        ]);
        let result = total_correlation(&records);
        assert!(
            result.tc_bits.abs() < 0.01,
            "independent fields should have TC ≈ 0, got {}",
            result.tc_bits
        );
    }

    #[test]
    fn perfectly_correlated_fields() {
        // a determines b: (0,0), (1,1) — TC = H(a) + H(b) - H(a,b) = 1 + 1 - 1 = 1
        let records = make_records(&[
            &[("a", "0"), ("b", "0")],
            &[("a", "0"), ("b", "0")],
            &[("a", "1"), ("b", "1")],
            &[("a", "1"), ("b", "1")],
        ]);
        let result = total_correlation(&records);
        assert!(
            (result.tc_bits - 1.0).abs() < EPS,
            "perfectly correlated should have TC = 1.0, got {}",
            result.tc_bits
        );
    }

    #[test]
    fn tc_non_negative() {
        let records = make_records(&[
            &[("x", "a"), ("y", "1"), ("z", "yes")],
            &[("x", "b"), ("y", "2"), ("z", "no")],
            &[("x", "a"), ("y", "2"), ("z", "yes")],
            &[("x", "b"), ("y", "1"), ("z", "no")],
        ]);
        let result = total_correlation(&records);
        assert!(
            result.tc_bits >= -EPS,
            "TC should be non-negative, got {}",
            result.tc_bits
        );
    }

    #[test]
    fn tc_upper_bounded_by_sum_marginals() {
        let records = make_records(&[&[("a", "0"), ("b", "0")], &[("a", "1"), ("b", "1")]]);
        let result = total_correlation(&records);
        assert!(
            result.tc_bits <= result.sum_marginal_entropies + EPS,
            "TC ({}) should be ≤ Σ H ({}) ",
            result.tc_bits,
            result.sum_marginal_entropies
        );
    }

    #[test]
    fn normalized_bounded_01() {
        let records = make_records(&[
            &[("a", "x"), ("b", "y")],
            &[("a", "x"), ("b", "z")],
            &[("a", "y"), ("b", "y")],
        ]);
        let result = total_correlation(&records);
        assert!(
            result.normalized_tc >= -EPS && result.normalized_tc <= 1.0 + EPS,
            "normalized TC should be in [0,1], got {}",
            result.normalized_tc
        );
    }

    #[test]
    fn three_way_dependency() {
        // city, state, zip: city+state determines zip perfectly
        // All records have the same mapping
        let records = make_records(&[
            &[("city", "SF"), ("state", "CA"), ("zip", "94103")],
            &[("city", "SF"), ("state", "CA"), ("zip", "94103")],
            &[("city", "NYC"), ("state", "NY"), ("zip", "10001")],
            &[("city", "NYC"), ("state", "NY"), ("zip", "10001")],
            &[("city", "CHI"), ("state", "IL"), ("zip", "60601")],
            &[("city", "CHI"), ("state", "IL"), ("zip", "60601")],
        ]);
        let result = total_correlation(&records);
        // All three fields are perfectly correlated: TC should be significant
        assert!(
            result.tc_bits > 1.0,
            "three-way dependency should have high TC, got {}",
            result.tc_bits
        );
    }

    // ---- total_correlation_for_fields ----

    #[test]
    fn for_fields_subset() {
        let records = make_records(&[
            &[("a", "0"), ("b", "0"), ("c", "x")],
            &[("a", "1"), ("b", "1"), ("c", "y")],
        ]);
        let result = total_correlation_for_fields(&records, &["a", "b"]);
        assert_eq!(result.field_count, 2);
        assert!(result.tc_bits > 0.0);
    }

    #[test]
    fn for_fields_empty_records() {
        let result = total_correlation_for_fields(&[], &["a", "b"]);
        assert!(result.tc_bits.abs() < EPS);
    }

    #[test]
    fn for_fields_single_field() {
        let records = make_records(&[&[("a", "0")], &[("a", "1")]]);
        let result = total_correlation_for_fields(&records, &["a"]);
        assert!(result.tc_bits.abs() < EPS);
        assert_eq!(result.field_count, 1);
    }

    #[test]
    fn constant_fields_zero_tc() {
        // All records identical: H(joint) = 0, H(marginals) = 0, TC = 0
        let records = make_records(&[
            &[("a", "same"), ("b", "same")],
            &[("a", "same"), ("b", "same")],
            &[("a", "same"), ("b", "same")],
        ]);
        let result = total_correlation(&records);
        assert!(
            result.tc_bits.abs() < EPS,
            "constant fields should have TC = 0, got {}",
            result.tc_bits
        );
    }
}
