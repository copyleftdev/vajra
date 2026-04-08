//! Streaming statistics accumulator with bounded memory.
//!
//! Uses [`DDSketch`] for numeric distributions, [`CountMinSketch`] for
//! high-cardinality frequency estimation, and [`SpaceSaving`] for top-k
//! detection.  Processes [`JsonEvent`]s one at a time and produces the
//! same [`StatsResult`] as the DOM-based [`StatsAnalyzer`].

use std::collections::BTreeMap;

use vajra_core::event::{JsonEvent, ScalarValue};
use vajra_types::path::WildcardPath;

use crate::analyzer::{PathStats, StatsResult};
use crate::cms::CountMinSketch;
use crate::ddsketch::DDSketch;
use crate::entropy;
use crate::space_saving::SpaceSaving;

/// Configuration for the streaming stats pipeline.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// DDSketch relative accuracy (default 0.01).
    pub ddsketch_alpha: f64,
    /// CMS error rate (default 0.001).
    pub cms_epsilon: f64,
    /// CMS failure probability (default 0.01).
    pub cms_delta: f64,
    /// Space-Saving k (default 100).
    pub top_k: usize,
    /// Cardinality threshold: switch from exact to CMS (default 10_000).
    pub exact_threshold: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            ddsketch_alpha: 0.01,
            cms_epsilon: 0.001,
            cms_delta: 0.01,
            top_k: 100,
            exact_threshold: 10_000,
        }
    }
}

/// Tracks value frequencies, starting exact and switching to approximate
/// when cardinality exceeds the configured threshold.
enum ValueTracker {
    /// Exact counting (below cardinality threshold).
    Exact(BTreeMap<String, u64>),
    /// Approximate counting (above threshold).
    Approximate(CountMinSketch),
}

impl ValueTracker {
    fn new_exact() -> Self {
        Self::Exact(BTreeMap::new())
    }
}

/// Per-path accumulator for streaming statistics.
struct PathAccumulator {
    /// Total observations.
    count: u64,
    /// Type counts (fixed array, always exact).
    type_counts: [u64; 7],
    /// Null count.
    null_count: u64,
    /// Value tracking: starts exact, switches to CMS at threshold.
    values: ValueTracker,
    /// Numeric distribution via DDSketch.
    numeric_sketch: Option<DDSketch>,
    /// Top-k values via Space-Saving.
    top_k: SpaceSaving,
    /// Config reference values (needed for transitions).
    exact_threshold: usize,
    /// CMS parameters for when we need to create one.
    cms_epsilon: f64,
    cms_delta: f64,
    /// DDSketch alpha.
    ddsketch_alpha: f64,
}

impl PathAccumulator {
    fn new(config: &StreamingConfig) -> Self {
        Self {
            count: 0,
            type_counts: [0; 7],
            null_count: 0,
            values: ValueTracker::new_exact(),
            numeric_sketch: None,
            top_k: SpaceSaving::new(config.top_k),
            exact_threshold: config.exact_threshold,
            cms_epsilon: config.cms_epsilon,
            cms_delta: config.cms_delta,
            ddsketch_alpha: config.ddsketch_alpha,
        }
    }

    fn observe(&mut self, value: &ScalarValue) {
        self.count += 1;

        let type_idx = value.type_index();
        self.type_counts[type_idx] += 1;

        if value.is_null() {
            self.null_count += 1;
        }

        // Track numeric values in DDSketch
        if let Some(f) = value.as_f64() {
            let sketch = self
                .numeric_sketch
                .get_or_insert_with(|| DDSketch::new(self.ddsketch_alpha));
            sketch.add(f);
        }

        let canonical = value.to_canonical_string();

        // Track in Space-Saving for top-k
        self.top_k.add(&canonical);

        // Track in value frequency counter
        match &mut self.values {
            ValueTracker::Exact(map) => {
                *map.entry(canonical).or_insert(0) += 1;

                // Check if we need to transition to approximate
                if map.len() > self.exact_threshold {
                    let mut cms = CountMinSketch::new(self.cms_epsilon, self.cms_delta);
                    for (k, &v) in map.iter() {
                        cms.add(k.as_bytes(), v);
                    }
                    self.values = ValueTracker::Approximate(cms);
                }
            }
            ValueTracker::Approximate(cms) => {
                cms.add(canonical.as_bytes(), 1);
            }
        }
    }
}

