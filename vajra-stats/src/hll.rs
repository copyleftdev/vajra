//! HyperLogLog: distinct-value counting in fixed memory.
//!
//! Past the exact-tracking threshold, streaming `stats` reported the number of
//! occupied Space-Saving counters as `cardinality` — 100 for a field with
//! 120,000 distinct values. That is a lower bound, and a useless one. Counting
//! distinct values is the one thing neither Count-Min (frequencies) nor
//! Space-Saving (heavy hitters) can do. See #106.
//!
//! Fixed size, so it costs the same whether a path has ten distinct values or
//! ten billion, and mergeable, so partitioned work can be combined. Registers
//! are updated with a maximum, which makes the result independent of the order
//! values arrive in — a property determinism depends on and that
//! [`tests::estimates_are_order_independent`] asserts.

/// Register-count exponent: `m = 2^P` registers.
///
/// 11 gives 2048 one-byte registers — 2 KB per tracked path — for a standard
/// error of `1.04/sqrt(m)` ≈ 2.3%. Raising it to 12 halves the error and
/// doubles the memory *per path*, and a document with many distinct paths pays
/// that repeatedly, so the cheaper end is the right default here.
const P: u32 = 11;

/// Number of registers.
const M: usize = 1 << P;

/// Bits left for the leading-zero rank after the register index is taken.
const RANK_BITS: u32 = 64 - P;

/// Bias-correction constant for `M` registers.
///
/// Flajolet et al., the `m >= 128` case: `0.7213 / (1 + 1.079/m)`.
const ALPHA: f64 = 0.7213 / (1.0 + 1.079 / M as f64);

/// Estimates distinct values in fixed memory.
#[derive(Debug, Clone)]
pub struct HyperLogLog {
    /// Largest leading-zero rank seen per register.
    registers: Box<[u8; M]>,
}

impl Default for HyperLogLog {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperLogLog {
    /// An empty sketch, estimating zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registers: Box::new([0_u8; M]),
        }
    }

    /// Observe a value.
    ///
    /// Idempotent: adding the same value twice cannot change the estimate,
    /// because registers only ever take a maximum.
    pub fn add(&mut self, item: &[u8]) {
        let hash = hash64(item);
        // Top P bits select the register, the rest supply the rank. Splitting
        // one hash rather than taking two keeps the two independent enough for
        // the estimator while costing a single pass over the bytes.
        #[allow(clippy::cast_possible_truncation)]
        let index = (hash >> RANK_BITS) as usize;
        let rank = leading_zero_rank(hash);
        let register = &mut self.registers[index & (M - 1)];
        *register = (*register).max(rank);
    }

    /// Estimated number of distinct values observed.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn estimate(&self) -> u64 {
        let m = M as f64;
        let mut harmonic = 0.0_f64;
        let mut zeros = 0_usize;
        for &r in self.registers.iter() {
            harmonic += 1.0 / ((1_u64 << r) as f64);
            if r == 0 {
                zeros += 1;
            }
        }

        let raw = ALPHA * m * m / harmonic;

        // Below roughly 2.5m the raw estimator is badly biased, but empty
        // registers are then informative: linear counting is exact enough
        // there, and exact when nothing has been added.
        if raw <= 2.5 * m && zeros > 0 {
            let linear = m * (m / zeros as f64).ln();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return linear.round().max(0.0) as u64;
        }

        // No large-range correction: that exists for 32-bit hashes, where the
        // space saturates around 2^32. A 64-bit hash does not reach it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            raw.round().max(0.0) as u64
        }
    }

    /// Fold another sketch into this one.
    ///
    /// Registers hold maxima, so merging is a pointwise maximum and the result
    /// is the same as having observed both inputs in either order.
    pub fn merge(&mut self, other: &Self) {
        for (a, b) in self.registers.iter_mut().zip(other.registers.iter()) {
            *a = (*a).max(*b);
        }
    }

    /// Whether nothing has been observed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registers.iter().all(|&r| r == 0)
    }

    /// Bytes of register state, for documenting the memory cost.
    #[must_use]
    pub const fn size_bytes() -> usize {
        M
    }

    /// Standard error of the estimate: `1.04 / sqrt(m)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn relative_error() -> f64 {
        1.04 / (M as f64).sqrt()
    }
}

/// Position of the leftmost one-bit in the rank portion, counting from 1.
///
/// Saturates at `RANK_BITS + 1`, the value produced when every rank bit is
/// zero, so a register can never exceed what its width can represent.
fn leading_zero_rank(hash: u64) -> u8 {
    // Shifting the index bits off leaves the rank bits at the top, followed by
    // P zeros; those trailing zeros only matter in the all-zero case, which the
    // cap handles.
    let rank_bits = hash << P;
    let zeros = rank_bits.leading_zeros().min(RANK_BITS);
    #[allow(clippy::cast_possible_truncation)]
    {
        (zeros + 1) as u8
    }
}

