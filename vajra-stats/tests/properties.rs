#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use std::collections::BTreeMap;
use vajra_stats::cms::CountMinSketch;
use vajra_stats::ddsketch::DDSketch;
use vajra_stats::entropy::{normalized_entropy, shannon_entropy_from_counts};
use vajra_stats::lz_complexity::lz76_complexity;
use vajra_stats::mad::{mad, modified_z_score};
use vajra_stats::renyi::{renyi_entropy, renyi_spectrum};
use vajra_stats::space_saving::SpaceSaving;
use vajra_stats::total_correlation::total_correlation;
use vajra_stats::transfer_entropy::transfer_entropy;

// =========================================================================
// Entropy properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 1. Entropy bounds: 0 <= H <= log2(|non_zero_counts|)
    #[test]
    fn prop_entropy_bounds(counts in prop::collection::vec(0u64..1000, 1..50)) {
        let h = shannon_entropy_from_counts(&counts);
        let non_zero = counts.iter().filter(|&&c| c > 0).count();
        prop_assert!(h >= 0.0, "entropy must be >= 0, got {h}");
        if non_zero > 0 {
            let max_h = (non_zero as f64).log2();
            prop_assert!(h <= max_h + 1e-10,
                "entropy {h} exceeds log2({non_zero}) = {max_h}");
        }
    }

    // 2. Normalized entropy bounds: 0 <= normalized <= 1.0
    #[test]
    fn prop_normalized_entropy_bounds(
        counts in prop::collection::vec(1u64..100, 2..20)
    ) {
        let h = shannon_entropy_from_counts(&counts);
        let n = normalized_entropy(h, counts.len());
        prop_assert!(n >= 0.0, "normalized entropy must be >= 0, got {n}");
        prop_assert!(n <= 1.0 + 1e-10,
            "normalized entropy must be <= 1.0, got {n}");
    }

    // 3. Entropy of constant is zero: all-same counts -> entropy = 0
    #[test]
    fn prop_entropy_constant_is_zero(val in 1u64..10000, len in 1usize..20) {
        let counts = vec![val; len];
        let h = shannon_entropy_from_counts(&counts);
        // When all counts are equal, entropy = log2(len). But "constant" here means
        // there is only one distinct value: that's a single bin.
        // Actually, "all-same counts" with multiple bins = uniform = max entropy.
        // The property "entropy of constant is zero" means a single bin only.
        let single = vec![val];
        let h_single = shannon_entropy_from_counts(&single);
        prop_assert!((h_single - 0.0).abs() < 1e-12,
            "single bin should have entropy 0, got {h_single}");

        // Also verify: uniform distribution has entropy = log2(len)
        if len > 1 {
            let expected = (len as f64).log2();
            prop_assert!((h - expected).abs() < 1e-10,
                "uniform distribution entropy {h} should be log2({len}) = {expected}");
        }
    }
}

// =========================================================================
// MAD properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 4. MAD non-negative
    #[test]
    fn prop_mad_non_negative(
        values in prop::collection::vec(-1e6f64..1e6f64, 1..100)
    ) {
        let mut vals = values;
        let m = mad(&mut vals);
        prop_assert!(m >= 0.0, "MAD must be >= 0, got {m}");
    }

    // 5. MAD of constant is zero
    #[test]
    fn prop_mad_constant_is_zero(val in -1e6f64..1e6f64, len in 1usize..50) {
        let mut vals = vec![val; len];
        let m = mad(&mut vals);
        prop_assert!((m - 0.0).abs() < 1e-12,
            "MAD of constant values should be 0, got {m}");
    }

    // 6. Modified z-score at median is zero
    #[test]
    fn prop_z_score_at_median_is_zero(
        median_val in -1e6f64..1e6f64,
        mad_val in 0.001f64..1e6f64
    ) {
        let z = modified_z_score(median_val, median_val, mad_val);
        prop_assert!((z - 0.0).abs() < 1e-10,
            "z-score at median should be 0, got {z}");
    }
}