/// Streaming statistics accumulator with bounded memory.
///
/// Uses DDSketch for numeric distributions, CMS for high-cardinality
/// frequency estimation, and Space-Saving for top-k detection.
pub struct StreamingStatsAccumulator {
    /// Per-path statistics accumulators.
    path_accumulators: BTreeMap<WildcardPath, PathAccumulator>,
    /// Total events processed.
    total_events: u64,
    /// Configuration.
    config: StreamingConfig,
}

impl StreamingStatsAccumulator {
    /// Create a new accumulator with the given configuration.
    #[must_use]
    pub fn new(config: StreamingConfig) -> Self {
        Self {
            path_accumulators: BTreeMap::new(),
            total_events: 0,
            config,
        }
    }

    /// Process a single event.
    pub fn process_event(&mut self, event: &JsonEvent) {
        self.total_events += 1;

        if let JsonEvent::Value { path, value } = event {
            let config = &self.config;
            let acc = self
                .path_accumulators
                .entry(path.clone())
                .or_insert_with(|| PathAccumulator::new(config));
            acc.observe(value);
        }
        // Container start/end events don't contribute to scalar stats
    }

    /// Process a batch of events.
    pub fn process_events(&mut self, events: &[JsonEvent]) {
        for event in events {
            self.process_event(event);
        }
    }

    /// Total events processed.
    #[must_use]
    pub const fn total_events(&self) -> u64 {
        self.total_events
    }

    /// Check if the value tracker for a path is in exact mode.
    #[must_use]
    pub fn is_exact(&self, path: &WildcardPath) -> bool {
        self.path_accumulators
            .get(path)
            .map_or(true, |acc| matches!(&acc.values, ValueTracker::Exact(_)))
    }

    /// Produce the final `StatsResult`, consuming the accumulator.
    #[must_use]
    pub fn finalize(self) -> StatsResult {
        let mut paths = BTreeMap::new();

        for (path, acc) in self.path_accumulators {
            let (h, nh, cardinality, max_rarity) = match &acc.values {
                ValueTracker::Exact(map) => {
                    let count_vec: Vec<u64> = map.values().copied().collect();
                    let card = map.len() as u64;
                    let h = entropy::shannon_entropy_from_counts(&count_vec);
                    #[allow(clippy::cast_possible_truncation)]
                    let nh = entropy::normalized_entropy(h, card as usize);
                    let total: u64 = count_vec.iter().sum();
                    #[allow(clippy::cast_precision_loss)]
                    let max_rarity = if total > 0 {
                        let min_count = count_vec
                            .iter()
                            .copied()
                            .filter(|&c| c > 0)
                            .min()
                            .unwrap_or(1);
                        let min_p = min_count as f64 / total as f64;
                        -min_p.log2()
                    } else {
                        0.0
                    };
                    (h, nh, card, max_rarity)
                }
                ValueTracker::Approximate(_cms) => {
                    // In approximate mode, we estimate from top-k
                    let top = acc.top_k.top_k();
                    let count_vec: Vec<u64> = top.iter().map(|(_, c)| *c).collect();
                    let card = top.len() as u64; // Lower-bound estimate
                    let h = entropy::shannon_entropy_from_counts(&count_vec);
                    #[allow(clippy::cast_possible_truncation)]
                    let nh = entropy::normalized_entropy(h, card as usize);
                    let total: u64 = count_vec.iter().sum();
                    #[allow(clippy::cast_precision_loss)]
                    let max_rarity = if total > 0 {
                        let min_count = count_vec
                            .iter()
                            .copied()
                            .filter(|&c| c > 0)
                            .min()
                            .unwrap_or(1);
                        let min_p = min_count as f64 / total as f64;
                        -min_p.log2()
                    } else {
                        0.0
                    };
                    (h, nh, card, max_rarity)
                }
            };

            // Numeric stats from DDSketch
            let numeric_stats = acc.numeric_sketch.as_ref().and_then(|sk| {
                if sk.is_empty() {
                    return None;
                }
                let min = sk.min().unwrap_or(0.0);
                let max = sk.max().unwrap_or(0.0);
                let mean = sk.mean().unwrap_or(0.0);
                let median = sk.quantile(0.5).unwrap_or(0.0);
                let p05 = sk.quantile(0.05).unwrap_or(0.0);
                let p25 = sk.quantile(0.25).unwrap_or(0.0);
                let p75 = sk.quantile(0.75).unwrap_or(0.0);
                let p95 = sk.quantile(0.95).unwrap_or(0.0);

                // Approximate MAD from DDSketch: MAD ~ 0.6745 * (p75 - p25) / 2
                // This is a rough approximation; for exact MAD we'd need
                // a second sketch centered on the median.
                let iqr = p75 - p25;
                let approx_mad = iqr * 0.5;

                Some(crate::numeric::NumericStats {
                    min,
                    max,
                    mean,
                    median,
                    mad: approx_mad,
                    p05,
                    p25,
                    p75,
                    p95,
                    count: sk.count() as usize,
                })
            });

            // Top-k from Space-Saving
            let top_values: Vec<(String, u64)> = acc
                .top_k
                .top_k()
                .into_iter()
                .take(10) // Default top-10 for output
                .map(|(k, v)| (k.to_owned(), v))
                .collect();

            paths.insert(
                path,
                PathStats {
                    entropy: h,
                    normalized_entropy: nh,
                    cardinality,
                    total_count: acc.count,
                    max_rarity,
                    numeric_stats,
                    top_values,
                },
            );
        }

        StatsResult { paths }
    }

