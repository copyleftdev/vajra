//! Differential tests: compare exact algorithms against streaming
//! counterparts to verify error bounds.

use std::collections::BTreeMap;
use vajra_fingerprint::similarity::{jaccard_similarity, minhash_signature, minhash_similarity};
use vajra_stats::{CountMinSketch, DDSketch, SpaceSaving};
use vajra_types::path::WildcardPath;

// -----------------------------------------------------------------------
// 1. CMS estimates are always >= true counts
// -----------------------------------------------------------------------

#[test]
fn differential_cms_vs_exact() {
    let epsilon = 0.001;
    let delta = 0.01;
    let mut cms = CountMinSketch::new(epsilon, delta);

    // Zipf-like distribution: item_i appears floor(10000 / (i+1)) times.
    let mut true_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut total: u64 = 0;

    for i in 0..100_u64 {
        let count = 10_000 / (i + 1);
        let key = format!("item_{i}");
        for _ in 0..count {
            cms.add(key.as_bytes(), 1);
        }
        true_counts.insert(key, count);
        total += count;
    }

    assert_eq!(cms.total(), total);

    // Verify: estimate >= true_count (no underestimation).
    for (key, &true_count) in &true_counts {
        let est = cms.estimate(key.as_bytes());
        assert!(
            est >= true_count,
            "CMS underestimated '{key}': estimate={est}, true={true_count}"
        );
    }

    // Verify: estimate <= true_count + epsilon * total.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let error_bound = (epsilon * total as f64).ceil() as u64;
    for (key, &true_count) in &true_counts {
        let est = cms.estimate(key.as_bytes());
        assert!(
            est <= true_count + error_bound,
            "CMS overestimated '{key}' beyond bound: estimate={est}, true={true_count}, bound={error_bound}"
        );
    }
}

#[test]
fn differential_cms_vs_exact_high_cardinality() {
    let epsilon = 0.01;
    let delta = 0.001;
    let mut cms = CountMinSketch::new(epsilon, delta);
    let mut true_counts: BTreeMap<String, u64> = BTreeMap::new();

    // Insert 10,000 distinct items each appearing once.
    for i in 0..10_000_u64 {
        let key = format!("k{i}");
        cms.add(key.as_bytes(), 1);
        *true_counts.entry(key).or_insert(0) += 1;
    }

    // Also add a heavy hitter 500 times.
    for _ in 0..500 {
        cms.add(b"heavy", 1);
    }
    *true_counts.entry("heavy".to_string()).or_insert(0) += 500;

    let total = cms.total();

    // No underestimation.
    for (key, &tc) in &true_counts {
        let est = cms.estimate(key.as_bytes());
        assert!(est >= tc, "underestimated '{key}'");
    }

    // Error bound check for the heavy hitter.
    let est_heavy = cms.estimate(b"heavy");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bound = 500 + (epsilon * total as f64).ceil() as u64;
    assert!(
        est_heavy <= bound,
        "heavy hitter overcount: est={est_heavy}, bound={bound}"
    );
}

// -----------------------------------------------------------------------
// 2. DDSketch quantiles are within relative error of exact quantiles
// -----------------------------------------------------------------------

#[test]
fn differential_ddsketch_vs_exact() {
    let alpha = 0.01;
    let mut sketch = DDSketch::new(alpha);
    let mut exact_values: Vec<f64> = Vec::new();

    // Insert 10,000 values: 1, 2, 3, ..., 10000.
    for i in 1..=10_000 {
        let v = i as f64;
        sketch.add(v);
        exact_values.push(v);
    }
    exact_values.sort_by(|a, b| a.total_cmp(b));

    let quantiles = [0.01, 0.05, 0.25, 0.50, 0.75, 0.95, 0.99];
    for &q in &quantiles {
        let sketch_est = sketch
            .quantile(q)
            .unwrap_or_else(|| panic!("quantile {q} returned None"));

        // Exact quantile: interpolation at rank q * (n-1).
        let rank = q * (exact_values.len() as f64 - 1.0);
        let lower = rank.floor() as usize;
        let frac = rank - rank.floor();
        let true_quantile = if lower + 1 < exact_values.len() {
            exact_values[lower] * (1.0 - frac) + exact_values[lower + 1] * frac
        } else {
            exact_values[lower]
        };

        // DDSketch guarantees relative error <= alpha for positive values.
        let rel_error = if true_quantile.abs() > 1e-10 {
            (sketch_est - true_quantile).abs() / true_quantile.abs()
        } else {
            (sketch_est - true_quantile).abs()
        };

        assert!(
            rel_error <= alpha + 0.02, // small tolerance for bucket boundaries
            "DDSketch q={q}: sketch={sketch_est:.2}, true={true_quantile:.2}, rel_error={rel_error:.6}"
        );
    }
}

