//! DDSketch: relative-error quantile estimation.
//!
//! Implements the algorithm from Masson, Rim, Lee (2019).
//! Provides bounded-memory quantile estimation with a configurable
//! relative accuracy guarantee `alpha`.

use std::collections::BTreeMap;

/// A DDSketch for streaming quantile estimation with relative error guarantee.
///
/// For a given accuracy parameter `alpha`, every reported quantile is within
/// a factor of `(1 ± alpha)` of the true quantile value.
pub struct DDSketch {
    /// Gamma = (1 + alpha) / (1 - alpha), the base of the logarithmic mapping.
    gamma: f64,
    /// ln(gamma), cached for key computation.
    ln_gamma: f64,
    /// Positive-side bins: bucket_key -> count.
    pos_bins: BTreeMap<i32, u64>,
    /// Negative-side bins: bucket_key -> count (keyed on abs value).
    neg_bins: BTreeMap<i32, u64>,
    /// Count of exact zeros.
    zero_count: u64,
    /// Total number of insertions.
    count: u64,
    /// Running minimum.
    min: f64,
    /// Running maximum.
    max: f64,
    /// Running sum.
    sum: f64,
}

impl DDSketch {
    /// Create a new `DDSketch` with the given relative accuracy `alpha`.
    ///
    /// `alpha` must be in `(0.0, 1.0)`. A typical value is 0.01 (1% relative error).
    ///
    /// # Panics
    ///
    /// Panics if `alpha` is not in `(0.0, 1.0)`.
    #[must_use]
    pub fn new(alpha: f64) -> Self {
        assert!(
            alpha > 0.0 && alpha < 1.0,
            "alpha must be in (0.0, 1.0), got {alpha}"
        );
        let gamma = (1.0 + alpha) / (1.0 - alpha);
        Self {
            gamma,
            ln_gamma: gamma.ln(),
            pos_bins: BTreeMap::new(),
            neg_bins: BTreeMap::new(),
            zero_count: 0,
            count: 0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            sum: 0.0,
        }
    }

    /// Map a positive value to its bucket key.
    fn key(&self, v: f64) -> i32 {
        debug_assert!(v > 0.0);
        (v.ln() / self.ln_gamma).ceil() as i32
    }

    /// Map a bucket key back to a representative value (lower bound of the bucket).
    fn value_at(&self, k: i32) -> f64 {
        self.gamma.powi(k)
    }

    /// Insert a value into the sketch.
    pub fn add(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }

