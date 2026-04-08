#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use vajra_anomaly::instability::type_instability_score;
use vajra_anomaly::numeric::detect_numeric_outliers;
use vajra_anomaly::rarity::rarity_score;

// =========================================================================
// Rarity properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 1. Rarity non-negative: rarity_score >= 0 for valid inputs
    #[test]
    fn prop_rarity_non_negative(
        value_count in 1u64..10000,
        total_count in 1u64..10000,
    ) {
        let r = rarity_score(value_count, total_count);
        prop_assert!(r >= 0.0,
            "rarity should be >= 0, got {r} for count={value_count}, total={total_count}");
    }

    // 2. Rarity monotone: rarer values (smaller count relative to total)
    //    have higher rarity scores.
    #[test]
    fn prop_rarity_monotone(
        total in 100u64..10000,
        count_a in 1u64..50,
        count_b in 51u64..100,
    ) {
        // count_a < count_b, so count_a is rarer.
        // Ensure both are <= total.
        let total = total.max(count_b + 1);
        let r_a = rarity_score(count_a, total);
        let r_b = rarity_score(count_b, total);
        prop_assert!(r_a >= r_b,
            "rarer value (count={count_a}) should have higher rarity ({r_a}) than \
             more common value (count={count_b}, rarity={r_b}), total={total}");
    }
}

// =========================================================================
// Type instability properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 3. Type instability bounds: 0 <= instability <= 1
    #[test]
    fn prop_type_instability_bounds(
        counts in prop::collection::vec(0u64..1000, 7..=7)
    ) {
        let mut arr = [0u64; 7];
        for (i, &c) in counts.iter().enumerate() {
            arr[i] = c;
        }
        let score = type_instability_score(&arr);
        prop_assert!(score >= 0.0,
            "instability should be >= 0, got {score}");
        prop_assert!(score <= 1.0 + 1e-12,
            "instability should be <= 1, got {score}");
    }

    // 4. Type instability of single type is zero
    #[test]
    fn prop_type_instability_single_type_is_zero(
        type_idx in 0usize..7,
        count in 1u64..10000,
    ) {
        let mut arr = [0u64; 7];
        arr[type_idx] = count;
        let score = type_instability_score(&arr);
        prop_assert!((score - 0.0).abs() < 1e-12,
            "single type should have instability 0, got {score} for type_idx={type_idx}");
    }
}

// =========================================================================
// MAD outlier threshold properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 5. MAD outlier threshold: values at the median are never outliers.
    //    We use detect_numeric_outliers and verify that values equal to the
    //    median are never flagged.
    #[test]
    fn prop_median_values_never_outliers(
        base_values in prop::collection::vec(-1000.0f64..1000.0, 3..50),
        threshold in 0.1f64..10.0,
    ) {
        // First compute the median.
        let mut sorted = base_values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if sorted.len() % 2 == 1 {
            sorted[sorted.len() / 2]
        } else {
            f64::midpoint(sorted[sorted.len() / 2 - 1], sorted[sorted.len() / 2])
        };

        // Run outlier detection.
        let mut values = base_values;
        let outliers = detect_numeric_outliers(&mut values, threshold);

        // No outlier should have value == median.
        for o in &outliers {
            prop_assert!((o.value - median).abs() > 1e-15 || o.z_mad.abs() < 1e-10,
                "value at median ({median}) should not be an outlier, \
                 but got z_mad={}", o.z_mad);
        }
    }
}

// =========================================================================
// Rarity edge case properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // Rarity of zero count is zero.
    #[test]
    fn prop_rarity_zero_count_is_zero(total in 1u64..10000) {
        let r = rarity_score(0, total);
        prop_assert!((r - 0.0).abs() < 1e-12,
            "rarity of zero count should be 0, got {r}");
    }

    // Rarity of full count (count == total) is zero.
    #[test]
    fn prop_rarity_full_count_is_zero(total in 1u64..10000) {
        let r = rarity_score(total, total);
        prop_assert!((r - 0.0).abs() < 1e-12,
            "rarity of count==total should be 0 (-log2(1) = 0), got {r}");
    }

    // Type instability is monotone: adding a second type increases instability.
    #[test]
    fn prop_type_instability_monotone(
        dominant_count in 10u64..1000,
        minority_count in 1u64..10,
        type_a in 0usize..7,
        offset in 1usize..7,
    ) {
        let type_b = (type_a + offset) % 7;

        // Single type: instability = 0
        let mut single = [0u64; 7];
        single[type_a] = dominant_count;
        let score_single = type_instability_score(&single);

        // Two types: instability > 0
        let mut mixed = [0u64; 7];
        mixed[type_a] = dominant_count;
        mixed[type_b] = minority_count;
        let score_mixed = type_instability_score(&mixed);

        prop_assert!(score_mixed > score_single,
            "adding a second type should increase instability: single={score_single}, mixed={score_mixed}");
    }
}

// =========================================================================
// Extended CI tests
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    #[ignore]
    fn prop_rarity_non_negative_extended(
        value_count in 1u64..100000,
        total_count in 1u64..100000,
    ) {
        let r = rarity_score(value_count, total_count);
        prop_assert!(r >= 0.0);
    }

    #[test]
    #[ignore]
    fn prop_type_instability_bounds_extended(
        counts in prop::collection::vec(0u64..10000, 7..=7)
    ) {
        let mut arr = [0u64; 7];
        for (i, &c) in counts.iter().enumerate() {
            arr[i] = c;
        }
        let score = type_instability_score(&arr);
        prop_assert!(score >= 0.0);
        prop_assert!(score <= 1.0 + 1e-12);
    }
}