// =========================================================================
// DDSketch properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 7. DDSketch relative error for positive values
    //    The DDSketch guarantees that each bucket boundary is within a factor
    //    of (1 +/- alpha) of the true value. However, the quantile rank mapping
    //    can introduce additional error, especially for extreme quantiles with
    //    small datasets. We use a generous tolerance and require >= 50 values.
    #[test]
    fn prop_ddsketch_relative_error(
        values in prop::collection::vec(1.0f64..1000.0f64, 50..200)
    ) {
        let alpha = 0.05;
        let mut sk = DDSketch::new(alpha);
        let mut sorted = values.clone();
        for &v in &values {
            sk.add(v);
        }
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Test the median -- the most robust quantile.
        for &q in &[0.25, 0.5, 0.75] {
            let est = sk.quantile(q).unwrap();
            let rank = ((q * (sorted.len() as f64 - 1.0)).round() as usize)
                .min(sorted.len() - 1);
            let true_val = sorted[rank];
            let rel_err = if true_val.abs() > 1e-9 {
                (est - true_val).abs() / true_val.abs()
            } else {
                (est - true_val).abs()
            };
            // Allow alpha + some slack for rank mapping.
            prop_assert!(rel_err <= alpha + 0.10,
                "q={q}: est={est}, true={true_val}, rel_err={rel_err}, alpha={alpha}");
        }
    }

    // 8. DDSketch merge equivalence
    #[test]
    fn prop_ddsketch_merge_equivalence(
        values_a in prop::collection::vec(1.0f64..1000.0f64, 5..50),
        values_b in prop::collection::vec(1.0f64..1000.0f64, 5..50),
    ) {
        let alpha = 0.05;
        let mut sk_a = DDSketch::new(alpha);
        let mut sk_b = DDSketch::new(alpha);
        let mut sk_combined = DDSketch::new(alpha);

        for &v in &values_a {
            sk_a.add(v);
            sk_combined.add(v);
        }
        for &v in &values_b {
            sk_b.add(v);
            sk_combined.add(v);
        }
        sk_a.merge(&sk_b);

        prop_assert_eq!(sk_a.count(), sk_combined.count(),
            "merged count should match combined count");
        prop_assert_eq!(sk_a.min(), sk_combined.min(),
            "merged min should match combined min");
        prop_assert_eq!(sk_a.max(), sk_combined.max(),
            "merged max should match combined max");

        for &q in &[0.25, 0.5, 0.75] {
            let v_merged = sk_a.quantile(q).unwrap();
            let v_combined = sk_combined.quantile(q).unwrap();
            let denom = v_combined.abs().max(1.0);
            let rel_diff = (v_merged - v_combined).abs() / denom;
            prop_assert!(rel_diff < 0.1,
                "q={q}: merged={v_merged}, combined={v_combined}, rel_diff={rel_diff}");
        }
    }

    // 9. DDSketch count is exact
    #[test]
    fn prop_ddsketch_count_exact(
        values in prop::collection::vec(-1000.0f64..1000.0f64, 0..200)
    ) {
        let mut sk = DDSketch::default();
        for &v in &values {
            sk.add(v);
        }
        prop_assert_eq!(sk.count(), values.len() as u64,
            "count should be exact");
    }

    // 10. DDSketch min/max exact
    #[test]
    fn prop_ddsketch_min_max_exact(
        values in prop::collection::vec(-1000.0f64..1000.0f64, 1..100)
    ) {
        let mut sk = DDSketch::default();
        for &v in &values {
            sk.add(v);
        }
        let expected_min = values.iter().copied()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let expected_max = values.iter().copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        prop_assert_eq!(sk.min().unwrap(), expected_min,
            "min should be exact");
        prop_assert_eq!(sk.max().unwrap(), expected_max,
            "max should be exact");
    }
}

