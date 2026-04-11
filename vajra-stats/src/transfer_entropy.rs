//! Transfer entropy: directed information flow between time series.
//!
//! Transfer entropy measures how much knowing the past of source X reduces
//! uncertainty about the future of target Y, **beyond** what Y's own past
//! already tells you:
//!
//! ```text
//! TE(X→Y) = H(Y_t | Y_{t-1}^k) - H(Y_t | Y_{t-1}^k, X_{t-1}^l)
//! ```
//!
//! Key properties:
//! - TE(X→Y) ≠ TE(Y→X) — **directional** (reveals causal flow)
//! - TE ≥ 0 always (information can only help, never hurt prediction)
//! - Independent series → TE ≈ 0
//!
//! Reference: Schreiber, T. (2000). "Measuring information transfer."
//!            Physical Review Letters.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Result of transfer entropy computation between two series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferEntropyResult {
    /// TE(source → target): bits of directed information flow.
    pub te_source_to_target: f64,
    /// TE(target → source): reverse direction.
    pub te_target_to_source: f64,
    /// Net information flow: TE(X→Y) - TE(Y→X).
    /// Positive = source drives target. Negative = target drives source.
    pub net_flow: f64,
    /// History depth used for the computation.
    pub history_depth: usize,
    /// Number of valid transitions analyzed.
    pub transitions: usize,
}

impl Default for TransferEntropyResult {
    fn default() -> Self {
        Self {
            te_source_to_target: 0.0,
            te_target_to_source: 0.0,
            net_flow: 0.0,
            history_depth: 1,
            transitions: 0,
        }
    }
}