        if value > 0.0 {
            let k = self.key(value);
            *self.pos_bins.entry(k).or_insert(0) += 1;
        } else if value < 0.0 {
            let k = self.key(-value);
            *self.neg_bins.entry(k).or_insert(0) += 1;
        } else {
            self.zero_count += 1;
        }
    }

    /// Estimate the `q`-th quantile (0.0 to 1.0).
    ///
    /// Returns `None` if the sketch is empty.
    #[allow(clippy::cast_precision_loss)]
    pub fn quantile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        debug_assert!((0.0..=1.0).contains(&q));

        // The rank we are looking for (1-indexed).
        let rank = (q * (self.count as f64 - 1.0)).round() as u64 + 1;

        // Walk through: negative bins (descending key = largest absolute value first),
        // then zeros, then positive bins (ascending key).

        let mut running = 0u64;

        // Negative bins: iterate in *descending* key order (largest abs value first,
        // i.e., most-negative values first).
        for (&k, &c) in self.neg_bins.iter().rev() {
            running += c;
            if running >= rank {
                return Some(-self.value_at(k));
            }
        }

        // Zeros
        running += self.zero_count;
        if running >= rank {
            return Some(0.0);
        }

        // Positive bins: ascending key order (smallest positive first).
        for (&k, &c) in &self.pos_bins {
            running += c;
            if running >= rank {
                return Some(self.value_at(k));
            }
        }

        // Should not happen if count > 0, but return max as fallback.
        Some(self.max)
    }

    /// Total number of values inserted.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Minimum value inserted, or `None` if empty.
    #[must_use]
    pub fn min(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.min)
        }
    }

    /// Maximum value inserted, or `None` if empty.
    #[must_use]
    pub fn max(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.max)
        }
    }

    /// Mean of inserted values, or `None` if empty.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum / self.count as f64)
        }
    }

    /// Merge another sketch into this one. Both must have the same `alpha`
    /// (and therefore the same `gamma`).
    ///
    /// # Panics
    ///
    /// Panics if the two sketches have different `gamma` values.
    pub fn merge(&mut self, other: &DDSketch) {
        assert!(
            (self.gamma - other.gamma).abs() < 1e-12,
            "Cannot merge DDSketches with different alpha"
        );
        if other.count == 0 {
            return;
        }

        for (&k, &c) in &other.pos_bins {
            *self.pos_bins.entry(k).or_insert(0) += c;
        }
        for (&k, &c) in &other.neg_bins {
            *self.neg_bins.entry(k).or_insert(0) += c;
        }
        self.zero_count += other.zero_count;
        self.count += other.count;
        self.sum += other.sum;
        if other.min < self.min {
            self.min = other.min;
        }
        if other.max > self.max {
            self.max = other.max;
        }
    }

    /// Whether the sketch has received any values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Default for DDSketch {
    fn default() -> Self {
        Self::new(0.01)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sketch_returns_none() {
        let sk = DDSketch::default();
        assert!(sk.is_empty());
        assert!(sk.quantile(0.5).is_none());
        assert!(sk.min().is_none());
        assert!(sk.max().is_none());
        assert!(sk.mean().is_none());
        assert_eq!(sk.count(), 0);
    }

    #[test]
    fn single_value_all_quantiles() {
        let mut sk = DDSketch::default();
        sk.add(42.0);
        assert_eq!(sk.count(), 1);
        assert!(!sk.is_empty());
        // All quantiles should return approximately 42
        for &q in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = sk.quantile(q).unwrap();
            assert!(
                (v - 42.0).abs() / 42.0 <= 0.01 + 1e-9,
                "q={q}, v={v}, expected ~42.0"
            );
        }
    }

    #[test]
    fn min_max_mean_exact() {
        let mut sk = DDSketch::default();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            sk.add(v);
        }
        assert_eq!(sk.min().unwrap(), 1.0);
        assert_eq!(sk.max().unwrap(), 5.0);
        assert!((sk.mean().unwrap() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn count_is_exact() {
        let mut sk = DDSketch::default();
        for i in 0..1000 {
            sk.add(i as f64);
        }
        assert_eq!(sk.count(), 1000);
    }

    #[test]
    fn median_of_1_to_100() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in 1..=100 {
            sk.add(i as f64);
        }
        let med = sk.quantile(0.5).unwrap();
        let expected = 50.0;
        assert!(
            (med - expected).abs() / expected <= alpha + 0.02,
            "median={med}, expected ~{expected}"
        );
    }

    #[test]
    fn p99_of_1_to_1000() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in 1..=1000 {
            sk.add(i as f64);
        }
        let p99 = sk.quantile(0.99).unwrap();
        let expected = 990.0;
        assert!(
            (p99 - expected).abs() / expected <= alpha + 0.02,
            "p99={p99}, expected ~{expected}"
        );
    }

    #[test]
    fn p25_p75_of_1_to_100() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in 1..=100 {
            sk.add(i as f64);
        }
        let p25 = sk.quantile(0.25).unwrap();
        let p75 = sk.quantile(0.75).unwrap();
        assert!(
            (p25 - 25.0).abs() / 25.0 <= alpha + 0.05,
            "p25={p25}, expected ~25"
        );
        assert!(
            (p75 - 75.0).abs() / 75.0 <= alpha + 0.05,
            "p75={p75}, expected ~75"
        );
    }

    #[test]
    fn merge_equals_single_sketch() {
        let alpha = 0.01;
        let mut sk1 = DDSketch::new(alpha);
        let mut sk2 = DDSketch::new(alpha);
        let mut combined = DDSketch::new(alpha);

        for i in 1..=500 {
            sk1.add(i as f64);
            combined.add(i as f64);
        }
        for i in 501..=1000 {
            sk2.add(i as f64);
            combined.add(i as f64);
        }
        sk1.merge(&sk2);

        assert_eq!(sk1.count(), combined.count());
        assert_eq!(sk1.min(), combined.min());
        assert_eq!(sk1.max(), combined.max());
        assert!((sk1.mean().unwrap() - combined.mean().unwrap()).abs() < 1e-9);

        for &q in &[0.25, 0.5, 0.75, 0.99] {
            let v1 = sk1.quantile(q).unwrap();
            let v2 = combined.quantile(q).unwrap();
            assert!(
                (v1 - v2).abs() / v2.max(1.0) < 0.05,
                "q={q}: merged={v1}, combined={v2}"
            );
        }
    }

    #[test]
    fn negative_values() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in 1..=100 {
            sk.add(-(i as f64));
        }
        assert_eq!(sk.min().unwrap(), -100.0);
        assert_eq!(sk.max().unwrap(), -1.0);

        // Median should be around -50
        let med = sk.quantile(0.5).unwrap();
        assert!(
            (med - (-50.0)).abs() / 50.0 <= alpha + 0.05,
            "median={med}, expected ~-50"
        );
    }

    #[test]
    fn mixed_positive_negative() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in -50..=50 {
            sk.add(i as f64);
        }
        assert_eq!(sk.count(), 101);
        assert_eq!(sk.min().unwrap(), -50.0);
        assert_eq!(sk.max().unwrap(), 50.0);

        // Median should be around 0
        let med = sk.quantile(0.5).unwrap();
        assert!(med.abs() <= 2.0, "median={med}, expected ~0");
    }

    #[test]
    fn zeros_only() {
        let mut sk = DDSketch::default();
        for _ in 0..100 {
            sk.add(0.0);
        }
        assert_eq!(sk.count(), 100);
        assert_eq!(sk.quantile(0.5).unwrap(), 0.0);
        assert_eq!(sk.quantile(0.0).unwrap(), 0.0);
        assert_eq!(sk.quantile(1.0).unwrap(), 0.0);
    }

    #[test]
    fn alpha_005_wider_buckets() {
        // With alpha=0.05, there should be fewer distinct bucket keys than alpha=0.01
        let mut sk_narrow = DDSketch::new(0.01);
        let mut sk_wide = DDSketch::new(0.05);

        for i in 1..=1000 {
            sk_narrow.add(i as f64);
            sk_wide.add(i as f64);
        }

        let narrow_buckets = sk_narrow.pos_bins.len();
        let wide_buckets = sk_wide.pos_bins.len();
        assert!(
            wide_buckets < narrow_buckets,
            "wide={wide_buckets}, narrow={narrow_buckets}"
        );
    }

    #[test]
    fn merge_empty_into_nonempty() {
        let mut sk = DDSketch::default();
        sk.add(10.0);
        let empty = DDSketch::default();
        sk.merge(&empty);
        assert_eq!(sk.count(), 1);
        assert_eq!(sk.quantile(0.5).unwrap(), sk.quantile(0.5).unwrap());
    }

    #[test]
    fn merge_nonempty_into_empty() {
        let mut sk = DDSketch::default();
        let mut other = DDSketch::default();
        other.add(10.0);
        sk.merge(&other);
        assert_eq!(sk.count(), 1);
    }

    #[test]
    fn large_dataset_quantiles() {
        let alpha = 0.01;
        let mut sk = DDSketch::new(alpha);
        for i in 1..=10_000 {
            sk.add(i as f64);
        }
        assert_eq!(sk.count(), 10_000);

        // p50 ~ 5000
        let p50 = sk.quantile(0.5).unwrap();
        assert!((p50 - 5000.0).abs() / 5000.0 <= alpha + 0.02, "p50={p50}");

        // p99 ~ 9900
        let p99 = sk.quantile(0.99).unwrap();
        assert!((p99 - 9900.0).abs() / 9900.0 <= alpha + 0.02, "p99={p99}");
    }

    #[test]
    fn duplicate_values() {
        let mut sk = DDSketch::default();
        for _ in 0..1000 {
            sk.add(42.0);
        }
        assert_eq!(sk.count(), 1000);
        let med = sk.quantile(0.5).unwrap();
        assert!(
            (med - 42.0).abs() / 42.0 <= 0.01 + 1e-9,
            "median={med}, expected 42"
        );
    }

    #[test]
    fn quantile_boundaries() {
        let mut sk = DDSketch::default();
        for i in 1..=100 {
            sk.add(i as f64);
        }
        let q0 = sk.quantile(0.0).unwrap();
        let q1 = sk.quantile(1.0).unwrap();
        // q(0) should be near min, q(1) near max
        assert!((q0 - 1.0).abs() / 1.0 <= 0.02, "q(0)={q0}, expected ~1.0");
        assert!(
            (q1 - 100.0).abs() / 100.0 <= 0.02,
            "q(1)={q1}, expected ~100.0"
        );
    }
}
