//! Streaming fingerprint accumulator.
//!
//! Tracks the path set and per-path dominant types incrementally from
//! [`JsonEvent`]s.  Produces path-set and typed-path fingerprints
//! equivalent to those from the DOM-based [`FingerprintAnalyzer`],
//! without requiring the full document tree.
//!
//! The Merkle shape fingerprint is omitted in streaming mode because
//! it requires bottom-up tree traversal.

use std::collections::{BTreeMap, BTreeSet};

use vajra_core::event::JsonEvent;
use vajra_types::json_type::JsonType;
use vajra_types::path::WildcardPath;

/// Result of streaming fingerprint analysis.
#[derive(Debug, Clone)]
pub struct StreamingFingerprintResult {
    /// Hash of the sorted set of distinct wildcard paths.
    pub path_set: [u8; 32],
    /// Hash of sorted (path, dominant_type) pairs.
    pub typed_path: [u8; 32],
}

/// Streaming fingerprint accumulator.
///
/// Tracks path set incrementally for path-set and typed-path fingerprints.
pub struct StreamingFingerprintAccumulator {
    /// All distinct paths seen (bounded by distinct path count, not data volume).
    paths: BTreeSet<WildcardPath>,
    /// Per-path dominant type tracking.
    path_types: BTreeMap<WildcardPath, [u64; 7]>,
}

impl StreamingFingerprintAccumulator {
    /// Create a new empty accumulator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            paths: BTreeSet::new(),
            path_types: BTreeMap::new(),
        }
    }

    /// Process a single event.
    pub fn process_event(&mut self, event: &JsonEvent) {
        let path = event.path();
        self.paths.insert(path.clone());

        let type_counts = self
            .path_types
            .entry(path.clone())
            .or_insert([0u64; 7]);

        match event {
            JsonEvent::ObjectStart { .. } => {
                type_counts[JsonType::Object.index()] += 1;
            }
            JsonEvent::ArrayStart { .. } => {
                type_counts[JsonType::Array.index()] += 1;
            }
            JsonEvent::Value { value, .. } => {
                type_counts[value.type_index()] += 1;
            }
            // End events don't change type counts
            JsonEvent::ObjectEnd { .. } | JsonEvent::ArrayEnd { .. } => {}
        }
    }

    /// Process a batch of events.
    pub fn process_events(&mut self, events: &[JsonEvent]) {
        for event in events {
            self.process_event(event);
        }
    }

    /// Produce the final fingerprint result, consuming the accumulator.
    #[must_use]
    pub fn finalize(self) -> StreamingFingerprintResult {
        let path_set = self.compute_path_set_fingerprint();
        let typed_path = self.compute_typed_path_fingerprint();

        StreamingFingerprintResult {
            path_set,
            typed_path,
        }
    }

    /// Compute path-set fingerprint: BLAKE3 hash of sorted distinct paths.
    fn compute_path_set_fingerprint(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        for path in &self.paths {
            hasher.update(path.as_str().as_bytes());
            hasher.update(b"\x00");
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute typed-path fingerprint: BLAKE3 hash of sorted (path, dominant_type) pairs.
    fn compute_typed_path_fingerprint(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        for path in &self.paths {
            hasher.update(path.as_str().as_bytes());
            hasher.update(b":");

            let type_name = self
                .path_types
                .get(path)
                .and_then(|counts| dominant_type(counts))
                .map_or("none", json_type_name);

            hasher.update(type_name.as_bytes());
            hasher.update(b"\x00");
        }
        *hasher.finalize().as_bytes()
    }
}

impl Default for StreamingFingerprintAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the dominant type from a type count array.
fn dominant_type(counts: &[u64; 7]) -> Option<JsonType> {
    let types = [
        JsonType::Null,
        JsonType::Bool,
        JsonType::Integer,
        JsonType::Float,
        JsonType::String,
        JsonType::Array,
        JsonType::Object,
    ];
    types
        .iter()
        .max_by_key(|t| counts[t.index()])
        .filter(|t| counts[t.index()] > 0)
        .copied()
}

/// Stable string name for a `JsonType`, matching `typed_path.rs`.
const fn json_type_name(t: JsonType) -> &'static str {
    match t {
        JsonType::Null => "null",
        JsonType::Bool => "boolean",
        JsonType::Integer => "integer",
        JsonType::Float => "float",
        JsonType::String => "string",
        JsonType::Array => "array",
        JsonType::Object => "object",
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

    // Test 10: Path set matches DOM path set fingerprint
    #[test]
    fn streaming_path_set_matches_dom() {
        let json = r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        // DOM fingerprint
        let dom_fp = crate::path_set::path_set_fingerprint(&doc);

        // Streaming fingerprint
        let events = emit_events(&value);
        let mut acc = StreamingFingerprintAccumulator::new();
        acc.process_events(&events);
        let stream_fp = acc.finalize();

        assert_eq!(
            dom_fp, stream_fp.path_set,
            "path set fingerprints should match"
        );
    }

    // Test 11: Typed path matches DOM typed path fingerprint
    #[test]
    fn streaming_typed_path_matches_dom() {
        let json = r#"{"a": 1, "b": "hello", "c": [true, false]}"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        // DOM fingerprint
        let dom_fp = crate::typed_path::typed_path_fingerprint(&doc);

        // Streaming fingerprint
        let events = emit_events(&value);
        let mut acc = StreamingFingerprintAccumulator::new();
        acc.process_events(&events);
        let stream_fp = acc.finalize();

        assert_eq!(
            dom_fp, stream_fp.typed_path,
            "typed path fingerprints should match"
        );
    }

    // Test 12 (part 1): Full streaming pipeline produces equivalent results
    #[test]
    fn full_pipeline_fingerprints_match_dom() {
        let json = r#"{"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]}"#;
        let value = parse_value(json);
        let doc = parse_doc(json);

        // DOM fingerprints
        let dom_result = crate::analyzer::FingerprintAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("{e}"));

        // Streaming fingerprints
        let events = emit_events(&value);
        let mut acc = StreamingFingerprintAccumulator::new();
        acc.process_events(&events);
        let stream_result = acc.finalize();

        assert_eq!(dom_result.path_set, stream_result.path_set);
        assert_eq!(dom_result.typed_path, stream_result.typed_path);
    }

    #[test]
    fn empty_events_produce_fingerprints() {
        let acc = StreamingFingerprintAccumulator::new();
        let result = acc.finalize();
        // Both should be valid 32-byte hashes (of empty input)
        assert_eq!(result.path_set.len(), 32);
        assert_eq!(result.typed_path.len(), 32);
    }

    #[test]
    fn path_set_deterministic() {
        let json = r#"{"x": 1, "y": 2}"#;
        let value = parse_value(json);
        let events = emit_events(&value);

        let mut acc1 = StreamingFingerprintAccumulator::new();
        acc1.process_events(&events);
        let r1 = acc1.finalize();

        let mut acc2 = StreamingFingerprintAccumulator::new();
        acc2.process_events(&events);
        let r2 = acc2.finalize();

        assert_eq!(r1.path_set, r2.path_set);
        assert_eq!(r1.typed_path, r2.typed_path);
    }
}