/// Compute transfer entropy TE(source → target) using binned estimation.
///
/// Both `source` and `target` are discrete time series of categorical values
/// (represented as u64 bin indices). `history_depth` controls how many past
/// steps to condition on (k=1 is the Markov assumption).
///
/// Returns 0.0 for empty or insufficient input.
#[must_use]
pub fn transfer_entropy(source: &[u64], target: &[u64], history_depth: usize) -> f64 {
    let n = source.len().min(target.len());
    if n <= history_depth || history_depth == 0 {
        return 0.0;
    }

    // Count joint occurrences for:
    // 1. p(y_t, y_past, x_past) — full joint
    // 2. p(y_t, y_past) — target-only joint
    // 3. p(y_past, x_past) — history joint
    // 4. p(y_past) — target history marginal
    //
    // TE = Σ p(y_t, y_past, x_past) * log2(p(y_t | y_past, x_past) / p(y_t | y_past))
    //    = Σ p(full) * log2((p(full) * p(y_past)) / (p(target_joint) * p(history_joint)))

    // Use tuples as keys for BTreeMap (deterministic ordering)
    let mut full_joint: BTreeMap<(u64, Vec<u64>, Vec<u64>), u64> = BTreeMap::new();
    let mut target_joint: BTreeMap<(u64, Vec<u64>), u64> = BTreeMap::new();
    let mut history_joint: BTreeMap<(Vec<u64>, Vec<u64>), u64> = BTreeMap::new();
    let mut target_marginal: BTreeMap<Vec<u64>, u64> = BTreeMap::new();

    let transitions = n - history_depth;

    for t in history_depth..n {
        let y_t = target[t];
        let y_past: Vec<u64> = target[t - history_depth..t].to_vec();
        let x_past: Vec<u64> = source[t - history_depth..t].to_vec();

        *full_joint
            .entry((y_t, y_past.clone(), x_past.clone()))
            .or_insert(0) += 1;
        *target_joint.entry((y_t, y_past.clone())).or_insert(0) += 1;
        *history_joint.entry((y_past.clone(), x_past)).or_insert(0) += 1;
        *target_marginal.entry(y_past).or_insert(0) += 1;
    }

    if transitions == 0 {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let total_f = transitions as f64;
    let mut te = 0.0_f64;

    for ((y_t, y_past, x_past), &full_count) in &full_joint {
        if full_count == 0 {
            continue;
        }

        let tj_count = target_joint
            .get(&(*y_t, y_past.clone()))
            .copied()
            .unwrap_or(0);
        let hj_count = history_joint
            .get(&(y_past.clone(), x_past.clone()))
            .copied()
            .unwrap_or(0);
        let tm_count = target_marginal.get(y_past).copied().unwrap_or(0);

        if tj_count == 0 || hj_count == 0 || tm_count == 0 {
            continue;
        }

        #[allow(clippy::cast_precision_loss)]
        let p_full = full_count as f64 / total_f;

        // log2((p_full * p_y_past) / (p_target_joint * p_history_joint))
        // = log2(full_count * tm_count / (tj_count * hj_count))
        #[allow(clippy::cast_precision_loss)]
        let ratio = (full_count as f64 * tm_count as f64) / (tj_count as f64 * hj_count as f64);

        if ratio > 0.0 {
            te += p_full * ratio.log2();
        }
    }

    // TE is theoretically ≥ 0; clamp to handle floating-point noise
    if te < 0.0 {
        0.0
    } else {
        te
    }
}

/// Compute full bidirectional transfer entropy between two discrete time series.
///
/// Returns TE in both directions plus net flow.
#[must_use]
pub fn bidirectional_transfer_entropy(
    source: &[u64],
    target: &[u64],
    history_depth: usize,
) -> TransferEntropyResult {
    let n = source.len().min(target.len());
    let k = if history_depth == 0 { 1 } else { history_depth };

    if n <= k {
        return TransferEntropyResult {
            history_depth: k,
            ..TransferEntropyResult::default()
        };
    }

    let te_xy = transfer_entropy(source, target, k);
    let te_yx = transfer_entropy(target, source, k);

    TransferEntropyResult {
        te_source_to_target: te_xy,
        te_target_to_source: te_yx,
        net_flow: te_xy - te_yx,
        history_depth: k,
        transitions: n - k,
    }
}

/// Bin continuous f64 values into discrete bins for transfer entropy estimation.
///
/// Uses equal-width binning with `num_bins` bins. Returns bin indices as u64.
#[must_use]
pub fn bin_values(values: &[f64], num_bins: usize) -> Vec<u64> {
    if values.is_empty() || num_bins == 0 {
        return Vec::new();
    }

    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;
    for &v in values {
        if v.is_finite() {
            if v < min_val {
                min_val = v;
            }
            if v > max_val {
                max_val = v;
            }
        }
    }

    if !min_val.is_finite() || !max_val.is_finite() || (max_val - min_val).abs() < f64::EPSILON {
        // All values identical or no finite values: all in bin 0
        return vec![0; values.len()];
    }

    let range = max_val - min_val;
    #[allow(clippy::cast_precision_loss)]
    let bin_width = range / num_bins as f64;

    values
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                return 0;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let bin = ((v - min_val) / bin_width).floor() as u64;
            #[allow(clippy::cast_possible_truncation)]
            let max_bin = (num_bins as u64).saturating_sub(1);
            bin.min(max_bin)
        })
        .collect()
}

