//! Median Absolute Deviation (MAD) and modified z-score computation.
//!
//! MAD is a robust measure of statistical dispersion, resistant to outliers.
//! The modified z-score uses MAD instead of standard deviation to identify
//! potential anomalies.

/// Compute the median of a **pre-sorted** slice.
///
/// Returns 0.0 for an empty slice.
#[must_use]
pub fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        f64::midpoint(sorted[n / 2 - 1], sorted[n / 2])
    }
}

/// Compute the Median Absolute Deviation of a mutable slice of values.
///
/// This sorts the input in place, computes the median, then computes the
/// median of the absolute deviations from the median.
///
/// Returns 0.0 for an empty slice or when all values are identical.
#[must_use]
pub fn mad(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = median(values);
    let mut deviations: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median(&deviations)
}

/// Compute the modified z-score for a single value, given the pre-computed
/// median and MAD.
///
/// The modified z-score is `0.6745 * (value - median) / MAD`.
///
/// If `mad_value` is 0.0 (all values are identical), returns 0.0 to avoid
/// division by zero.
#[must_use]
pub fn modified_z_score(value: f64, median_value: f64, mad_value: f64) -> f64 {
    if mad_value == 0.0 {
        return 0.0;
    }
    0.6745 * (value - median_value) / mad_value
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    // ---- median ----

    #[test]
    fn median_empty() {
        assert!((median(&[]) - 0.0).abs() < EPS);
    }

    #[test]
    fn median_single() {
        assert!((median(&[42.0]) - 42.0).abs() < EPS);
    }

    #[test]
    fn median_odd_count() {
        assert!((median(&[1.0, 2.0, 3.0]) - 2.0).abs() < EPS);
    }

    #[test]
    fn median_even_count() {
        assert!((median(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < EPS);
    }

    #[test]
    fn median_already_sorted() {
        assert!((median(&[10.0, 20.0, 30.0, 40.0, 50.0]) - 30.0).abs() < EPS);
    }

    // ---- MAD ----

    #[test]
    fn mad_empty() {
        assert!((mad(&mut []) - 0.0).abs() < EPS);
    }

    #[test]
    fn mad_single() {
        assert!((mad(&mut [5.0]) - 0.0).abs() < EPS);
    }

    #[test]
    fn mad_all_identical() {
        assert!((mad(&mut [7.0, 7.0, 7.0, 7.0]) - 0.0).abs() < EPS);
    }

    #[test]
    fn mad_known_dataset() {
        // Data: [1, 1, 2, 2, 4, 6, 9]
        // Sorted: [1, 1, 2, 2, 4, 6, 9]
        // Median = 2
        // Deviations: [1, 1, 0, 0, 2, 4, 7]
        // Sorted deviations: [0, 0, 1, 1, 2, 4, 7]
        // MAD = 1
        let mut data = vec![1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0];
        assert!((mad(&mut data) - 1.0).abs() < EPS);
    }

    #[test]
    fn mad_symmetric_dataset() {
        // Data: [1, 2, 3, 4, 5]
        // Median = 3
        // Deviations: [2, 1, 0, 1, 2]
        // Sorted deviations: [0, 1, 1, 2, 2]
        // MAD = 1
        let mut data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((mad(&mut data) - 1.0).abs() < EPS);
    }

    #[test]
    fn mad_even_count() {
        // Data: [2, 4, 6, 8]
        // Median = (4+6)/2 = 5
        // Deviations: [3, 1, 1, 3]
        // Sorted deviations: [1, 1, 3, 3]
        // MAD = (1+3)/2 = 2
        let mut data = vec![2.0, 4.0, 6.0, 8.0];
        assert!((mad(&mut data) - 2.0).abs() < EPS);
    }

    // ---- modified z-score ----

    #[test]
    fn z_score_zero_mad() {
        assert!((modified_z_score(5.0, 3.0, 0.0) - 0.0).abs() < EPS);
    }

    #[test]
    fn z_score_at_median() {
        assert!((modified_z_score(3.0, 3.0, 1.0) - 0.0).abs() < EPS);
    }

    #[test]
    fn z_score_positive() {
        // value=5, median=3, mad=1 => 0.6745*(5-3)/1 = 1.349
        let z = modified_z_score(5.0, 3.0, 1.0);
        assert!((z - 1.349).abs() < 1e-4);
    }

    #[test]
    fn z_score_negative() {
        // value=1, median=3, mad=1 => 0.6745*(1-3)/1 = -1.349
        let z = modified_z_score(1.0, 3.0, 1.0);
        assert!((z - (-1.349)).abs() < 1e-4);
    }

    #[test]
    fn z_score_known_values() {
        // value=9, median=2, mad=1 => 0.6745*7/1 = 4.7215
        let z = modified_z_score(9.0, 2.0, 1.0);
        assert!((z - 4.7215).abs() < 1e-4);
    }
}
