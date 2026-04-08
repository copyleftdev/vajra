//! Benford's Law analysis for leading digit distributions.
//!
//! Benford's Law predicts the frequency of leading digits in many naturally
//! occurring datasets: P(d) = log10(1 + 1/d) for d in 1..=9.
//!
//! This module provides tools to compute expected and observed leading-digit
//! distributions, and to test conformity using chi-squared and mean absolute
//! deviation (Nigrini's conformity test).

use serde::{Deserialize, Serialize};

/// Result of a Benford's Law analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenfordResult {
    /// Observed leading-digit counts for digits 1..=9.
    pub distribution: [u64; 9],
    /// Chi-squared statistic comparing observed to expected.
    pub chi_squared: f64,
    /// Mean absolute deviation of observed proportions from expected
    /// (Nigrini's conformity test).
    pub mad_conformity: f64,
    /// Total number of values that contributed a leading digit.
    pub total: u64,
    /// Whether the distribution conforms to Benford's Law
    /// (MAD < 0.015, close conformity per Nigrini).
    pub conforms: bool,
}

/// Returns the expected Benford distribution P(d) for d = 1..=9.
///
/// Each element `i` (0-indexed) contains `log10(1 + 1/(i+1))`.
#[must_use]
pub fn expected_benford_distribution() -> [f64; 9] {
    let mut dist = [0.0_f64; 9];
    let mut i = 0;
    while i < 9 {
        #[allow(clippy::cast_precision_loss)]
        let d = (i + 1) as f64;
        dist[i] = (1.0 + 1.0 / d).log10();
        i += 1;
    }
    dist
}

/// Count leading digits for a slice of numeric values.
///
/// Returns an array of 9 counts, one per digit 1..=9.
/// - Zeros are skipped (they have no leading digit).
/// - Negative values use their absolute value.
/// - NaN and infinity values are skipped.
#[must_use]
pub fn leading_digit_distribution(values: &[f64]) -> [u64; 9] {
    let mut counts = [0_u64; 9];
    for &v in values {
        if let Some(d) = leading_digit(v) {
            counts[(d - 1) as usize] += 1;
        }
    }
    counts
}

/// Compute the chi-squared statistic comparing observed counts to Benford's
/// expected distribution.
///
/// Returns 0.0 if `total` is 0.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn benford_chi_squared(observed: &[u64; 9], total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let expected = expected_benford_distribution();
    let n = total as f64;
    let mut chi2 = 0.0;
    for i in 0..9 {
        let e = expected[i] * n;
        let o = observed[i] as f64;
        if e > 0.0 {
            chi2 += (o - e) * (o - e) / e;
        }
    }
    chi2
}

/// Compute the mean absolute deviation of observed proportions from Benford's
/// expected proportions (Nigrini's conformity test).
///
/// Returns 0.0 if `total` is 0.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn benford_mad_score(observed: &[u64; 9], total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let expected = expected_benford_distribution();
    let n = total as f64;
    let mut sum = 0.0;
    for i in 0..9 {
        let observed_prop = observed[i] as f64 / n;
        sum += (observed_prop - expected[i]).abs();
    }
    sum / 9.0
}

/// Perform a full Benford's Law analysis on a slice of numeric values.
///
/// Returns a [`BenfordResult`] with the distribution, chi-squared, MAD
/// conformity score, total count, and whether the data conforms
/// (MAD < 0.015 per Nigrini's close conformity threshold).
#[must_use]
pub fn analyze_benford(values: &[f64]) -> BenfordResult {
    let distribution = leading_digit_distribution(values);
    let total: u64 = distribution.iter().sum();
    let chi_squared = benford_chi_squared(&distribution, total);
    let mad_conformity = benford_mad_score(&distribution, total);
    let conforms = mad_conformity < 0.015;

    BenfordResult {
        distribution,
        chi_squared,
        mad_conformity,
        total,
        conforms,
    }
}