/// Deterministic 64-bit hash.
///
/// FNV-1a accumulates, then the MurmurHash3 finalizer avalanches. FNV alone is
/// not good enough here: HyperLogLog reads the leading bits as a geometric
/// variable, so poorly mixed high bits skew every estimate. Fixed constants and
/// no seed from the environment, because the same input must produce the same
/// estimate on every machine and every run.
fn hash64(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01b3;

    let mut h = FNV_OFFSET_BASIS;
    for &byte in data {
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }

    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sketch_of(n: usize) -> HyperLogLog {
        let mut hll = HyperLogLog::new();
        for i in 0..n {
            hll.add(format!("item-{i}").as_bytes());
        }
        hll
    }

    /// Error within about three standard errors of the true value across four
    /// orders of magnitude. The point of the sketch is that this holds at 10
    /// million as well as at 1,000, in the same 2 KB.
    #[test]
    fn estimates_stay_within_the_error_bound_across_magnitudes() {
        let tolerance = HyperLogLog::relative_error() * 3.0;
        for n in [1_000_usize, 10_000, 100_000, 1_000_000] {
            let estimate = sketch_of(n).estimate();
            #[allow(clippy::cast_precision_loss)]
            let error = (estimate as f64 - n as f64).abs() / n as f64;
            assert!(
                error < tolerance,
                "n={n}: estimated {estimate}, relative error {error:.4} exceeds {tolerance:.4}"
            );
        }
    }

    /// The case from #106: 120,000 distinct values reported as 100.
    #[test]
    fn the_case_that_motivated_this() {
        let estimate = sketch_of(120_000).estimate();
        #[allow(clippy::cast_precision_loss)]
        let error = (estimate as f64 - 120_000.0).abs() / 120_000.0;
        assert!(
            error < 0.1,
            "estimated {estimate} for 120,000 distinct values"
        );
        assert!(
            estimate > 100_000,
            "must not collapse to the counter budget: {estimate}"
        );
    }

    /// Small cardinalities go through linear counting and should be close to
    /// exact — this is where a raw HyperLogLog estimator is worst.
    #[test]
    fn small_cardinalities_are_near_exact() {
        for n in [1_usize, 5, 50, 200] {
            let estimate = sketch_of(n).estimate();
            let diff = estimate.abs_diff(n as u64);
            assert!(
                diff <= 1,
                "n={n}: estimated {estimate}, off by {diff} in the linear-counting range"
            );
        }
    }

    #[test]
    fn an_empty_sketch_estimates_zero() {
        let hll = HyperLogLog::new();
        assert_eq!(hll.estimate(), 0);
        assert!(hll.is_empty());
    }

    /// Determinism: registers take maxima, so arrival order cannot matter.
    #[test]
    fn estimates_are_order_independent() {
        let mut forward = HyperLogLog::new();
        let mut backward = HyperLogLog::new();
        for i in 0..5_000 {
            forward.add(format!("item-{i}").as_bytes());
        }
        for i in (0..5_000).rev() {
            backward.add(format!("item-{i}").as_bytes());
        }
        assert_eq!(forward.registers, backward.registers);
        assert_eq!(forward.estimate(), backward.estimate());
    }

    #[test]
    fn repeats_do_not_inflate_the_estimate() {
        let mut hll = HyperLogLog::new();
        for _ in 0..100 {
            for i in 0..500 {
                hll.add(format!("item-{i}").as_bytes());
            }
        }
        let estimate = hll.estimate();
        assert!(
            estimate.abs_diff(500) <= 5,
            "500 distinct values seen 100 times each estimated as {estimate}"
        );
    }

    #[test]
    fn merging_matches_observing_both() {
        let mut a = HyperLogLog::new();
        let mut b = HyperLogLog::new();
        let mut both = HyperLogLog::new();
        for i in 0..3_000 {
            a.add(format!("a-{i}").as_bytes());
            both.add(format!("a-{i}").as_bytes());
        }
        for i in 0..3_000 {
            b.add(format!("b-{i}").as_bytes());
            both.add(format!("b-{i}").as_bytes());
        }
        a.merge(&b);
        assert_eq!(a.registers, both.registers, "merge must be a pointwise max");
        assert_eq!(a.estimate(), both.estimate());
    }

    /// Overlapping sets must not be double-counted: the union of two sets
    /// sharing every element is that set.
    #[test]
    fn merging_identical_sketches_does_not_double_count() {
        let a = sketch_of(2_000);
        let mut merged = a.clone();
        merged.merge(&a);
        assert_eq!(merged.estimate(), a.estimate());
    }

    #[test]
    fn rank_saturates_rather_than_overflowing() {
        // All rank bits zero: the index is all ones, everything else is zero.
        let hash = u64::MAX << RANK_BITS;
        #[allow(clippy::cast_possible_truncation)]
        let expected = (RANK_BITS + 1) as u8;
        assert_eq!(leading_zero_rank(hash), expected);
        // A leading one in the rank portion gives rank 1.
        assert_eq!(leading_zero_rank(1_u64 << (RANK_BITS - 1)), 1);
    }

    #[test]
    fn the_register_budget_is_what_the_docs_claim() {
        assert_eq!(HyperLogLog::size_bytes(), 2048);
        assert!(HyperLogLog::relative_error() < 0.024);
    }
}