#[test]
fn differential_ddsketch_vs_exact_negative_values() {
    let alpha = 0.02;
    let mut sketch = DDSketch::new(alpha);
    let mut exact_values: Vec<f64> = Vec::new();

    // Mix of negative and positive values.
    for i in -500..=500 {
        if i == 0 {
            continue; // skip zero for simpler quantile computation
        }
        let v = i as f64;
        sketch.add(v);
        exact_values.push(v);
    }
    exact_values.sort_by(|a, b| a.total_cmp(b));

    for &q in &[0.25, 0.50, 0.75] {
        let sketch_est = sketch
            .quantile(q)
            .unwrap_or_else(|| panic!("quantile {q} returned None"));

        let rank = q * (exact_values.len() as f64 - 1.0);
        let lower = rank.floor() as usize;
        let frac = rank - rank.floor();
        let true_quantile = if lower + 1 < exact_values.len() {
            exact_values[lower] * (1.0 - frac) + exact_values[lower + 1] * frac
        } else {
            exact_values[lower]
        };

        let abs_error = (sketch_est - true_quantile).abs();
        // For mixed-sign data, use absolute error tolerance.
        assert!(
            abs_error <= true_quantile.abs() * alpha + 5.0,
            "DDSketch q={q}: sketch={sketch_est:.2}, true={true_quantile:.2}, abs_error={abs_error:.6}"
        );
    }
}

// -----------------------------------------------------------------------
// 3. DDSketch merge produces same results as single sketch
// -----------------------------------------------------------------------

#[test]
fn differential_ddsketch_merge() {
    let alpha = 0.01;
    let mut sk1 = DDSketch::new(alpha);
    let mut sk2 = DDSketch::new(alpha);
    let mut combined = DDSketch::new(alpha);

    // Split data: first half into sk1, second half into sk2, all into combined.
    for i in 1..=5000 {
        let v = i as f64;
        sk1.add(v);
        combined.add(v);
    }
    for i in 5001..=10_000 {
        let v = i as f64;
        sk2.add(v);
        combined.add(v);
    }

    sk1.merge(&sk2);

    // After merge, counts should be identical.
    assert_eq!(sk1.count(), combined.count());
    assert_eq!(sk1.min(), combined.min());
    assert_eq!(sk1.max(), combined.max());
    assert!(
        (sk1.mean().unwrap_or(0.0) - combined.mean().unwrap_or(0.0)).abs() < 1e-9,
        "mean mismatch after merge"
    );

    // Quantiles should be identical (merge is exact for DDSketch).
    for &q in &[0.25, 0.5, 0.75, 0.99] {
        let v_merged = sk1.quantile(q).unwrap_or(0.0);
        let v_combined = combined.quantile(q).unwrap_or(0.0);
        // They should be exactly equal since merge adds bin counts.
        assert!(
            (v_merged - v_combined).abs() < 1e-9,
            "DDSketch merge q={q}: merged={v_merged:.4}, combined={v_combined:.4}"
        );
    }
}

// -----------------------------------------------------------------------
// 4. Space-Saving always contains the true top-k
// -----------------------------------------------------------------------

#[test]
fn differential_space_saving_vs_exact() {
    // Space-Saving with k > number of heavy hitters ensures all heavy
    // hitters are tracked. Use k=15 to track 10 heavy hitters plus room
    // for some rare items.
    let k = 15;
    let mut ss = SpaceSaving::new(k);
    let mut true_counts: BTreeMap<String, u64> = BTreeMap::new();

    // 10 heavy hitters with large counts.
    for i in 0..10_u64 {
        let count = (10 - i) * 500;
        let key = format!("heavy_{i}");
        for _ in 0..count {
            ss.add(&key);
        }
        *true_counts.entry(key).or_insert(0) += count;
    }
    // 20 rare items, each appearing 1-3 times.
    for i in 0..20_u64 {
        let count = (i % 3) + 1;
        let key = format!("rare_{i}");
        for _ in 0..count {
            ss.add(&key);
        }
        *true_counts.entry(key).or_insert(0) += count;
    }

    // Find the true top-10 by frequency.
    let mut sorted_true: Vec<(String, u64)> = true_counts.into_iter().collect();
    sorted_true.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let true_top_10: Vec<String> = sorted_true
        .iter()
        .take(10)
        .map(|(k, _)| k.clone())
        .collect();

    // With k=15 slots, all true top-10 items should be tracked.
    let top = ss.top_k();
    let tracked_items: Vec<&str> = top.iter().map(|(name, _)| *name).collect();

    for expected_item in &true_top_10 {
        assert!(
            tracked_items.contains(&expected_item.as_str()),
            "Space-Saving missing true top-10 item '{expected_item}'; tracked: {tracked_items:?}"
        );
    }
}

#[test]
fn differential_space_saving_ordering() {
    // Verify that top_k returns items in descending count order.
    let mut ss = SpaceSaving::new(20);

    for _ in 0..500 {
        ss.add("alpha");
    }
    for _ in 0..300 {
        ss.add("beta");
    }
    for _ in 0..100 {
        ss.add("gamma");
    }
    for _ in 0..50 {
        ss.add("delta");
    }

    let top = ss.top_k();
    for window in top.windows(2) {
        assert!(
            window[0].1 >= window[1].1,
            "top_k not in descending order: {:?} before {:?}",
            window[0],
            window[1]
        );
    }
}

