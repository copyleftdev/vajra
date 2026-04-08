//! One-dimensional Wasserstein (Earth Mover's) distance.
//!
//! Computes the 1-Wasserstein distance between two empirical 1D
//! distributions by sorting samples and integrating the absolute
//! difference of their CDFs.

/// Compute the 1-Wasserstein distance (Earth Mover's distance) between
/// two one-dimensional empirical distributions.
///
/// Both slices are sorted in place. The distance is computed by
/// evaluating the integral of the absolute difference of the two
/// empirical CDFs over the combined set of sample points.
///
/// # Complexity
///
/// O(n log n + m log m) for sorting, where n = `a.len()`, m = `b.len()`.
///
/// # Edge cases
///
/// - If either input is empty, returns 0.0.
/// - If both inputs are identical, returns 0.0.
/// - Single-element inputs are handled correctly.
#[must_use]
pub fn wasserstein_1d(a: &mut [f64], b: &mut [f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    b.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    let n_a = a.len() as f64;
    let n_b = b.len() as f64;

    // Merge the two sorted samples and integrate |CDF_a - CDF_b|.
    // Each sample point from `a` increments CDF_a by 1/n_a,
    // and each from `b` increments CDF_b by 1/n_b.
    let mut i = 0_usize;
    let mut j = 0_usize;
    let mut cdf_a = 0.0_f64;
    let mut cdf_b = 0.0_f64;
    let mut integral = 0.0_f64;
    let mut prev_x = f64::NEG_INFINITY;

    while i < a.len() || j < b.len() {
        let x = match (a.get(i), b.get(j)) {
            (Some(&ai), Some(&bj)) => {
                if ai <= bj {
                    ai
                } else {
                    bj
                }
            }
            (Some(&ai), None) => ai,
            (None, Some(&bj)) => bj,
            (None, None) => break,
        };

        // Add the area of the rectangle from prev_x to x with height |cdf_a - cdf_b|.
        if prev_x.is_finite() {
            integral += (x - prev_x) * (cdf_a - cdf_b).abs();
        }

        // Advance all points at position x.
        while i < a.len() && a[i] <= x {
            cdf_a += 1.0 / n_a;
            i += 1;
        }
        while j < b.len() && b[j] <= x {
            cdf_b += 1.0 / n_b;
            j += 1;
        }

        prev_x = x;
    }

    integral
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_distributions_zero() {
        let mut a = vec![1.0, 2.0, 3.0];
        let mut b = vec![1.0, 2.0, 3.0];
        let d = wasserstein_1d(&mut a, &mut b);
        assert!((d - 0.0).abs() < 1e-12, "identical distributions should give 0, got {d}");
    }

    #[test]
    fn shifted_by_constant() {
        // [1, 2, 3] vs [4, 5, 6]: each point shifted by 3.
        // Wasserstein distance = 3.0.
        let mut a = vec![1.0, 2.0, 3.0];
        let mut b = vec![4.0, 5.0, 6.0];
        let d = wasserstein_1d(&mut a, &mut b);
        assert!(
            (d - 3.0).abs() < 1e-12,
            "shift by 3 should give distance 3.0, got {d}"
        );
    }

    #[test]
    fn single_values() {
        let mut a = vec![0.0];
        let mut b = vec![5.0];
        let d = wasserstein_1d(&mut a, &mut b);
        assert!((d - 5.0).abs() < 1e-12, "distance between 0 and 5 should be 5.0, got {d}");
    }

    #[test]
    fn empty_input_returns_zero() {
        let mut a: Vec<f64> = vec![];
        let mut b = vec![1.0, 2.0];
        assert!((wasserstein_1d(&mut a, &mut b) - 0.0).abs() < f64::EPSILON);

        let mut c = vec![1.0];
        let mut d: Vec<f64> = vec![];
        assert!((wasserstein_1d(&mut c, &mut d) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn both_empty_returns_zero() {
        let mut a: Vec<f64> = vec![];
        let mut b: Vec<f64> = vec![];
        assert!((wasserstein_1d(&mut a, &mut b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn different_sizes() {
        // a = [0], b = [0, 1]
        // CDF_a jumps to 1.0 at x=0; CDF_b jumps to 0.5 at x=0, to 1.0 at x=1.
        // Integral of |CDF_a - CDF_b|:
        //   from x=0 to x=1: |1.0 - 0.5| * 1 = 0.5
        let mut a = vec![0.0];
        let mut b = vec![0.0, 1.0];
        let d = wasserstein_1d(&mut a, &mut b);
        assert!(
            (d - 0.5).abs() < 1e-12,
            "expected 0.5, got {d}"
        );
    }

    #[test]
    fn unsorted_input_handled() {
        let mut a = vec![3.0, 1.0, 2.0];
        let mut b = vec![6.0, 4.0, 5.0];
        let d = wasserstein_1d(&mut a, &mut b);
        assert!(
            (d - 3.0).abs() < 1e-12,
            "should handle unsorted input, got {d}"
        );
    }
}