/// Extract the leading (most significant) digit from a float.
///
/// Returns `None` for zero, NaN, or infinity.
fn leading_digit(value: f64) -> Option<u8> {
    if !value.is_finite() || value == 0.0 {
        return None;
    }
    let abs = value.abs();
    // Use log10 to find the leading digit:
    //   leading_digit = floor(abs / 10^floor(log10(abs)))
    let exp = abs.log10().floor();
    let leading = (abs / 10.0_f64.powf(exp)).floor() as u8;
    // Clamp to 1..=9 (rounding edge cases)
    if leading >= 1 && leading <= 9 {
        Some(leading)
    } else if leading == 0 {
        // Edge case: floating point imprecision could give 0
        Some(1)
    } else {
        // leading > 9, which can happen due to floating point rounding
        Some(9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    // ---- expected_benford_distribution ----

    #[test]
    fn expected_distribution_sums_to_approximately_one() {
        let dist = expected_benford_distribution();
        let sum: f64 = dist.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "expected distribution sums to {sum}, expected ~1.0"
        );
    }

    #[test]
    fn expected_distribution_decreasing() {
        let dist = expected_benford_distribution();
        for i in 1..9 {
            assert!(
                dist[i] < dist[i - 1],
                "expected P({}) < P({}), but {:.6} >= {:.6}",
                i + 1,
                i,
                dist[i],
                dist[i - 1]
            );
        }
    }

    #[test]
    fn expected_distribution_known_values() {
        let dist = expected_benford_distribution();
        // P(1) = log10(2) ≈ 0.30103
        assert!((dist[0] - 0.30103).abs() < 1e-4);
        // P(9) = log10(10/9) ≈ 0.04576
        assert!((dist[8] - 0.04576).abs() < 1e-4);
    }

    // ---- leading_digit_distribution ----

    #[test]
    fn leading_digit_empty_input() {
        let counts = leading_digit_distribution(&[]);
        assert_eq!(counts, [0; 9]);
    }

    #[test]
    fn leading_digit_all_zeros() {
        let counts = leading_digit_distribution(&[0.0, 0.0, 0.0]);
        assert_eq!(counts, [0; 9]);
    }

    #[test]
    fn leading_digit_single_value() {
        let counts = leading_digit_distribution(&[42.0]);
        // Leading digit of 42 is 4
        assert_eq!(counts[3], 1);
        let sum: u64 = counts.iter().sum();
        assert_eq!(sum, 1);
    }

    #[test]
    fn leading_digit_negatives_use_absolute() {
        let counts_pos = leading_digit_distribution(&[7.5]);
        let counts_neg = leading_digit_distribution(&[-7.5]);
        assert_eq!(counts_pos, counts_neg);
    }

    #[test]
    fn leading_digit_various() {
        let values = [1.0, 2.0, 3.0, 10.0, 200.0, 3000.0, 0.005, 0.09];
        let counts = leading_digit_distribution(&values);
        // 1.0 -> 1, 2.0 -> 2, 3.0 -> 3, 10.0 -> 1, 200.0 -> 2, 3000.0 -> 3, 0.005 -> 5, 0.09 -> 9
        assert_eq!(counts[0], 2); // digit 1
        assert_eq!(counts[1], 2); // digit 2
        assert_eq!(counts[2], 2); // digit 3
        assert_eq!(counts[4], 1); // digit 5
        assert_eq!(counts[8], 1); // digit 9
    }

    // ---- benford_chi_squared ----

    #[test]
    fn chi_squared_zero_total() {
        let obs = [0; 9];
        assert!((benford_chi_squared(&obs, 0) - 0.0).abs() < EPS);
    }

    #[test]
    fn chi_squared_benford_conforming_data() {
        // Powers of 2 are known to approximately follow Benford's Law
        let powers: Vec<f64> = (0..1000).map(|i| 2.0_f64.powi(i)).collect();
        let counts = leading_digit_distribution(&powers);
        let total: u64 = counts.iter().sum();
        let chi2 = benford_chi_squared(&counts, total);
        // Should be relatively low for Benford-conforming data
        // Chi-squared critical value for 8 df at 0.05 is 15.507
        assert!(
            chi2 < 20.0,
            "powers of 2 should roughly conform to Benford; chi2={chi2}"
        );
    }

    #[test]
    fn chi_squared_uniform_distribution_high() {
        // All leading digits equally distributed - does NOT follow Benford
        let obs = [100, 100, 100, 100, 100, 100, 100, 100, 100];
        let total = 900;
        let chi2 = benford_chi_squared(&obs, total);
        // This should be substantially higher than for Benford-conforming data
        assert!(
            chi2 > 50.0,
            "uniform distribution should have high chi-squared; chi2={chi2}"
        );
    }

    // ---- benford_mad_score ----

    #[test]
    fn mad_score_zero_total() {
        let obs = [0; 9];
        assert!((benford_mad_score(&obs, 0) - 0.0).abs() < EPS);
    }

    #[test]
    fn mad_score_conforming_data() {
        // Powers of 2
        let powers: Vec<f64> = (0..1000).map(|i| 2.0_f64.powi(i)).collect();
        let counts = leading_digit_distribution(&powers);
        let total: u64 = counts.iter().sum();
        let mad = benford_mad_score(&counts, total);
        // Should be close conformity (MAD < 0.015 per Nigrini)
        assert!(
            mad < 0.015,
            "powers of 2 should have close conformity; MAD={mad}"
        );
    }

    // ---- analyze_benford (integration) ----

    #[test]
    fn analyze_empty() {
        let result = analyze_benford(&[]);
        assert_eq!(result.total, 0);
        assert_eq!(result.distribution, [0; 9]);
        assert!((result.chi_squared - 0.0).abs() < EPS);
        assert!((result.mad_conformity - 0.0).abs() < EPS);
        // With no data, MAD is 0 < 0.015, so conforms is true
        assert!(result.conforms);
    }

    #[test]
    fn analyze_benford_conforming() {
        // Fibonacci-like sequence: known to follow Benford's Law
        let mut fib = vec![1.0_f64, 1.0];
        for i in 2..500 {
            let next = fib[i - 1] + fib[i - 2];
            fib.push(next);
        }
        let result = analyze_benford(&fib);
        assert!(
            result.conforms,
            "Fibonacci sequence should conform to Benford; MAD={}",
            result.mad_conformity
        );
        assert_eq!(result.total, 500);
    }

    #[test]
    fn analyze_non_conforming() {
        // All same leading digit
        let data: Vec<f64> = (0..100).map(|i| 500.0 + f64::from(i)).collect();
        let result = analyze_benford(&data);
        // With all leading digits being 5, this should not conform
        assert!(
            !result.conforms,
            "all-fives leading digit should not conform; MAD={}",
            result.mad_conformity
        );
    }

    #[test]
    fn analyze_single_value() {
        let result = analyze_benford(&[42.0]);
        assert_eq!(result.total, 1);
        assert_eq!(result.distribution[3], 1); // digit 4
    }

    #[test]
    fn analyze_with_zeros_and_negatives() {
        let result = analyze_benford(&[0.0, -3.5, 0.0, 7.2, -1.0]);
        // 0.0 skipped twice, -3.5 -> 3, 7.2 -> 7, -1.0 -> 1
        assert_eq!(result.total, 3);
        assert_eq!(result.distribution[0], 1); // digit 1
        assert_eq!(result.distribution[2], 1); // digit 3
        assert_eq!(result.distribution[6], 1); // digit 7
    }
}