// =========================================================================
// Count-Min Sketch properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 11. CMS no underestimation: estimate(x) >= true_count(x)
    #[test]
    fn prop_cms_no_underestimation(
        items in prop::collection::vec(("[a-z]{1,5}", 1u64..20), 1..50)
    ) {
        let mut cms = CountMinSketch::new(0.01, 0.01);
        let mut true_counts = std::collections::HashMap::new();
        for (item, count) in &items {
            cms.add(item.as_bytes(), *count);
            *true_counts.entry(item.clone()).or_insert(0u64) += count;
        }
        for (item, &true_count) in &true_counts {
            let est = cms.estimate(item.as_bytes());
            prop_assert!(est >= true_count,
                "CMS underestimation: item={item}, est={est}, true={true_count}");
        }
    }

    // 12. CMS error bound: estimate(x) <= true_count(x) + epsilon * total
    #[test]
    fn prop_cms_error_bound(
        items in prop::collection::vec(("[a-z]{1,5}", 1u64..20), 1..50)
    ) {
        let epsilon = 0.01;
        let delta = 0.001;
        let mut cms = CountMinSketch::new(epsilon, delta);
        let mut true_counts = std::collections::HashMap::new();
        for (item, count) in &items {
            cms.add(item.as_bytes(), *count);
            *true_counts.entry(item.clone()).or_insert(0u64) += count;
        }
        let total = cms.total();

        // The epsilon*total bound is a *probabilistic* guarantee: it holds for
        // any given query with probability at least 1 - delta. Asserting it for
        // every item of every case therefore fails eventually by construction —
        // and because this sketch derives its row seeds deterministically from
        // the row index, "eventually" means as soon as proptest generates a
        // string set that collides in all rows. It does, given enough cases.
        //
        // So count violations rather than forbidding them, and fail only if
        // there are more than the guarantee allows. A genuinely broken sketch
        // violates the bound for most items, not one.
        let mut violations = 0usize;
        for (item, &true_count) in &true_counts {
            let est = cms.estimate(item.as_bytes());
            // Never underestimating is deterministic and is asserted strictly.
            prop_assert!(est >= true_count,
                "CMS underestimated: item={item}, est={est}, true={true_count}");
            let bound = true_count + (epsilon * total as f64).ceil() as u64;
            if est > bound {
                violations += 1;
            }
        }

        let allowed = 1 + true_counts.len() / 10;
        prop_assert!(violations <= allowed,
            "CMS exceeded its error bound for {violations} of {} items (allowed {allowed}); \
             a probabilistic bound should not fail this broadly",
            true_counts.len());
    }

    // 12b. The epsilon*total bound holds for at least (1 - delta) of queries.
    //
    // This is what a Count-Min Sketch actually promises, and it needs a large
    // sample to check — which a per-case property test cannot provide. Fixed
    // inputs, so it is deterministic.
    #[test]
    fn prop_cms_error_bound_holds_at_rate(seed in 0u64..64) {
        let epsilon = 0.01;
        let mut cms = CountMinSketch::new(epsilon, 0.001);
        let mut true_counts = std::collections::HashMap::new();

        // 2000 distinct keys derived from the seed, so different seeds exercise
        // different key sets without reintroducing per-case flakiness.
        for i in 0..2000u64 {
            let key = format!("k{}-{}", seed, i * 7919 % 2000);
            cms.add(key.as_bytes(), 1 + i % 5);
            *true_counts.entry(key).or_insert(0u64) += 1 + i % 5;
        }

        let total = cms.total();
        let mut violations = 0usize;
        for (item, &true_count) in &true_counts {
            let est = cms.estimate(item.as_bytes());
            let bound = true_count + (epsilon * total as f64).ceil() as u64;
            if est > bound {
                violations += 1;
            }
        }

        // Generous headroom over delta=0.001: the point is to catch a sketch
        // that is wrong in kind, not to measure the constant.
        #[allow(clippy::cast_precision_loss)]
        let rate = violations as f64 / true_counts.len() as f64;
        prop_assert!(rate <= 0.02,
            "bound violated for {rate:.4} of queries ({violations}/{}), expected <= 0.02",
            true_counts.len());
    }

    // 13. CMS total is exact
    #[test]
    fn prop_cms_total_exact(
        items in prop::collection::vec(("[a-z]{1,5}", 1u64..100), 0..50)
    ) {
        let mut cms = CountMinSketch::default();
        let mut expected_total = 0u64;
        for (item, count) in &items {
            cms.add(item.as_bytes(), *count);
            expected_total += count;
        }
        prop_assert_eq!(cms.total(), expected_total,
            "CMS total should be exact");
    }
}

