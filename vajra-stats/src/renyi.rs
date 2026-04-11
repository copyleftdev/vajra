//! Rényi entropy spectrum: α-parameterized diversity profiling.
//!
//! Shannon entropy is one point (α=1) on a continuous family.
//! Different α values answer different questions:
//!
//! - **H₀ (Hartley):** `log₂(|support|)` — how many distinct values exist
//! - **H₁ (Shannon):** average surprise (limit as α→1)
//! - **H₂ (Collision):** `-log₂(Σ p²)` — probability two random draws match
//! - **H∞ (Min-entropy):** `-log₂(max p)` — worst-case unpredictability
//!
//! Reference: Rényi, A. (1961). "On measures of entropy and information."

use serde::{Deserialize, Serialize};

/// The four canonical Rényi entropy orders plus their divergence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenyiSpectrum {
    /// H₀ (Hartley entropy): log₂ of the support size.
    pub hartley: f64,
    /// H₁ (Shannon entropy): -Σ p log₂(p).
    pub shannon: f64,
    /// H₂ (Collision entropy): -log₂(Σ p²).
    pub collision: f64,
    /// H∞ (Min-entropy): -log₂(max p).
    pub min_entropy: f64,
    /// Spectrum divergence: H₀ - H∞ (spread of the spectrum).
    pub divergence: f64,
}

impl Default for RenyiSpectrum {
    fn default() -> Self {
        Self {
            hartley: 0.0,
            shannon: 0.0,
            collision: 0.0,
            min_entropy: 0.0,
            divergence: 0.0,
        }
    }
}

/// Compute the Rényi entropy of order α from observation counts.
///
/// Formula: `H_α(X) = (1 / (1-α)) · log₂(Σ p(x)^α)`
///
/// Special cases:
/// - α = 0 → Hartley entropy: `log₂(|support|)`
/// - α = 1 → Shannon entropy (via limit): `-Σ p log₂(p)`
/// - α = ∞ → Min-entropy: `-log₂(max p)`
///
/// Returns 0.0 for empty input or a single value.
#[must_use]
pub fn renyi_entropy(counts: &[u64], alpha: f64) -> f64 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 0.0;
    }

    // Filter out zero counts
    let nonzero: Vec<u64> = counts.iter().copied().filter(|&c| c > 0).collect();
    if nonzero.len() <= 1 {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let total_f = total as f64;

    // α = 0: Hartley entropy
    if alpha.abs() < f64::EPSILON {
        #[allow(clippy::cast_precision_loss)]
        return (nonzero.len() as f64).log2();
    }

    // α ≈ 1: Shannon entropy (use direct formula to avoid division by zero)
    if (alpha - 1.0).abs() < 1e-12 {
        let mut h = 0.0_f64;
        for &c in &nonzero {
            #[allow(clippy::cast_precision_loss)]
            let p = c as f64 / total_f;
            h -= p * p.log2();
        }
        return if h < 0.0 { 0.0 } else { h };
    }

    // α → ∞: Min-entropy
    if alpha.is_infinite() && alpha.is_sign_positive() {
        let max_count = nonzero.iter().copied().max().unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let max_p = max_count as f64 / total_f;
        return -max_p.log2();
    }

    // General case: H_α = (1 / (1-α)) · log₂(Σ p^α)
    let mut sum_p_alpha = 0.0_f64;
    for &c in &nonzero {
        #[allow(clippy::cast_precision_loss)]
        let p = c as f64 / total_f;
        sum_p_alpha += p.powf(alpha);
    }

    let h = sum_p_alpha.log2() / (1.0 - alpha);

    // Guard against negative zero from floating-point arithmetic
    if h < 0.0 {
        0.0
    } else {
        h
    }
}