/// Bin string values into discrete bins by mapping each unique string to an index.
///
/// Uses a BTreeMap for deterministic ordering.
#[must_use]
pub fn bin_strings(values: &[&str]) -> Vec<u64> {
    let mut label_map: BTreeMap<&str, u64> = BTreeMap::new();
    let mut next_id = 0u64;

    values
        .iter()
        .map(|&v| {
            *label_map.entry(v).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    // ---- transfer_entropy ----

    #[test]
    fn empty_returns_zero() {
        assert!((transfer_entropy(&[], &[], 1)).abs() < EPS);
    }

    #[test]
    fn single_element_returns_zero() {
        assert!((transfer_entropy(&[0], &[0], 1)).abs() < EPS);
    }

    #[test]
    fn independent_series_near_zero() {
        // Two independent constant series: TE should be 0
        let source = vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let target = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let te = transfer_entropy(&source, &target, 1);
        assert!(
            te.abs() < 0.1,
            "independent series should have TE near 0, got {te}"
        );
    }

    #[test]
    fn causal_coupling_positive() {
        // Y_t = X_{t-1}: target copies source with lag 1
        let source = vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let mut target = vec![0u64; source.len()];
        target[1..source.len()].copy_from_slice(&source[..(source.len() - 1)]);
        let te = transfer_entropy(&source, &target, 1);
        assert!(
            te > 0.0,
            "causal coupling should produce positive TE, got {te}"
        );
    }

    #[test]
    fn asymmetric_coupling() {
        // X drives Y but Y doesn't drive X
        let source = vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let mut target = vec![0u64; source.len()];
        target[1..source.len()].copy_from_slice(&source[..(source.len() - 1)]);
        let te_xy = transfer_entropy(&source, &target, 1);
        let te_yx = transfer_entropy(&target, &source, 1);
        // TE(X→Y) should be significantly larger than TE(Y→X)
        assert!(
            te_xy > te_yx,
            "TE(X→Y) = {te_xy} should be > TE(Y→X) = {te_yx}"
        );
    }

    #[test]
    fn te_non_negative() {
        let source = vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0];
        let target = vec![2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
        let te = transfer_entropy(&source, &target, 1);
        assert!(te >= 0.0, "TE should be non-negative, got {te}");
    }

    #[test]
    fn zero_history_depth() {
        let source = vec![0, 1, 0, 1];
        let target = vec![1, 0, 1, 0];
        assert!((transfer_entropy(&source, &target, 0)).abs() < EPS);
    }

    // ---- bidirectional_transfer_entropy ----

    #[test]
    fn bidirectional_consistent() {
        let source = vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
        let mut target = vec![0u64; source.len()];
        target[1..source.len()].copy_from_slice(&source[..(source.len() - 1)]);
        let result = bidirectional_transfer_entropy(&source, &target, 1);
        assert!(
            (result.net_flow - (result.te_source_to_target - result.te_target_to_source)).abs()
                < EPS
        );
        assert!(result.net_flow > 0.0, "net flow should be positive");
        assert_eq!(result.history_depth, 1);
        assert_eq!(result.transitions, source.len() - 1);
    }

    #[test]
    fn bidirectional_empty() {
        let result = bidirectional_transfer_entropy(&[], &[], 1);
        assert!(result.te_source_to_target.abs() < EPS);
        assert!(result.te_target_to_source.abs() < EPS);
        assert_eq!(result.transitions, 0);
    }

    // ---- bin_values ----

    #[test]
    fn bin_empty() {
        assert!(bin_values(&[], 10).is_empty());
    }

    #[test]
    fn bin_constant() {
        let bins = bin_values(&[5.0, 5.0, 5.0], 10);
        assert_eq!(bins, vec![0, 0, 0]);
    }

    #[test]
    fn bin_two_values() {
        let bins = bin_values(&[0.0, 1.0], 2);
        assert_eq!(bins[0], 0);
        assert_eq!(bins[1], 1);
    }

    #[test]
    fn bin_range() {
        let bins = bin_values(&[0.0, 0.25, 0.5, 0.75, 1.0], 4);
        // All bins should be in [0, 3]
        for &b in &bins {
            assert!(b <= 3, "bin {b} out of range");
        }
    }

    // ---- bin_strings ----

    #[test]
    fn bin_strings_deterministic() {
        let a = bin_strings(&["feat", "fix", "feat", "revert", "fix"]);
        let b = bin_strings(&["feat", "fix", "feat", "revert", "fix"]);
        assert_eq!(a, b);
    }

    #[test]
    fn bin_strings_distinct_ids() {
        let bins = bin_strings(&["a", "b", "c"]);
        assert_ne!(bins[0], bins[1]);
        assert_ne!(bins[1], bins[2]);
    }

    #[test]
    fn bin_strings_empty() {
        assert!(bin_strings(&[]).is_empty());
    }
}