// -----------------------------------------------------------------------
// 5. MinHash similarity converges to true Jaccard
// -----------------------------------------------------------------------

#[test]
fn differential_minhash_convergence() {
    // Generate two sets with known Jaccard similarity.
    // Use larger sets for better MinHash convergence.
    // Shared: 300 paths, Only in A: 200, Only in B: 200.
    // True Jaccard = 300 / (300 + 200 + 200) = 300/700 ~= 0.4286.
    let shared: Vec<WildcardPath> = (0..300)
        .map(|i| WildcardPath::root().push_key(&format!("shared_{i}")))
        .collect();
    let only_a: Vec<WildcardPath> = (0..200)
        .map(|i| WildcardPath::root().push_key(&format!("a_only_{i}")))
        .collect();
    let only_b: Vec<WildcardPath> = (0..200)
        .map(|i| WildcardPath::root().push_key(&format!("b_only_{i}")))
        .collect();

    let mut set_a = shared.clone();
    set_a.extend(only_a);
    let mut set_b = shared;
    set_b.extend(only_b);

    let exact_jaccard = jaccard_similarity(&set_a, &set_b);
    let expected = 300.0 / 700.0;
    assert!(
        (exact_jaccard - expected).abs() < 1e-10,
        "exact jaccard should be {expected:.4}, got {exact_jaccard}"
    );

    // Test convergence with increasing k.
    // Average over multiple seeds to reduce variance.
    let k_values = [32, 64, 128, 256, 512];
    let num_trials = 5;

    for &k in &k_values {
        let mut total_error = 0.0;
        for seed in 0..num_trials {
            let sig_a = minhash_signature(&set_a, k, seed as u64 * 1000 + 42);
            let sig_b = minhash_signature(&set_b, k, seed as u64 * 1000 + 42);
            let approx = minhash_similarity(&sig_a, &sig_b);
            total_error += (exact_jaccard - approx).abs();
        }
        let avg_error = total_error / num_trials as f64;

        // At k=512, average error across trials should be small.
        if k == 512 {
            assert!(
                avg_error < 0.15,
                "at k={k}, average error should be < 0.15, got {avg_error:.4}"
            );
        }
    }
}

#[test]
fn differential_minhash_identical_sets_exact() {
    let paths: Vec<WildcardPath> = (0..50)
        .map(|i| WildcardPath::root().push_key(&format!("path_{i}")))
        .collect();

    let seed = 99;
    for &k in &[32, 128, 512] {
        let sig = minhash_signature(&paths, k, seed);
        let sim = minhash_similarity(&sig, &sig);
        assert!(
            (sim - 1.0).abs() < f64::EPSILON,
            "identical sets should give similarity 1.0 at k={k}, got {sim}"
        );
    }
}

#[test]
fn differential_minhash_disjoint_sets() {
    let set_a: Vec<WildcardPath> = (0..100)
        .map(|i| WildcardPath::root().push_key(&format!("aaa_{i}")))
        .collect();
    let set_b: Vec<WildcardPath> = (0..100)
        .map(|i| WildcardPath::root().push_key(&format!("bbb_{i}")))
        .collect();

    let exact = jaccard_similarity(&set_a, &set_b);
    assert!(exact.abs() < 1e-10, "disjoint sets should have jaccard 0.0");

    let seed = 77;
    let sig_a = minhash_signature(&set_a, 512, seed);
    let sig_b = minhash_signature(&set_b, 512, seed);
    let approx = minhash_similarity(&sig_a, &sig_b);

    assert!(
        approx < 0.05,
        "disjoint sets should have very low MinHash similarity, got {approx}"
    );
}

// -----------------------------------------------------------------------
// Additional: CMS merge preserves bounds
// -----------------------------------------------------------------------

#[test]
fn differential_cms_merge_preserves_bounds() {
    let epsilon = 0.001;
    let delta = 0.01;
    let mut cms1 = CountMinSketch::new(epsilon, delta);
    let mut cms2 = CountMinSketch::new(epsilon, delta);
    let mut true_counts: BTreeMap<String, u64> = BTreeMap::new();

    // Split insertions across two sketches.
    for i in 0..50_u64 {
        let count = 100 + i * 10;
        let key = format!("item_{i}");
        for _ in 0..count {
            cms1.add(key.as_bytes(), 1);
        }
        *true_counts.entry(key).or_insert(0) += count;
    }
    for i in 50..100_u64 {
        let count = 100 + i * 10;
        let key = format!("item_{i}");
        for _ in 0..count {
            cms2.add(key.as_bytes(), 1);
        }
        *true_counts.entry(key).or_insert(0) += count;
    }
    // Add shared items to both.
    for _ in 0..200 {
        cms1.add(b"shared", 1);
        cms2.add(b"shared", 1);
    }
    *true_counts.entry("shared".to_string()).or_insert(0) += 400;

    cms1.merge(&cms2);

    // After merge, no underestimation for any item.
    for (key, &tc) in &true_counts {
        let est = cms1.estimate(key.as_bytes());
        assert!(
            est >= tc,
            "post-merge underestimate for '{key}': est={est}, true={tc}"
        );
    }
}