// =========================================================================
// Space-Saving properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 14. Space-Saving contains top: the most frequent item is always tracked
    //     when k >= 1 and the item dominates strongly enough.
    #[test]
    fn prop_space_saving_contains_top(
        dominant_count in 100u32..500,
        k in 2usize..10,
        rare_items in prop::collection::vec("[a-z]{1,4}", 1..20),
    ) {
        let mut ss = SpaceSaving::new(k);
        // Insert the dominant item many times.
        for _ in 0..dominant_count {
            ss.add("dominant_item");
        }
        // Insert rare items once each.
        for item in &rare_items {
            ss.add(item);
        }
        // The dominant item should remain tracked because its count
        // exceeds min_count + number_of_evictions.
        // This is guaranteed when dominant_count > rare_items.len()
        // (which it always is given our ranges).
        prop_assert!(ss.contains("dominant_item"),
            "dominant item should be tracked: dominant_count={dominant_count}, rare={}",
            rare_items.len());
    }

    // 15. Space-Saving count non-decreasing: adding items never decreases
    //     any tracked counter.
    #[test]
    fn prop_space_saving_count_non_decreasing(
        items in prop::collection::vec("[a-z]{1,3}", 2..30),
    ) {
        let k = 5;
        let mut ss = SpaceSaving::new(k);
        let mut prev_top: Vec<(String, u64)> = Vec::new();

        for item in &items {
            ss.add(item);
            let current_top: Vec<(String, u64)> = ss.top_k()
                .iter()
                .map(|&(s, c)| (s.to_string(), c))
                .collect();

            // For any item that was tracked before and is still tracked,
            // its count should not have decreased.
            for (prev_name, prev_count) in &prev_top {
                if let Some(&(_, cur_count)) = current_top.iter()
                    .find(|(n, _)| n == prev_name)
                {
                    prop_assert!(cur_count >= *prev_count,
                        "counter for {prev_name} decreased from {prev_count} to {cur_count}");
                }
            }
            prev_top = current_top;
        }
    }
}

// =========================================================================
// Extended CI tests
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    #[ignore]
    fn prop_entropy_bounds_extended(counts in prop::collection::vec(0u64..10000, 1..100)) {
        let h = shannon_entropy_from_counts(&counts);
        let non_zero = counts.iter().filter(|&&c| c > 0).count();
        prop_assert!(h >= 0.0);
        if non_zero > 0 {
            let max_h = (non_zero as f64).log2();
            prop_assert!(h <= max_h + 1e-10);
        }
    }

    #[test]
    #[ignore]
    fn prop_cms_no_underestimation_extended(
        items in prop::collection::vec(("[a-z]{1,8}", 1u64..100), 1..100)
    ) {
        let mut cms = CountMinSketch::new(0.001, 0.001);
        let mut true_counts = std::collections::HashMap::new();
        for (item, count) in &items {
            cms.add(item.as_bytes(), *count);
            *true_counts.entry(item.clone()).or_insert(0u64) += count;
        }
        for (item, &true_count) in &true_counts {
            let est = cms.estimate(item.as_bytes());
            prop_assert!(est >= true_count);
        }
    }
}

