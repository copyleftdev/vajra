//! Chaos and adversarial input tests for information-theoretic primitives.
//!
//! Feed pathological inputs and verify NO PANICS and graceful degradation.
//! Every function must handle: empty, single, all-identical, all-unique,
//! extreme floats, and degenerate distributions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use vajra_stats::lz_complexity::{
    classify_entropy_complexity, lz76_complexity, lz76_phrase_count, lz_analyze,
    lz_complexity_for_values,
};
use vajra_stats::renyi::{normalized_renyi_spectrum, renyi_entropy, renyi_spectrum};
use vajra_stats::total_correlation::{total_correlation, total_correlation_for_fields};
use vajra_stats::transfer_entropy::{bidirectional_transfer_entropy, bin_values, transfer_entropy};

// -----------------------------------------------------------------------
// Rényi Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_renyi_empty_counts() {
    let s = renyi_spectrum(&[]);
    assert!(s.hartley.is_finite());
    assert!(s.shannon.is_finite());
    assert!(s.collision.is_finite());
    assert!(s.min_entropy.is_finite());
}

#[test]
fn chaos_renyi_all_zeros() {
    let s = renyi_spectrum(&[0, 0, 0]);
    assert!(s.hartley.is_finite());
    assert!(s.shannon.is_finite());
}

#[test]
fn chaos_renyi_single_massive_count() {
    let s = renyi_spectrum(&[u64::MAX / 2]);
    assert!(s.hartley.is_finite());
    assert!(s.shannon >= 0.0);
}

#[test]
fn chaos_renyi_one_hot() {
    // One value dominates completely
    let mut counts = vec![0u64; 100];
    counts[0] = 1_000_000;
    counts[50] = 1;
    let s = renyi_spectrum(&counts);
    assert!(s.hartley.is_finite());
    assert!(s.min_entropy.is_finite());
    assert!(s.min_entropy >= 0.0);
}

#[test]
fn chaos_renyi_extreme_alpha() {
    let counts = &[10, 20, 30];
    // Very large alpha
    assert!(renyi_entropy(counts, 100.0).is_finite());
    // Very small positive alpha
    assert!(renyi_entropy(counts, 0.001).is_finite());
    // Negative alpha (unusual but should not panic)
    let h = renyi_entropy(counts, -1.0);
    assert!(h.is_finite());
}

#[test]
fn chaos_renyi_normalized_single_value() {
    let ns = normalized_renyi_spectrum(&[42]);
    assert!(ns.hartley.is_finite());
    assert!(ns.shannon.is_finite());
}

#[test]
fn chaos_renyi_10k_distinct_values() {
    let counts: Vec<u64> = (1..=10_000).collect();
    let s = renyi_spectrum(&counts);
    assert!(s.hartley.is_finite());
    assert!(s.shannon.is_finite());
    assert!(s.collision.is_finite());
    assert!(s.min_entropy.is_finite());
}

