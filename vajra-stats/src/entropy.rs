//! Shannon entropy computation for value distributions.
//!
//! Entropy measures the unpredictability of a distribution. A path where
//! every value is identical has entropy 0; one where all values are unique
//! has maximum entropy log2(N).

/// Compute Shannon entropy from a slice of observation counts.
///
/// Each element in `counts` is the number of times a distinct value was
/// observed. The total observation count is the sum of `counts`.
///
/// Returns 0.0 for empty input or when only a single distinct value exists.
#[must_use]
pub fn shannon_entropy_from_counts(counts: &[u64]) -> f64 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let total_f = total as f64;
    let mut entropy = 0.0_f64;

    for &c in counts {
        if c == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = c as f64 / total_f;
        entropy -= p * p.log2();
    }

    // Guard against negative zero from floating-point arithmetic.
    if entropy < 0.0 {
        0.0
    } else {
        entropy
    }
}

/// Normalize an entropy value by the theoretical maximum for the given
/// support size: `entropy / log2(support_size)`.
///
/// Returns 0.0 when `support_size` is 0 or 1 (entropy is trivially 0 in
/// those cases, regardless of the raw value).
#[must_use]
pub fn normalized_entropy(entropy: f64, support_size: usize) -> f64 {
    if support_size <= 1 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let max_entropy = (support_size as f64).log2();
    if max_entropy <= 0.0 {
        return 0.0;
    }
    let normed = entropy / max_entropy;
    // Clamp to [0, 1] to handle tiny floating-point overshoots.
    normed.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    #[test]
    fn empty_input_returns_zero() {
        let counts: &[u64] = &[];
        assert!((shannon_entropy_from_counts(counts) - 0.0).abs() < EPS);
    }

    #[test]
    fn single_value_returns_zero() {
        assert!((shannon_entropy_from_counts(&[100]) - 0.0).abs() < EPS);
    }

    #[test]
    fn uniform_over_two_returns_one() {
        // H = log2(2) = 1.0
        let h = shannon_entropy_from_counts(&[50, 50]);
        assert!((h - 1.0).abs() < EPS);
    }

    #[test]
    fn uniform_over_four_returns_two() {
        // H = log2(4) = 2.0
        let h = shannon_entropy_from_counts(&[25, 25, 25, 25]);
        assert!((h - 2.0).abs() < EPS);
    }

    #[test]
    fn uniform_over_eight_returns_three() {
        let h = shannon_entropy_from_counts(&[10, 10, 10, 10, 10, 10, 10, 10]);
        assert!((h - 3.0).abs() < EPS);
    }

    #[test]
    fn known_distribution() {
        // p = [0.5, 0.25, 0.25]
        // H = -0.5*log2(0.5) - 0.25*log2(0.25) - 0.25*log2(0.25)
        //   = 0.5 + 0.5 + 0.5 = 1.5
        let h = shannon_entropy_from_counts(&[2, 1, 1]);
        assert!((h - 1.5).abs() < EPS);
    }

    #[test]
    fn zero_counts_ignored() {
        let h = shannon_entropy_from_counts(&[50, 0, 50]);
        assert!((h - 1.0).abs() < EPS);
    }

    #[test]
    fn all_zeros_returns_zero() {
        assert!((shannon_entropy_from_counts(&[0, 0, 0]) - 0.0).abs() < EPS);
    }

    // ---- normalized entropy ----

    #[test]
    fn normalized_constant_is_zero() {
        let h = shannon_entropy_from_counts(&[100]);
        let nh = normalized_entropy(h, 1);
        assert!((nh - 0.0).abs() < EPS);
    }

    #[test]
    fn normalized_uniform_is_one() {
        let h = shannon_entropy_from_counts(&[10, 10, 10, 10]);
        let nh = normalized_entropy(h, 4);
        assert!((nh - 1.0).abs() < EPS);
    }

    #[test]
    fn normalized_empty_support_is_zero() {
        let nh = normalized_entropy(1.0, 0);
        assert!((nh - 0.0).abs() < EPS);
    }

    #[test]
    fn normalized_support_one_is_zero() {
        let nh = normalized_entropy(0.0, 1);
        assert!((nh - 0.0).abs() < EPS);
    }

    #[test]
    fn normalized_partial() {
        // p = [0.5, 0.25, 0.25], H = 1.5, max = log2(3)
        let h = shannon_entropy_from_counts(&[2, 1, 1]);
        let nh = normalized_entropy(h, 3);
        let expected = 1.5 / (3.0_f64).log2();
        assert!((nh - expected).abs() < EPS);
    }
}