    /// Merge another accumulator into this one (for parallel batch processing).
    ///
    /// Both accumulators must have the same configuration.
    pub fn merge(&mut self, other: Self) {
        self.total_events += other.total_events;

        for (path, other_acc) in other.path_accumulators {
            if let Some(self_acc) = self.path_accumulators.get_mut(&path) {
                // Merge counts
                self_acc.count += other_acc.count;
                self_acc.null_count += other_acc.null_count;
                for i in 0..7 {
                    self_acc.type_counts[i] += other_acc.type_counts[i];
                }

                // Merge numeric sketch
                match (&mut self_acc.numeric_sketch, &other_acc.numeric_sketch) {
                    (Some(self_sk), Some(other_sk)) => {
                        self_sk.merge(other_sk);
                    }
                    (None, Some(other_sk)) => {
                        // Clone by creating new and merging
                        let mut new_sk = DDSketch::new(self_acc.ddsketch_alpha);
                        new_sk.merge(other_sk);
                        self_acc.numeric_sketch = Some(new_sk);
                    }
                    _ => {}
                }

                // Merge value trackers
                match (&mut self_acc.values, other_acc.values) {
                    (ValueTracker::Exact(self_map), ValueTracker::Exact(other_map)) => {
                        for (k, v) in other_map {
                            *self_map.entry(k).or_insert(0) += v;
                        }
                        // Check if we need to transition
                        if self_map.len() > self_acc.exact_threshold {
                            let mut cms =
                                CountMinSketch::new(self_acc.cms_epsilon, self_acc.cms_delta);
                            for (k, &v) in self_map.iter() {
                                cms.add(k.as_bytes(), v);
                            }
                            self_acc.values = ValueTracker::Approximate(cms);
                        }
                    }
                    (ValueTracker::Approximate(self_cms), ValueTracker::Exact(other_map)) => {
                        for (k, v) in &other_map {
                            self_cms.add(k.as_bytes(), *v);
                        }
                    }
                    (ValueTracker::Exact(self_map), ValueTracker::Approximate(other_cms)) => {
                        let mut cms = CountMinSketch::new(self_acc.cms_epsilon, self_acc.cms_delta);
                        for (k, &v) in self_map.iter() {
                            cms.add(k.as_bytes(), v);
                        }
                        cms.merge(&other_cms);
                        self_acc.values = ValueTracker::Approximate(cms);
                    }
                    (ValueTracker::Approximate(self_cms), ValueTracker::Approximate(other_cms)) => {
                        self_cms.merge(&other_cms);
                    }
                }

                // Note: SpaceSaving doesn't have a merge operation.
                // The top-k from `self` is kept as-is. For a proper merge
                // we would need to replay events, but this is a known
                // limitation for the Space-Saving algorithm in parallel settings.
            } else {
                self.path_accumulators.insert(path, other_acc);
            }
        }
    }
}