// -----------------------------------------------------------------------
// LZ Complexity Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_lz_empty() {
    assert_eq!(lz76_phrase_count(&[]), 0);
    assert!((lz76_complexity(&[]) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn chaos_lz_single_byte() {
    let r = lz_analyze(&[42]);
    assert!(r.normalized.is_finite());
    assert_eq!(r.phrase_count, 1);
}

#[test]
fn chaos_lz_all_same_byte() {
    let data = vec![0xFFu8; 10_000];
    let c = lz76_complexity(&data);
    assert!(c.is_finite());
    assert!(c >= 0.0);
    assert!(c <= 1.0);
}

#[test]
fn chaos_lz_all_unique_bytes() {
    // 256 distinct bytes
    let data: Vec<u8> = (0..=255).collect();
    let c = lz76_complexity(&data);
    assert!(c.is_finite());
    assert!(c >= 0.0);
}

#[test]
fn chaos_lz_empty_strings() {
    let result = lz_complexity_for_values(&[""]);
    assert!(result.normalized.is_finite());
}

#[test]
fn chaos_lz_classify_boundary() {
    // Exact boundary values
    assert_eq!(classify_entropy_complexity(0.5, 0.5), "constant");
    assert_eq!(classify_entropy_complexity(0.500001, 0.500001), "random");
}

#[test]
fn chaos_lz_long_repeated_pattern() {
    // 100KB of repeated pattern
    let pattern = b"abcdefghij";
    let data: Vec<u8> = pattern.iter().copied().cycle().take(100_000).collect();
    let c = lz76_complexity(&data);
    assert!(c.is_finite());
    assert!(c >= 0.0);
    assert!(c <= 1.0);
}

// -----------------------------------------------------------------------
// Transfer Entropy Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_te_empty_series() {
    let te = transfer_entropy(&[], &[], 1);
    assert!(te.is_finite());
    assert!((te - 0.0).abs() < f64::EPSILON);
}

#[test]
fn chaos_te_single_element() {
    let te = transfer_entropy(&[0], &[1], 1);
    assert!(te.is_finite());
}

#[test]
fn chaos_te_history_exceeds_length() {
    let source = vec![0, 1, 0];
    let target = vec![1, 0, 1];
    let te = transfer_entropy(&source, &target, 100);
    assert!(te.is_finite());
    assert!((te - 0.0).abs() < f64::EPSILON);
}

#[test]
fn chaos_te_constant_source() {
    let source = vec![0u64; 100];
    let target: Vec<u64> = (0..100).map(|i| i % 3).collect();
    let te = transfer_entropy(&source, &target, 1);
    assert!(te.is_finite());
    assert!(te >= 0.0);
}

#[test]
fn chaos_te_constant_target() {
    let source: Vec<u64> = (0..100).map(|i| i % 5).collect();
    let target = vec![0u64; 100];
    let te = transfer_entropy(&source, &target, 1);
    assert!(te.is_finite());
    assert!(te >= 0.0);
}

#[test]
fn chaos_te_mismatched_lengths() {
    let source = vec![0u64, 1, 2];
    let target = vec![1u64, 0, 1, 2, 0, 1, 2, 0];
    let r = bidirectional_transfer_entropy(&source, &target, 1);
    assert!(r.te_source_to_target.is_finite());
    assert!(r.te_target_to_source.is_finite());
}

#[test]
fn chaos_te_zero_history() {
    let te = transfer_entropy(&[0, 1, 2], &[2, 1, 0], 0);
    assert!((te - 0.0).abs() < f64::EPSILON);
}

#[test]
fn chaos_te_very_long_series() {
    let source: Vec<u64> = (0..10_000).map(|i| i % 7).collect();
    let target: Vec<u64> = (0..10_000).map(|i| (i + 1) % 7).collect();
    let te = transfer_entropy(&source, &target, 1);
    assert!(te.is_finite());
    assert!(te >= 0.0);
}

// ---- bin_values chaos ----

#[test]
fn chaos_bin_values_all_nan() {
    let bins = bin_values(&[f64::NAN, f64::NAN, f64::NAN], 10);
    assert_eq!(bins.len(), 3);
    for &b in &bins {
        assert_eq!(b, 0);
    }
}

#[test]
fn chaos_bin_values_infinity() {
    let bins = bin_values(&[f64::INFINITY, f64::NEG_INFINITY, 0.0], 10);
    assert_eq!(bins.len(), 3);
}

#[test]
fn chaos_bin_values_zero_bins() {
    let bins = bin_values(&[1.0, 2.0, 3.0], 0);
    assert!(bins.is_empty());
}

#[test]
fn chaos_bin_values_subnormal() {
    let bins = bin_values(&[5e-324, 5e-324, 1.0], 10);
    assert_eq!(bins.len(), 3);
}

// -----------------------------------------------------------------------
// Total Correlation Chaos
// -----------------------------------------------------------------------

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

#[test]
fn chaos_tc_empty_records() {
    let result = total_correlation(&[]);
    assert!(result.tc_bits.is_finite());
    assert!((result.tc_bits - 0.0).abs() < f64::EPSILON);
}

#[test]
fn chaos_tc_single_record() {
    let records = make_records(&[&[("a", "1"), ("b", "2")]]);
    let result = total_correlation(&records);
    assert!(result.tc_bits.is_finite());
    assert!(result.tc_bits >= 0.0);
}

#[test]
fn chaos_tc_no_shared_fields() {
    let records = vec![
        {
            let mut m = BTreeMap::new();
            m.insert("a".to_owned(), "1".to_owned());
            m
        },
        {
            let mut m = BTreeMap::new();
            m.insert("b".to_owned(), "2".to_owned());
            m
        },
    ];
    let result = total_correlation(&records);
    assert!(result.tc_bits.is_finite());
}

#[test]
fn chaos_tc_many_fields() {
    // More than 8 fields — should be capped
    let records: Vec<BTreeMap<String, String>> = (0..10)
        .map(|i| {
            (0..20)
                .map(|j| (format!("field_{j}"), format!("val_{}", (i + j) % 5)))
                .collect()
        })
        .collect();
    let result = total_correlation(&records);
    assert!(result.tc_bits.is_finite());
    assert!(result.tc_bits >= 0.0);
    // Should have been capped to 8 fields
    assert!(result.field_count <= 8);
}

#[test]
fn chaos_tc_for_fields_nonexistent() {
    let records = make_records(&[&[("a", "1")], &[("a", "2")]]);
    let result = total_correlation_for_fields(&records, &["missing1", "missing2"]);
    assert!(result.tc_bits.is_finite());
}

#[test]
fn chaos_tc_all_identical_records() {
    let records = make_records(&[
        &[("x", "same"), ("y", "same"), ("z", "same")],
        &[("x", "same"), ("y", "same"), ("z", "same")],
        &[("x", "same"), ("y", "same"), ("z", "same")],
    ]);
    let result = total_correlation(&records);
    assert!(result.tc_bits.is_finite());
    assert!((result.tc_bits - 0.0).abs() < f64::EPSILON);
}
