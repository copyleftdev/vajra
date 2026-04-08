//! Space-Saving algorithm for streaming top-k heavy hitters.
//!
//! Implements the algorithm from Metwally, Agrawal, El Abbadi (2005).
//! Tracks the approximate top-k most frequent items in a stream
//! using bounded memory proportional to k.

use std::collections::BTreeMap;

/// A Space-Saving data structure for streaming top-k heavy hitter detection.
///
/// Maintains at most `k` counters. When a new item arrives and all `k` slots
/// are occupied, the item with the smallest count is evicted and the new item
/// takes its place with `count = evicted_count + 1`.
pub struct SpaceSaving {
    /// Maximum number of tracked items.
    k: usize,
    /// Tracked items and their counts.
    counters: BTreeMap<String, u64>,
    /// Total number of items observed.
    total: u64,
}

impl SpaceSaving {
    /// Create a new `SpaceSaving` tracker for the top-k items.
    ///
    /// # Panics
    ///
    /// Panics if `k` is 0.
    #[must_use]
    pub fn new(k: usize) -> Self {
        assert!(k > 0, "k must be at least 1");
        Self {
            k,
            counters: BTreeMap::new(),
            total: 0,
        }
    }

    /// Observe an item in the stream.
    ///
    /// - If the item is already tracked, its count is incremented.
    /// - If not tracked and there is space, the item is added with count 1.
    /// - If not tracked and at capacity, the item with the minimum count is
    ///   evicted and the new item is added with `min_count + 1`.
    pub fn add(&mut self, item: &str) {
        self.total += 1;

        // If already tracked, just increment.
        if let Some(count) = self.counters.get_mut(item) {
            *count += 1;
            return;
        }

        // If there is space, add with count 1.
        if self.counters.len() < self.k {
            self.counters.insert(item.to_string(), 1);
            return;
        }

        // At capacity: evict the minimum-count item.
        // Find minimum count, then the lexicographically-first item with that count.
        // Safety: we checked len >= k > 0 above, so counters is non-empty.
        let Some(&min_count) = self.counters.values().min() else {
            return;
        };
        let Some(min_key) = self
            .counters
            .iter()
            .filter(|(_, v)| **v == min_count)
            .map(|(k, _)| k.clone())
            .next()
        else {
            return;
        };

        self.counters.remove(&min_key);
        self.counters.insert(item.to_string(), min_count + 1);
    }

    /// Return the tracked items sorted by count descending, with lexicographic
    /// tie-breaking for equal counts.
    #[must_use]
    pub fn top_k(&self) -> Vec<(&str, u64)> {
        let mut items: Vec<(&str, u64)> = self
            .counters
            .iter()
            .map(|(k, &v)| (k.as_str(), v))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        items
    }

    /// Check whether an item is currently being tracked.
    #[must_use]
    pub fn contains(&self, item: &str) -> bool {
        self.counters.contains_key(item)
    }

