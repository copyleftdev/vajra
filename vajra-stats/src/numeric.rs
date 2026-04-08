//! Numeric distribution summary statistics.
//!
//! Computes min, max, mean, median, MAD, and percentiles for a collection
//! of numeric values extracted from a JSON path.

use crate::mad;
use serde::{Deserialize, Serialize};

/// Summary statistics for a numeric distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub mad: f64,
    pub p05: f64,
    pub p25: f64,
    pub p75: f64,
    pub p95: f64,
    pub count: usize,
}

/// Compute percentile via linear interpolation on a **pre-sorted** slice.
///
/// `p` must be in `[0.0, 1.0]`. Returns 0.0 for an empty slice.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }

    // Use the "C = 1" interpolation method (numpy default):
    //   index = p * (n - 1)
    let idx = p * (n - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;

    if lo == hi || hi >= n {
        return sorted[lo.min(n - 1)];
    }

    let frac = idx - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

/// Compute summary statistics for a mutable `Vec` of numeric values.
///
/// The input is sorted in place. Returns `None` if the input is empty.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_numeric_stats(values: &mut [f64]) -> Option<NumericStats> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = values.len();
    let min = values[0];
    let max = values[count - 1];
    let sum: f64 = values.iter().sum();
    let mean = sum / count as f64;
    let med = mad::median(values);

    // Compute MAD on a copy so we don't re-sort `values`.
    let mut deviation_input = values.to_owned();
    let mad_val = mad::mad(&mut deviation_input);

    let p05 = percentile(values, 0.05);
    let p25 = percentile(values, 0.25);
    let p75 = percentile(values, 0.75);
    let p95 = percentile(values, 0.95);

    Some(NumericStats {
        min,
        max,
        mean,
        median: med,
        mad: mad_val,
        p05,
        p25,
        p75,
        p95,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    // ---- percentile ----

    #[test]
    fn percentile_empty() {
        assert!((percentile(&[], 0.5) - 0.0).abs() < EPS);
    }

    #[test]
    fn percentile_single() {
        assert!((percentile(&[42.0], 0.5) - 42.0).abs() < EPS);
        assert!((percentile(&[42.0], 0.0) - 42.0).abs() < EPS);
        assert!((percentile(&[42.0], 1.0) - 42.0).abs() < EPS);
    }

    #[test]
    fn percentile_min_max() {
        let data = &[1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(data, 0.0) - 1.0).abs() < EPS);
        assert!((percentile(data, 1.0) - 5.0).abs() < EPS);
    }

    #[test]
    fn percentile_median_odd() {
        let data = &[1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(data, 0.5) - 3.0).abs() < EPS);
    }

    #[test]
    fn percentile_median_even() {
        let data = &[1.0, 2.0, 3.0, 4.0];
        // p50 on 4 elements: idx = 0.5 * 3 = 1.5 -> lerp(2,3,0.5) = 2.5
        assert!((percentile(data, 0.5) - 2.5).abs() < EPS);
    }

    #[test]
    fn percentile_quartiles() {
        let data = &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        // p25: idx = 0.25 * 9 = 2.25 -> lerp(3.0, 4.0, 0.25) = 3.25
        assert!((percentile(data, 0.25) - 3.25).abs() < EPS);
        // p75: idx = 0.75 * 9 = 6.75 -> lerp(7.0, 8.0, 0.75) = 7.75
        assert!((percentile(data, 0.75) - 7.75).abs() < EPS);
    }

    // ---- compute_numeric_stats ----

    #[test]
    fn stats_empty() {
        assert!(compute_numeric_stats(&mut []).is_none());
    }

    #[test]
    fn stats_single_value() -> Result<(), Box<dyn std::error::Error>> {
        let s = compute_numeric_stats(&mut [42.0]).ok_or("expected Some")?;
        assert!((s.min - 42.0).abs() < EPS);
        assert!((s.max - 42.0).abs() < EPS);
        assert!((s.mean - 42.0).abs() < EPS);
        assert!((s.median - 42.0).abs() < EPS);
        assert!((s.mad - 0.0).abs() < EPS);
        assert_eq!(s.count, 1);
        Ok(())
    }

    #[test]
    fn stats_known_dataset() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let s = compute_numeric_stats(&mut data).ok_or("expected Some")?;

        assert!((s.min - 1.0).abs() < EPS);
        assert!((s.max - 5.0).abs() < EPS);
        assert!((s.mean - 3.0).abs() < EPS);
        assert!((s.median - 3.0).abs() < EPS);
        assert!((s.mad - 1.0).abs() < EPS);
        assert_eq!(s.count, 5);
        Ok(())
    }

    #[test]
    fn stats_all_identical() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![7.0, 7.0, 7.0, 7.0];
        let s = compute_numeric_stats(&mut data).ok_or("expected Some")?;

        assert!((s.min - 7.0).abs() < EPS);
        assert!((s.max - 7.0).abs() < EPS);
        assert!((s.mean - 7.0).abs() < EPS);
        assert!((s.median - 7.0).abs() < EPS);
        assert!((s.mad - 0.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn stats_unsorted_input() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![5.0, 1.0, 4.0, 2.0, 3.0];
        let s = compute_numeric_stats(&mut data).ok_or("expected Some")?;

        assert!((s.min - 1.0).abs() < EPS);
        assert!((s.max - 5.0).abs() < EPS);
        assert!((s.mean - 3.0).abs() < EPS);
        assert!((s.median - 3.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn stats_percentiles() -> Result<(), Box<dyn std::error::Error>> {
        // 10 values: 1..=10
        let mut data: Vec<f64> = (1..=10).map(|x| x as f64).collect();
        let s = compute_numeric_stats(&mut data).ok_or("expected Some")?;

        // p05: idx=0.05*9=0.45 -> lerp(1,2,0.45) = 1.45
        assert!((s.p05 - 1.45).abs() < EPS);
        // p25: idx=0.25*9=2.25 -> lerp(3,4,0.25) = 3.25
        assert!((s.p25 - 3.25).abs() < EPS);
        // p75: idx=0.75*9=6.75 -> lerp(7,8,0.75) = 7.75
        assert!((s.p75 - 7.75).abs() < EPS);
        // p95: idx=0.95*9=8.55 -> lerp(9,10,0.55) = 9.55
        assert!((s.p95 - 9.55).abs() < EPS);
        Ok(())
    }

    #[test]
    fn stats_two_values() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![10.0, 20.0];
        let s = compute_numeric_stats(&mut data).ok_or("expected Some")?;

        assert!((s.min - 10.0).abs() < EPS);
        assert!((s.max - 20.0).abs() < EPS);
        assert!((s.mean - 15.0).abs() < EPS);
        assert!((s.median - 15.0).abs() < EPS);
        Ok(())
    }
}