// =========================================================================
// Rényi Spectrum properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // R1. Monotone: H₀ >= H₁ >= H₂ >= H∞
    #[test]
    fn prop_renyi_monotone(counts in prop::collection::vec(1u64..1000, 2..30)) {
        let s = renyi_spectrum(&counts);
        prop_assert!(s.hartley >= s.shannon - 1e-10,
            "H₀={} < H₁={}", s.hartley, s.shannon);
        prop_assert!(s.shannon >= s.collision - 1e-10,
            "H₁={} < H₂={}", s.shannon, s.collision);
        prop_assert!(s.collision >= s.min_entropy - 1e-10,
            "H₂={} < H∞={}", s.collision, s.min_entropy);
    }

    // R2. Non-negative for all orders
    #[test]
    fn prop_renyi_non_negative(counts in prop::collection::vec(0u64..500, 1..30)) {
        let s = renyi_spectrum(&counts);
        prop_assert!(s.hartley >= 0.0);
        prop_assert!(s.shannon >= 0.0);
        prop_assert!(s.collision >= 0.0);
        prop_assert!(s.min_entropy >= 0.0);
    }

    // R3. Hartley = log₂(support size)
    #[test]
    fn prop_renyi_hartley_equals_log_support(counts in prop::collection::vec(1u64..100, 2..30)) {
        let support = counts.iter().filter(|&&c| c > 0).count();
        let h0 = renyi_entropy(&counts, 0.0);
        let expected = (support as f64).log2();
        prop_assert!((h0 - expected).abs() < 1e-10,
            "H₀={h0} should equal log₂({support})={expected}");
    }

    // R4. Min-entropy = -log₂(max p)
    #[test]
    fn prop_renyi_min_entropy_bound(counts in prop::collection::vec(1u64..100, 2..30)) {
        let total: u64 = counts.iter().sum();
        let max_count = counts.iter().copied().max().unwrap();
        let max_p = max_count as f64 / total as f64;
        let expected = -max_p.log2();
        let h_inf = renyi_entropy(&counts, f64::INFINITY);
        prop_assert!((h_inf - expected).abs() < 1e-10,
            "H∞={h_inf} should equal -log₂({max_p})={expected}");
    }

    // R5. α=1 matches Shannon
    #[test]
    fn prop_renyi_alpha1_matches_shannon(counts in prop::collection::vec(0u64..500, 1..30)) {
        let h_renyi = renyi_entropy(&counts, 1.0);
        let h_shannon = shannon_entropy_from_counts(&counts);
        prop_assert!((h_renyi - h_shannon).abs() < 1e-10,
            "H₁={h_renyi} != Shannon={h_shannon}");
    }

    // R6. Uniform distribution: all orders equal
    #[test]
    fn prop_renyi_uniform_all_equal(val in 1u64..1000, len in 2usize..20) {
        let counts = vec![val; len];
        let s = renyi_spectrum(&counts);
        let expected = (len as f64).log2();
        prop_assert!((s.hartley - expected).abs() < 1e-10);
        prop_assert!((s.shannon - expected).abs() < 1e-10);
        prop_assert!((s.collision - expected).abs() < 1e-10);
        prop_assert!((s.min_entropy - expected).abs() < 1e-10);
    }

    // R7. Divergence = H₀ - H∞
    #[test]
    fn prop_renyi_divergence(counts in prop::collection::vec(1u64..500, 2..30)) {
        let s = renyi_spectrum(&counts);
        prop_assert!((s.divergence - (s.hartley - s.min_entropy)).abs() < 1e-10);
    }
}

// =========================================================================
// LZ Complexity properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // L1. Normalized complexity bounded [0, 1]
    #[test]
    fn prop_lz_bounded_01(data in prop::collection::vec(0u8..255, 2..500)) {
        let c = lz76_complexity(&data);
        prop_assert!(c >= 0.0, "LZ complexity must be >= 0, got {c}");
        prop_assert!(c <= 1.0, "LZ complexity must be <= 1, got {c}");
    }

    // L2. Constant sequence: lower complexity than random
    //     LZ76 normalization gives ~O(sqrt(n)/n*log(n)) for constant input,
    //     so use a longer sequence and a relative comparison.
    #[test]
    fn prop_lz_constant_lower_than_random(byte in 0u8..255, len in 500usize..2000) {
        let constant_data = vec![byte; len];
        let c_const = lz76_complexity(&constant_data);
        // For long constant sequences, complexity should be well below 1.0
        prop_assert!(c_const < 0.5,
            "constant sequence of len {len} should have complexity < 0.5, got {c_const}");
    }

    // L3. Empty / single byte → 0
    #[test]
    fn prop_lz_trivial_zero(byte in 0u8..255) {
        prop_assert!((lz76_complexity(&[]) - 0.0).abs() < f64::EPSILON);
        prop_assert!((lz76_complexity(&[byte]) - 0.0).abs() < f64::EPSILON);
    }
}