impl Default for StreamingStatsAccumulator {
    fn default() -> Self {
        Self::new(StreamingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_core::stream::emit_events;
    use vajra_types::Analyzer;

    fn parse_value(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap_or_else(|e| {
            panic!("parse failed: {e}");
        })
    }

    fn parse_doc(json: &str) -> vajra_types::Document {
        vajra_core::parse_str(json).unwrap_or_else(|e| {
            panic!("parse failed: {e}");
        })
    }

    // Test 4: Streaming stats vs DOM stats - same document, equivalent results
    #[test]
    fn streaming_vs_dom_stats_equivalent() {
        let json = r#"["a", "b", "a", "c", "a", "b"]"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        // DOM stats
        let dom_result = crate::analyzer::StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("dom analyze failed: {e}"));

        // Streaming stats
        let events = emit_events(&value);
        let mut acc = StreamingStatsAccumulator::default();
        acc.process_events(&events);
        let stream_result = acc.finalize();

        let path = WildcardPath::root().push_array_wildcard();

        let dom_stats = dom_result
            .paths
            .get(&path)
            .unwrap_or_else(|| panic!("missing dom"));
        let stream_stats = stream_result
            .paths
            .get(&path)
            .unwrap_or_else(|| panic!("missing stream"));

        assert_eq!(dom_stats.cardinality, stream_stats.cardinality);
        assert_eq!(dom_stats.total_count, stream_stats.total_count);
        assert!((dom_stats.entropy - stream_stats.entropy).abs() < 1e-10);
    }

    // Test 5: Entropy matches DOM entropy for exact mode (below threshold)
    #[test]
    fn streaming_entropy_matches_dom_exact() {
        let json = r#"["x", "y", "x", "x", "y", "z"]"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        let dom_result = crate::analyzer::StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("{e}"));

        let events = emit_events(&value);
        let mut acc = StreamingStatsAccumulator::default();
        acc.process_events(&events);
        let stream_result = acc.finalize();

        let path = WildcardPath::root().push_array_wildcard();
        let dom_h = dom_result
            .paths
            .get(&path)
            .map(|s| s.entropy)
            .unwrap_or(0.0);
        let stream_h = stream_result
            .paths
            .get(&path)
            .map(|s| s.entropy)
            .unwrap_or(0.0);

        assert!(
            (dom_h - stream_h).abs() < 1e-10,
            "dom_h={dom_h}, stream_h={stream_h}"
        );
    }

    // Test 6: Numeric sketch produces correct median/percentiles
    #[test]
    fn streaming_numeric_sketch_percentiles() {
        let values: Vec<i64> = (1..=100).collect();
        let json = serde_json::to_string(&values).unwrap_or_else(|e| panic!("{e}"));
        let value = parse_value(&json);

        let events = emit_events(&value);
        let mut acc = StreamingStatsAccumulator::default();
        acc.process_events(&events);
        let result = acc.finalize();

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).unwrap_or_else(|| panic!("missing"));
        let ns = stats
            .numeric_stats
            .as_ref()
            .unwrap_or_else(|| panic!("no numeric"));

        // DDSketch with alpha=0.01 should be within ~2% of true values
        assert!(
            (ns.median - 50.0).abs() / 50.0 < 0.05,
            "median={}",
            ns.median
        );
        assert!((ns.min - 1.0).abs() < 1e-10, "min={}", ns.min);
        assert!((ns.max - 100.0).abs() < 1e-10, "max={}", ns.max);
    }