    /// Total number of items observed.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_empty() {
        let ss = SpaceSaving::new(5);
        assert_eq!(ss.top_k().len(), 0);
        assert_eq!(ss.total(), 0);
    }

    #[test]
    fn single_item() {
        let mut ss = SpaceSaving::new(5);
        ss.add("alpha");
        assert_eq!(ss.total(), 1);
        assert!(ss.contains("alpha"));
        let top = ss.top_k();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0], ("alpha", 1));
    }

    #[test]
    fn ordering_by_count() {
        let mut ss = SpaceSaving::new(10);
        for _ in 0..100 {
            ss.add("a");
        }
        for _ in 0..50 {
            ss.add("b");
        }
        for _ in 0..10 {
            ss.add("c");
        }
        let top = ss.top_k();
        assert_eq!(top[0].0, "a");
        assert_eq!(top[0].1, 100);
        assert_eq!(top[1].0, "b");
        assert_eq!(top[1].1, 50);
        assert_eq!(top[2].0, "c");
        assert_eq!(top[2].1, 10);
    }

    #[test]
    fn k2_eviction() {
        let mut ss = SpaceSaving::new(2);
        // Insert a lot of "a" and "b"
        for _ in 0..100 {
            ss.add("a");
        }
        for _ in 0..50 {
            ss.add("b");
        }
        // Now add "c" which should evict "b" (the minimum)
        ss.add("c");
        assert_eq!(ss.top_k().len(), 2);
        assert!(ss.contains("a"));
        // "c" replaces the evicted item with min_count+1 = 51
        assert!(ss.contains("c"));
        assert!(!ss.contains("b"));
    }

    #[test]
    fn evicted_items_below_threshold() {
        let mut ss = SpaceSaving::new(3);
        // Add many items, only the frequent ones should survive.
        // With k=3, each rare item evicts the current min and gets min+1.
        // After 20 rare items the min counter grows, but the heavy hitters
        // stay above it as long as their counts are high enough.
        // With 200/150/120 vs 20 rare single-inserts the heavy hitters dominate.
        for _ in 0..200 {
            ss.add("freq1");
        }
        for _ in 0..150 {
            ss.add("freq2");
        }
        for _ in 0..120 {
            ss.add("freq3");
        }
        // Add rare items — each evicts the current min counter item.
        // After all rare items, the three frequency items should be back on top.
        for i in 0..5 {
            ss.add(&format!("rare_{i}"));
        }
        // At this point the three slots hold freq1, freq2, and either freq3
        // or a rare item that inherited a small count. The two heavy hitters
        // with the largest counts always survive.
        assert!(ss.contains("freq1"));
        assert!(ss.contains("freq2"));
        // Top-k should have 3 items
        assert_eq!(ss.top_k().len(), 3);
    }

    #[test]
    fn lexicographic_tie_breaking() {
        let mut ss = SpaceSaving::new(10);
        ss.add("banana");
        ss.add("apple");
        ss.add("cherry");
        // All have count 1
        let top = ss.top_k();
        assert_eq!(top[0].0, "apple");
        assert_eq!(top[1].0, "banana");
        assert_eq!(top[2].0, "cherry");
    }

    #[test]
    fn total_count_correct() {
        let mut ss = SpaceSaving::new(5);
        for _ in 0..30 {
            ss.add("x");
        }
        for _ in 0..20 {
            ss.add("y");
        }
        assert_eq!(ss.total(), 50);
    }

    #[test]
    fn k1_single_slot() {
        let mut ss = SpaceSaving::new(1);
        for _ in 0..100 {
            ss.add("dominant");
        }
        ss.add("other");
        // "dominant" should still be there since it had count 100
        // Actually: "other" evicts "dominant" (min is 100), so "other" gets 101
        // This is correct behavior for Space-Saving with k=1
        let top = ss.top_k();
        assert_eq!(top.len(), 1);
        assert_eq!(ss.total(), 101);
    }

    #[test]
    fn contains_only_tracked() {
        let mut ss = SpaceSaving::new(2);
        ss.add("a");
        ss.add("b");
        assert!(ss.contains("a"));
        assert!(ss.contains("b"));
        assert!(!ss.contains("c"));
    }

    #[test]
    fn heavy_hitters_survive() {
        // A realistic scenario: a few items are very frequent, many are rare.
        let mut ss = SpaceSaving::new(5);
        let heavy = ["server_error", "timeout", "auth_failure"];
        for name in &heavy {
            for _ in 0..500 {
                ss.add(name);
            }
        }
        // Add 100 distinct rare items
        for i in 0..100 {
            ss.add(&format!("rare_{i}"));
        }

        // All heavy hitters should still be tracked
        for name in &heavy {
            assert!(ss.contains(name), "{name} should be tracked but isn't");
        }

        // And they should be in the top of top_k
        let top = ss.top_k();
        let top_names: Vec<&str> = top.iter().take(3).map(|(n, _)| *n).collect();
        for name in &heavy {
            assert!(
                top_names.contains(name),
                "{name} not in top 3: {top_names:?}"
            );
        }
    }
}