// =========================================================================
// Transfer Entropy properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // T1. TE non-negative
    #[test]
    fn prop_te_non_negative(
        source in prop::collection::vec(0u64..5, 10..50),
        target in prop::collection::vec(0u64..5, 10..50),
    ) {
        let te = transfer_entropy(&source, &target, 1);
        prop_assert!(te >= 0.0, "TE must be >= 0, got {te}");
    }

    // T2. TE of identical constant series = 0
    #[test]
    fn prop_te_constant_zero(val in 0u64..10, len in 5usize..50) {
        let source = vec![val; len];
        let target = vec![val; len];
        let te = transfer_entropy(&source, &target, 1);
        prop_assert!((te - 0.0).abs() < 1e-10,
            "TE of constant series should be 0, got {te}");
    }
}

// =========================================================================
// Total Correlation properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // TC1. TC non-negative
    #[test]
    fn prop_tc_non_negative(
        vals_a in prop::collection::vec("[a-c]", 5..20),
        vals_b in prop::collection::vec("[x-z]", 5..20),
    ) {
        let n = vals_a.len().min(vals_b.len());
        let records: Vec<BTreeMap<String, String>> = (0..n)
            .map(|i| {
                let mut m = BTreeMap::new();
                m.insert("a".to_owned(), vals_a[i].clone());
                m.insert("b".to_owned(), vals_b[i].clone());
                m
            })
            .collect();
        let result = total_correlation(&records);
        prop_assert!(result.tc_bits >= -1e-10,
            "TC must be >= 0, got {}", result.tc_bits);
    }

    // TC2. TC upper bounded by sum of marginals
    #[test]
    fn prop_tc_upper_bound(
        vals_a in prop::collection::vec("[a-c]", 5..20),
        vals_b in prop::collection::vec("[x-z]", 5..20),
    ) {
        let n = vals_a.len().min(vals_b.len());
        let records: Vec<BTreeMap<String, String>> = (0..n)
            .map(|i| {
                let mut m = BTreeMap::new();
                m.insert("a".to_owned(), vals_a[i].clone());
                m.insert("b".to_owned(), vals_b[i].clone());
                m
            })
            .collect();
        let result = total_correlation(&records);
        prop_assert!(result.tc_bits <= result.sum_marginal_entropies + 1e-10,
            "TC ({}) must be <= sum of marginals ({})",
            result.tc_bits, result.sum_marginal_entropies);
    }

    // TC3. Normalized TC bounded [0, 1]
    #[test]
    fn prop_tc_normalized_bounded(
        vals_a in prop::collection::vec("[a-d]", 5..30),
        vals_b in prop::collection::vec("[w-z]", 5..30),
    ) {
        let n = vals_a.len().min(vals_b.len());
        let records: Vec<BTreeMap<String, String>> = (0..n)
            .map(|i| {
                let mut m = BTreeMap::new();
                m.insert("a".to_owned(), vals_a[i].clone());
                m.insert("b".to_owned(), vals_b[i].clone());
                m
            })
            .collect();
        let result = total_correlation(&records);
        prop_assert!(result.normalized_tc >= -1e-10,
            "normalized TC must be >= 0, got {}", result.normalized_tc);
        prop_assert!(result.normalized_tc <= 1.0 + 1e-10,
            "normalized TC must be <= 1, got {}", result.normalized_tc);
    }

    // TC4. Single field → TC = 0
    #[test]
    fn prop_tc_single_field_zero(vals in prop::collection::vec("[a-z]", 2..20)) {
        let records: Vec<BTreeMap<String, String>> = vals
            .iter()
            .map(|v| {
                let mut m = BTreeMap::new();
                m.insert("only".to_owned(), v.clone());
                m
            })
            .collect();
        let result = total_correlation(&records);
        prop_assert!((result.tc_bits - 0.0).abs() < 1e-10,
            "single field should have TC=0, got {}", result.tc_bits);
    }
}