    // Test 7: Top-k matches exact top-k for small documents
    #[test]
    fn streaming_top_k_matches_exact() {
        let json = r#"["a", "b", "a", "c", "a", "b", "d"]"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        let dom_result = crate::analyzer::StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("{e}"));

        let events = emit_events(&value);
        let mut acc = StreamingStatsAccumulator::default();
        acc.process_events(&events);
        let stream_result = acc.finalize();

        let path = WildcardPath::root().push_array_wildcard();
        let dom_top = &dom_result
            .paths
            .get(&path)
            .unwrap_or_else(|| panic!("m"))
            .top_values;
        let stream_top = &stream_result
            .paths
            .get(&path)
            .unwrap_or_else(|| panic!("m"))
            .top_values;

        // First entry (most frequent) should match
        assert_eq!(dom_top[0].0, stream_top[0].0);
        assert_eq!(dom_top[0].1, stream_top[0].1);
    }

    // Test 8: ValueTracker switches from Exact to Approximate at threshold
    #[test]
    fn value_tracker_switches_at_threshold() {
        let config = StreamingConfig {
            exact_threshold: 5,
            ..StreamingConfig::default()
        };
        let mut acc = StreamingStatsAccumulator::new(config);

        let path = WildcardPath::root().push_array_wildcard();

        // Add 4 distinct values - should stay exact
        for i in 0..4 {
            let event = JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::String(format!("val_{i}")),
            };
            acc.process_event(&event);
        }
        assert!(acc.is_exact(&path));

        // Add more distinct values to exceed threshold
        for i in 4..10 {
            let event = JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::String(format!("val_{i}")),
            };
            acc.process_event(&event);
        }
        assert!(!acc.is_exact(&path));
    }

    // Test 9: Merge produces correct combined statistics
    #[test]
    fn merge_combined_statistics() {
        let mut acc1 = StreamingStatsAccumulator::default();
        let mut acc2 = StreamingStatsAccumulator::default();

        let path = WildcardPath::root().push_array_wildcard();

        // acc1 gets "a" x3
        for _ in 0..3 {
            acc1.process_event(&JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::String("a".into()),
            });
        }

        // acc2 gets "a" x2, "b" x1
        for _ in 0..2 {
            acc2.process_event(&JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::String("a".into()),
            });
        }
        acc2.process_event(&JsonEvent::Value {
            path: path.clone(),
            value: ScalarValue::String("b".into()),
        });

        acc1.merge(acc2);
        let result = acc1.finalize();

        let stats = result.paths.get(&path).unwrap_or_else(|| panic!("missing"));
        assert_eq!(stats.total_count, 6); // 3 + 2 + 1
        assert_eq!(stats.cardinality, 2); // "a" and "b"
    }

    // Test 13: Custom DDSketch alpha works
    #[test]
    fn custom_ddsketch_alpha() {
        let config = StreamingConfig {
            ddsketch_alpha: 0.05,
            ..StreamingConfig::default()
        };
        let mut acc = StreamingStatsAccumulator::new(config);

        let path = WildcardPath::root().push_array_wildcard();
        for i in 1..=100 {
            acc.process_event(&JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::Integer(i),
            });
        }

        let result = acc.finalize();
        let stats = result.paths.get(&path).unwrap_or_else(|| panic!("missing"));
        let ns = stats
            .numeric_stats
            .as_ref()
            .unwrap_or_else(|| panic!("no numeric"));

        // With alpha=0.05, wider buckets but median should still be reasonable
        assert!(
            (ns.median - 50.0).abs() / 50.0 < 0.10,
            "median={}",
            ns.median
        );
    }

    // Test 14: Empty events produce empty result
    #[test]
    fn empty_events_empty_result() {
        let acc = StreamingStatsAccumulator::default();
        let result = acc.finalize();
        assert!(result.paths.is_empty());
    }

    // Test 15: Single event produces correct single-path result
    #[test]
    fn single_event_single_path() {
        let mut acc = StreamingStatsAccumulator::default();
        let path = WildcardPath::root().push_key("x");
        acc.process_event(&JsonEvent::Value {
            path: path.clone(),
            value: ScalarValue::Integer(42),
        });

        let result = acc.finalize();
        assert_eq!(result.paths.len(), 1);

        let stats = result.paths.get(&path).unwrap_or_else(|| panic!("missing"));
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.cardinality, 1);
        assert!((stats.entropy - 0.0).abs() < 1e-10);
    }
}