/// Compute the full Rényi spectrum (H₀, H₁, H₂, H∞) from observation counts.
///
/// The spectrum is guaranteed to be monotonically decreasing:
/// H₀ ≥ H₁ ≥ H₂ ≥ H∞.
///
/// Returns a zeroed spectrum for empty input.
#[must_use]
pub fn renyi_spectrum(counts: &[u64]) -> RenyiSpectrum {
    let h0 = renyi_entropy(counts, 0.0);
    let h1 = renyi_entropy(counts, 1.0);
    let h2 = renyi_entropy(counts, 2.0);
    let h_inf = renyi_entropy(counts, f64::INFINITY);

    RenyiSpectrum {
        hartley: h0,
        shannon: h1,
        collision: h2,
        min_entropy: h_inf,
        divergence: h0 - h_inf,
    }
}

/// Compute the normalized Rényi spectrum, scaling each order to [0, 1].
///
/// Normalization uses the Hartley entropy (H₀ = log₂(support)) as the
/// maximum possible value, since H₀ ≥ H_α for all α ≥ 0.
#[must_use]
pub fn normalized_renyi_spectrum(counts: &[u64]) -> RenyiSpectrum {
    let raw = renyi_spectrum(counts);

    if raw.hartley <= 0.0 {
        return RenyiSpectrum::default();
    }

    let max_h = raw.hartley;
    RenyiSpectrum {
        hartley: 1.0, // H₀/H₀ is always 1
        shannon: (raw.shannon / max_h).clamp(0.0, 1.0),
        collision: (raw.collision / max_h).clamp(0.0, 1.0),
        min_entropy: (raw.min_entropy / max_h).clamp(0.0, 1.0),
        divergence: raw.divergence / max_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    // ---- renyi_entropy ----

    #[test]
    fn empty_returns_zero() {
        assert!((renyi_entropy(&[], 0.0)).abs() < EPS);
        assert!((renyi_entropy(&[], 1.0)).abs() < EPS);
        assert!((renyi_entropy(&[], 2.0)).abs() < EPS);
        assert!((renyi_entropy(&[], f64::INFINITY)).abs() < EPS);
    }

    #[test]
    fn single_value_returns_zero() {
        assert!((renyi_entropy(&[100], 0.0)).abs() < EPS);
        assert!((renyi_entropy(&[100], 1.0)).abs() < EPS);
        assert!((renyi_entropy(&[100], 2.0)).abs() < EPS);
        assert!((renyi_entropy(&[100], f64::INFINITY)).abs() < EPS);
    }

    #[test]
    fn hartley_is_log2_support() {
        // 4 distinct values → H₀ = log₂(4) = 2.0
        let h0 = renyi_entropy(&[10, 20, 30, 40], 0.0);
        assert!((h0 - 2.0).abs() < EPS);
    }

    #[test]
    fn shannon_matches_existing() {
        // p = [0.5, 0.25, 0.25], H₁ = 1.5
        let h1 = renyi_entropy(&[2, 1, 1], 1.0);
        assert!((h1 - 1.5).abs() < EPS);
    }

    #[test]
    fn collision_entropy_known() {
        // p = [0.5, 0.5] → H₂ = -log₂(0.25 + 0.25) = -log₂(0.5) = 1.0
        let h2 = renyi_entropy(&[50, 50], 2.0);
        assert!((h2 - 1.0).abs() < EPS);
    }

    #[test]
    fn collision_entropy_skewed() {
        // p = [0.5, 0.25, 0.25] → Σp² = 0.25 + 0.0625 + 0.0625 = 0.375
        // H₂ = -log₂(0.375)
        let h2 = renyi_entropy(&[2, 1, 1], 2.0);
        let expected = -(0.375_f64).log2();
        assert!((h2 - expected).abs() < EPS);
    }

    #[test]
    fn min_entropy_known() {
        // p = [0.5, 0.3, 0.2] → H∞ = -log₂(0.5) = 1.0
        let h_inf = renyi_entropy(&[5, 3, 2], f64::INFINITY);
        assert!((h_inf - 1.0).abs() < EPS);
    }

    #[test]
    fn uniform_distribution_all_equal() {
        // Uniform over 8: all orders should equal log₂(8) = 3.0
        let counts = &[10, 10, 10, 10, 10, 10, 10, 10];
        let h0 = renyi_entropy(counts, 0.0);
        let h1 = renyi_entropy(counts, 1.0);
        let h2 = renyi_entropy(counts, 2.0);
        let h_inf = renyi_entropy(counts, f64::INFINITY);

        assert!((h0 - 3.0).abs() < EPS);
        assert!((h1 - 3.0).abs() < EPS);
        assert!((h2 - 3.0).abs() < EPS);
        assert!((h_inf - 3.0).abs() < EPS);
    }

    #[test]
    fn zero_counts_ignored() {
        let h1 = renyi_entropy(&[50, 0, 50], 1.0);
        assert!((h1 - 1.0).abs() < EPS);
    }

    // ---- spectrum ----

    #[test]
    fn spectrum_monotone() {
        // Skewed distribution: monotone H₀ ≥ H₁ ≥ H₂ ≥ H∞
        let s = renyi_spectrum(&[100, 50, 20, 10, 5, 3, 2, 1]);
        assert!(s.hartley >= s.shannon - EPS);
        assert!(s.shannon >= s.collision - EPS);
        assert!(s.collision >= s.min_entropy - EPS);
    }

    #[test]
    fn spectrum_divergence() {
        let s = renyi_spectrum(&[100, 50, 20, 10, 5, 3, 2, 1]);
        assert!((s.divergence - (s.hartley - s.min_entropy)).abs() < EPS);
    }

    #[test]
    fn spectrum_uniform_zero_divergence() {
        let s = renyi_spectrum(&[10, 10, 10, 10]);
        assert!(s.divergence.abs() < EPS);
    }

    #[test]
    fn spectrum_empty() {
        let s = renyi_spectrum(&[]);
        assert!(s.hartley.abs() < EPS);
        assert!(s.shannon.abs() < EPS);
        assert!(s.collision.abs() < EPS);
        assert!(s.min_entropy.abs() < EPS);
        assert!(s.divergence.abs() < EPS);
    }

    // ---- normalized spectrum ----

    #[test]
    fn normalized_uniform_all_one() {
        let ns = normalized_renyi_spectrum(&[10, 10, 10, 10]);
        assert!((ns.hartley - 1.0).abs() < EPS);
        assert!((ns.shannon - 1.0).abs() < EPS);
        assert!((ns.collision - 1.0).abs() < EPS);
        assert!((ns.min_entropy - 1.0).abs() < EPS);
    }

    #[test]
    fn normalized_empty_all_zero() {
        let ns = normalized_renyi_spectrum(&[]);
        assert!(ns.hartley.abs() < EPS);
        assert!(ns.shannon.abs() < EPS);
        assert!(ns.collision.abs() < EPS);
        assert!(ns.min_entropy.abs() < EPS);
    }

    #[test]
    fn normalized_bounded_01() {
        let ns = normalized_renyi_spectrum(&[100, 50, 20, 10, 5, 3, 2, 1]);
        assert!(ns.hartley >= -EPS && ns.hartley <= 1.0 + EPS);
        assert!(ns.shannon >= -EPS && ns.shannon <= 1.0 + EPS);
        assert!(ns.collision >= -EPS && ns.collision <= 1.0 + EPS);
        assert!(ns.min_entropy >= -EPS && ns.min_entropy <= 1.0 + EPS);
    }

    // ---- consistency with Shannon ----

    #[test]
    fn renyi_alpha1_matches_shannon() {
        use crate::entropy::shannon_entropy_from_counts;

        let counts = &[7, 3, 5, 2, 8, 1];
        let h_renyi = renyi_entropy(counts, 1.0);
        let h_shannon = shannon_entropy_from_counts(counts);
        assert!(
            (h_renyi - h_shannon).abs() < 1e-10,
            "Rényi H₁ ({h_renyi}) must match Shannon ({h_shannon})"
        );
    }
}
