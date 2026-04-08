//! Rarity scoring via self-information.
//!
//! Computes rarity as `-log2(count / total)` (self-information in bits).
//! Values with high rarity scores are rare and potentially interesting.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A value flagged as rare based on its self-information score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RarityOutlier {
    /// The rare value.
    pub value: String,
    /// How many times the value appeared.
    pub count: u64,
    /// Rarity score in bits (self-information).
    pub rarity_bits: f64,
}

/// Compute the rarity (self-information) of a value in bits.
///
/// Returns `-log2(value_count / total_count)`.
///
/// # Edge cases
///
/// - `value_count == 0` or `total_count == 0`: returns 0.0 (undefined, but safe default).
/// - `value_count > total_count`: returns 0.0 (impossible data, safe default).
#[must_use]
pub fn rarity_score(value_count: u64, total_count: u64) -> f64 {
    if value_count == 0 || total_count == 0 || value_count > total_count {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let p = value_count as f64 / total_count as f64;
    -p.log2()
}

/// Detect rare values whose rarity exceeds the mean plus a MAD-based threshold.
///
/// # Algorithm
///
/// 1. Compute rarity score for each value.
/// 2. Compute mean rarity and MAD of rarity scores.
/// 3. Flag values where rarity exceeds the mean plus the sigma multiplier times the MAD.
///
/// # Edge cases
///
/// - Empty counts: returns empty vec.
/// - Single value: returns empty vec (cannot be rare relative to itself).
/// - All values have the same count: returns empty vec (MAD = 0, all equally rare).
#[must_use]
pub fn detect_rare_values(
    counts: &BTreeMap<String, u64>,
    total: u64,
    threshold_sigma: f64,
) -> Vec<RarityOutlier> {
    if counts.is_empty() || total == 0 {
        return Vec::new();
    }

    // Compute rarity for each value
    let scored: Vec<(&String, u64, f64)> = counts
        .iter()
        .map(|(val, &cnt)| (val, cnt, rarity_score(cnt, total)))
        .collect();

    if scored.len() <= 1 {
        return Vec::new();
    }

    // Compute mean rarity
    let rarities: Vec<f64> = scored.iter().map(|&(_, _, r)| r).collect();
    #[allow(clippy::cast_precision_loss)]
    let mean_rarity = rarities.iter().sum::<f64>() / rarities.len() as f64;

    // Compute MAD of rarity scores
    let mut abs_devs: Vec<f64> = rarities.iter().map(|r| (r - mean_rarity).abs()).collect();
    abs_devs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = median_of_sorted(&abs_devs);

    // If MAD is 0, all rarity scores are identical: no outliers
    if mad < f64::EPSILON {
        return Vec::new();
    }

    let cutoff = threshold_sigma.mul_add(mad, mean_rarity);

    scored
        .into_iter()
        .filter(|&(_, _, r)| r > cutoff)
        .map(|(val, cnt, r)| RarityOutlier {
            value: val.clone(),
            count: cnt,
            rarity_bits: r,
        })
        .collect()
}

/// Compute median of a sorted slice.
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let len = sorted.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 1 {
        sorted[len / 2]
    } else {
        f64::midpoint(sorted[len / 2 - 1], sorted[len / 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 0.1;
    const EPS_TIGHT: f64 = 1e-6;

    #[test]
    fn rarity_one_in_ten_thousand() {
        // -log2(1/10000) = log2(10000) ~ 13.2877
        let r = rarity_score(1, 10_000);
        assert!((r - 13.2877).abs() < EPS, "expected ~13.3 bits, got {r}");
    }

    #[test]
    fn rarity_uniform_distribution() {
        // 10 values each appearing 100 times out of 1000
        // Each has rarity = -log2(100/1000) = -log2(0.1) = log2(10) ~ 3.3219
        let expected = 10.0_f64.log2();
        for count in [100_u64; 10] {
            let r = rarity_score(count, 1000);
            assert!(
                (r - expected).abs() < EPS_TIGHT,
                "expected {expected}, got {r}"
            );
        }
    }

    #[test]
    fn rarity_zero_count() {
        assert!((rarity_score(0, 100) - 0.0).abs() < EPS_TIGHT);
    }

    #[test]
    fn rarity_zero_total() {
        assert!((rarity_score(5, 0) - 0.0).abs() < EPS_TIGHT);
    }

    #[test]
    fn rarity_count_exceeds_total() {
        assert!((rarity_score(200, 100) - 0.0).abs() < EPS_TIGHT);
    }

    #[test]
    fn detect_rare_empty() {
        let counts = BTreeMap::new();
        let result = detect_rare_values(&counts, 0, 2.0);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_rare_single_value() {
        let mut counts = BTreeMap::new();
        counts.insert("a".to_owned(), 100);
        let result = detect_rare_values(&counts, 100, 2.0);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_rare_uniform_no_outliers() {
        // All values have the same count => same rarity => MAD=0 => no outliers
        let mut counts = BTreeMap::new();
        for i in 0..10 {
            counts.insert(format!("val_{i}"), 100);
        }
        let result = detect_rare_values(&counts, 1000, 2.0);
        assert!(
            result.is_empty(),
            "uniform distribution should have no rare outliers"
        );
    }

    #[test]
    fn detect_rare_finds_singleton() {
        // 9 values each appearing 1000 times, 1 value appearing once
        let mut counts = BTreeMap::new();
        for i in 0..9 {
            counts.insert(format!("common_{i}"), 1000);
        }
        counts.insert("rare".to_owned(), 1);
        let total: u64 = counts.values().sum();

        let result = detect_rare_values(&counts, total, 2.0);
        assert!(!result.is_empty(), "should detect the rare value");

        let has_rare = result.iter().any(|o| o.value == "rare");
        assert!(has_rare, "the value 'rare' should be in the outliers");
    }
}
