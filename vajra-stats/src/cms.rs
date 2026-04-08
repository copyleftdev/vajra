//! Count-Min Sketch with Conservative Update.
//!
//! Streaming frequency estimation (Cormode & Muthukrishnan 2005) with
//! conservative update optimization (Estan & Varghese 2002).
//! Provides sub-linear space frequency estimation with bounded overcount.

/// A Count-Min Sketch with conservative update for streaming frequency estimation.
///
/// Guarantees that for any item, `estimate(item) >= true_count(item)` and
/// `estimate(item) <= true_count(item) + epsilon * total` with probability
/// at least `1 - delta`.
pub struct CountMinSketch {
    width: usize,
    depth: usize,
    table: Vec<Vec<u64>>,
    seeds: Vec<u64>,
    total: u64,
}

/// FNV-1a hash for a byte slice with a seed mixed in.
fn fnv1a(data: &[u8], seed: u64) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS ^ seed;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

impl CountMinSketch {
    /// Create a new Count-Min Sketch with the given error and confidence parameters.
    ///
    /// - `epsilon`: error factor. For any item, `estimate <= true_count + epsilon * total`.
    /// - `delta`: failure probability. The guarantee holds with probability `1 - delta`.
    ///
    /// # Panics
    ///
    /// Panics if `epsilon` or `delta` are not in `(0.0, 1.0)`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn new(epsilon: f64, delta: f64) -> Self {
        assert!(
            epsilon > 0.0 && epsilon < 1.0,
            "epsilon must be in (0.0, 1.0)"
        );
        assert!(delta > 0.0 && delta < 1.0, "delta must be in (0.0, 1.0)");

        let width = (std::f64::consts::E / epsilon).ceil() as usize;
        let depth = (1.0_f64 / delta).ln().ceil() as usize;

        // Deterministic seeds derived from row index.
        let seeds: Vec<u64> = (0..depth)
            .map(|i| 0x9e37_79b9_7f4a_7c15_u64.wrapping_mul(i as u64 + 1))
            .collect();

        Self {
            width,
            depth,
            table: vec![vec![0u64; width]; depth],
            seeds,
            total: 0,
        }
    }

    /// Hash an item for a given row.
    fn hash_for_row(&self, item: &[u8], row: usize) -> usize {
        let h = fnv1a(item, self.seeds[row]);
        (h as usize) % self.width
    }

    /// Add `count` occurrences of `item` using conservative update.
    ///
    /// Conservative update only increments counters that are at the current
    /// minimum, reducing overcounting.
    pub fn add(&mut self, item: &[u8], count: u64) {
        self.total += count;

        // Compute all column indices.
        let cols: Vec<usize> = (0..self.depth)
            .map(|row| self.hash_for_row(item, row))
            .collect();

        // Find the current minimum across all rows for this item.
        let min_count = cols
            .iter()
            .enumerate()
            .map(|(row, &col)| self.table[row][col])
            .min()
            .unwrap_or(0);

        let new_val = min_count + count;

        // Conservative update: only increment counters that are at the minimum.
        for (row, &col) in cols.iter().enumerate() {
            if self.table[row][col] < new_val {
                self.table[row][col] = new_val;
            }
        }
    }

    /// Estimate the count of `item`.
    ///
    /// Returns the minimum counter value across all rows, which is always
    /// `>= true_count` and `<= true_count + epsilon * total` w.h.p.
    #[must_use]
    pub fn estimate(&self, item: &[u8]) -> u64 {
        (0..self.depth)
            .map(|row| {
                let col = self.hash_for_row(item, row);
                self.table[row][col]
            })
            .min()
            .unwrap_or(0)
    }

    /// Total count of all insertions.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Merge another sketch into this one.
    ///
    /// Both sketches must have the same dimensions (width and depth).
    ///
    /// # Panics
    ///
    /// Panics if the dimensions differ.
    pub fn merge(&mut self, other: &CountMinSketch) {
        assert_eq!(self.width, other.width, "Width mismatch in CMS merge");
        assert_eq!(self.depth, other.depth, "Depth mismatch in CMS merge");

        for row in 0..self.depth {
            for col in 0..self.width {
                self.table[row][col] += other.table[row][col];
            }
        }
        self.total += other.total;
    }
}

