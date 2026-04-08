//! MAD-based numeric outlier detection.
//!
//! Uses the Median Absolute Deviation (MAD) to compute modified z-scores
//! for numeric values, flagging those that exceed a configurable threshold.

use serde::{Deserialize, Serialize};

/// A numeric outlier detected via MAD-based z-score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericOutlier {
    /// The path where the outlier was found.
    pub path: String,
    /// The outlier value.
    pub value: f64,
    /// The modified z-score (MAD-based).
    pub z_mad: f64,
    /// The median of the dataset.
    pub median: f64,
    /// The MAD of the dataset.
    pub mad: f64,
}

/// A scored numeric outlier (without path context).
///
/// Produced by [`detect_numeric_outliers`] before path assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericOutlierScore {
    /// Index of the outlier in the original (sorted) input.
    pub index: usize,
    /// The outlier value.
    pub value: f64,
    /// The modified z-score (MAD-based).
    pub z_mad: f64,
    /// The median of the dataset.
    pub median: f64,
    /// The MAD of the dataset.
    pub mad: f64,
}

/// Compute the median of a sorted slice of `f64` values.
///
/// Returns `None` if the slice is empty.
fn median_sorted(sorted: &[f64]) -> Option<f64> {
    let len = sorted.len();
    if len == 0 {
        return None;
    }
    if len % 2 == 1 {
        Some(sorted[len / 2])
    } else {
        Some(f64::midpoint(sorted[len / 2 - 1], sorted[len / 2]))
    }
}

/// Detect numeric outliers using MAD-based modified z-scores.
///
/// The input `values` slice is sorted in place to compute the median and MAD.
/// Values whose absolute modified z-score exceeds `threshold` are returned
/// as scored outliers.
///
/// # Algorithm
///
/// 1. Sort values, compute median.
/// 2. Compute MAD = median of absolute deviations from the median.
/// 3. Modified z-score = 0.6745 * (value - median) / MAD.
/// 4. Flag values where the absolute modified z-score exceeds the threshold.
///
/// # Edge cases
///
/// - Empty input: returns empty vec.
/// - Single value: returns empty vec (no meaningful outlier in isolation).
/// - MAD = 0 (all identical): any value differing from the median is flagged
///   with `z_mad = f64::INFINITY` (or `-INFINITY`).
#[must_use]
pub fn detect_numeric_outliers(values: &mut [f64], threshold: f64) -> Vec<NumericOutlierScore> {
    /// The consistency constant for normal distributions.
    const CONSISTENCY_K: f64 = 0.6745;

    if values.len() <= 1 {
        return Vec::new();
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let Some(median) = median_sorted(values) else {
        return Vec::new();
    };

    // Compute absolute deviations from median
    let mut abs_devs: Vec<f64> = values.iter().map(|v| (v - median).abs()).collect();
    abs_devs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let Some(mad) = median_sorted(&abs_devs) else {
        return Vec::new();
    };

    let mut outliers = Vec::new();

    if mad == 0.0 {
        // All values are identical (or nearly so). Any value that differs
        // from the median is an outlier with infinite z-score.
        for (i, &val) in values.iter().enumerate() {
            if (val - median).abs() > f64::EPSILON {
                let z = if val > median {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                };
                outliers.push(NumericOutlierScore {
                    index: i,
                    value: val,
                    z_mad: z,
                    median,
                    mad,
                });
            }
        }
    } else {
        for (i, &val) in values.iter().enumerate() {
            let z = CONSISTENCY_K * (val - median) / mad;
            if z.abs() > threshold {
                outliers.push(NumericOutlierScore {
                    index: i,
                    value: val,
                    z_mad: z,
                    median,
                    mad,
                });
            }
        }
    }

    outliers
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    #[test]
    fn empty_input_returns_empty() {
        let mut values: Vec<f64> = vec![];
        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(result.is_empty());
    }

    #[test]
    fn single_value_returns_empty() {
        let mut values = vec![42.0];
        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(result.is_empty());
    }

    #[test]
    fn all_identical_no_outliers() {
        let mut values = vec![5.0; 100];
        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(
            result.is_empty(),
            "all identical values should produce no outliers"
        );
    }

    #[test]
    fn all_identical_with_one_different() {
        let mut values = vec![5.0; 99];
        values.push(100.0);
        let result = detect_numeric_outliers(&mut values, 3.5);
        assert_eq!(result.len(), 1);
        assert!((result[0].value - 100.0).abs() < EPS);
        assert!(result[0].z_mad.is_infinite());
    }

    #[test]
    fn known_dataset_planted_outlier() {
        // Normal-ish data with a planted outlier at 1000.0
        let mut values: Vec<f64> = (1..=20).map(f64::from).collect();
        values.push(1000.0);

        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(!result.is_empty(), "should detect the planted outlier");

        // The outlier value 1000.0 must be among the results
        let has_outlier = result.iter().any(|o| (o.value - 1000.0).abs() < EPS);
        assert!(has_outlier, "1000.0 should be flagged as outlier");
    }

    #[test]
    fn median_computation_odd() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((median_sorted(&sorted).unwrap_or(0.0) - 3.0).abs() < EPS);
    }

    #[test]
    fn median_computation_even() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0];
        assert!((median_sorted(&sorted).unwrap_or(0.0) - 2.5).abs() < EPS);
    }

    #[test]
    fn median_empty() {
        let sorted: Vec<f64> = vec![];
        assert!(median_sorted(&sorted).is_none());
    }

    #[test]
    fn two_values_no_outlier() {
        // Two similar values should not be outliers
        let mut values = vec![10.0, 11.0];
        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(result.is_empty());
    }

    #[test]
    fn symmetric_outliers() {
        // Symmetric dataset: values around 50, with outliers at both extremes
        let mut values: Vec<f64> = (40..=60).map(f64::from).collect();
        values.push(-100.0);
        values.push(200.0);

        let result = detect_numeric_outliers(&mut values, 3.5);
        assert!(result.len() >= 2, "should detect both outliers");

        let has_low = result.iter().any(|o| (o.value - (-100.0)).abs() < EPS);
        let has_high = result.iter().any(|o| (o.value - 200.0).abs() < EPS);
        assert!(has_low, "-100.0 should be flagged");
        assert!(has_high, "200.0 should be flagged");
    }
}