impl Default for CountMinSketch {
    fn default() -> Self {
        Self::new(0.001, 0.01)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_estimate() {
        let mut cms = CountMinSketch::default();
        for _ in 0..100 {
            cms.add(b"hello", 1);
        }
        assert!(
            cms.estimate(b"hello") >= 100,
            "estimate={}, expected >= 100",
            cms.estimate(b"hello")
        );
    }

    #[test]
    fn unseen_item_zero_or_small() {
        let mut cms = CountMinSketch::default();
        cms.add(b"hello", 100);
        let est = cms.estimate(b"unseen");
        // Should be 0 or very small relative to total
        assert!(
            est <= 1,
            "estimate of unseen item = {est}, expected 0 or near 0"
        );
    }

    #[test]
    fn total_count_exact() {
        let mut cms = CountMinSketch::default();
        cms.add(b"a", 10);
        cms.add(b"b", 20);
        cms.add(b"c", 30);
        assert_eq!(cms.total(), 60);
    }

    #[test]
    fn no_underestimation() {
        let mut cms = CountMinSketch::default();
        cms.add(b"x", 50);
        cms.add(b"y", 30);
        cms.add(b"z", 20);
        assert!(cms.estimate(b"x") >= 50);
        assert!(cms.estimate(b"y") >= 30);
        assert!(cms.estimate(b"z") >= 20);
    }

    #[test]
    fn error_bound() {
        let epsilon = 0.01;
        let mut cms = CountMinSketch::new(epsilon, 0.01);

        // Insert many distinct items
        for i in 0..1000_u64 {
            let key = format!("item_{i}");
            cms.add(key.as_bytes(), 1);
        }

        // Check heavy hitter
        cms.add(b"heavy", 500);
        let est = cms.estimate(b"heavy");
        let bound = 500 + (epsilon * cms.total() as f64).ceil() as u64;
        assert!(est <= bound, "estimate={est}, bound={bound}");
        assert!(est >= 500, "estimate={est}, expected >= 500");
    }

    #[test]
    fn merge_two_sketches() {
        let epsilon = 0.001;
        let delta = 0.01;
        let mut cms1 = CountMinSketch::new(epsilon, delta);
        let mut cms2 = CountMinSketch::new(epsilon, delta);

        cms1.add(b"shared", 40);
        cms1.add(b"only1", 10);

        cms2.add(b"shared", 60);
        cms2.add(b"only2", 20);

        cms1.merge(&cms2);

        assert_eq!(cms1.total(), 130);
        assert!(cms1.estimate(b"shared") >= 100);
        assert!(cms1.estimate(b"only1") >= 10);
        assert!(cms1.estimate(b"only2") >= 20);
    }

    #[test]
    fn add_bulk_count() {
        let mut cms = CountMinSketch::default();
        cms.add(b"item", 1000);
        assert!(cms.estimate(b"item") >= 1000);
        assert_eq!(cms.total(), 1000);
    }

    #[test]
    fn empty_sketch() {
        let cms = CountMinSketch::default();
        assert_eq!(cms.total(), 0);
        assert_eq!(cms.estimate(b"anything"), 0);
    }

    #[test]
    fn many_items_overcount_bounded() {
        let epsilon = 0.01;
        let mut cms = CountMinSketch::new(epsilon, 0.001);

        // Insert 100 items each 10 times
        for i in 0..100_u64 {
            let key = format!("k{i}");
            cms.add(key.as_bytes(), 10);
        }

        // Most items should have estimate close to 10
        let mut overcount_violations = 0;
        for i in 0..100_u64 {
            let key = format!("k{i}");
            let est = cms.estimate(key.as_bytes());
            let bound = 10 + (epsilon * cms.total() as f64).ceil() as u64;
            if est > bound {
                overcount_violations += 1;
            }
        }
        // With delta = 0.001, less than ~0.1% should violate
        assert!(
            overcount_violations == 0,
            "violations={overcount_violations}"
        );
    }

    #[test]
    fn fnv1a_deterministic() {
        let h1 = fnv1a(b"hello", 42);
        let h2 = fnv1a(b"hello", 42);
        assert_eq!(h1, h2);

        let h3 = fnv1a(b"hello", 43);
        assert_ne!(h1, h3);
    }
}
