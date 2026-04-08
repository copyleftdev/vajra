//! Two-pass streaming analysis pipeline.
//!
//! Pass 1: Emit events from the DOM (or future SAX parser), run streaming
//!         stats and fingerprint accumulators.
//! Pass 2 (optional): For DOM-available data, run anomaly detection using
//!         the statistics from pass 1.
//!
//! This module re-exports the pipeline entry point. The actual accumulator
//! implementations live in `vajra-stats` and `vajra-fingerprint`.

use vajra_types::trie::PathTrie;
use vajra_types::DocumentMetadata;

use crate::event::JsonEvent;
use crate::stream::emit_events;

/// Build a [`PathTrie`] from a sequence of events.
///
/// This mirrors what the DOM parser does, but driven from the event stream
/// instead of walking `serde_json::Value` directly.
#[must_use]
pub fn trie_from_events(events: &[JsonEvent]) -> PathTrie {
    let mut trie = PathTrie::new();

    for event in events {
        match event {
            JsonEvent::ObjectStart { path } => {
                let node = trie.get_or_create(path);
                node.metadata
                    .observe(vajra_types::JsonType::Object, path.depth());
            }
            JsonEvent::ArrayStart { path } => {
                let node = trie.get_or_create(path);
                node.metadata
                    .observe(vajra_types::JsonType::Array, path.depth());
            }
            JsonEvent::Value { path, value } => {
                let json_type = match value {
                    crate::event::ScalarValue::Null => vajra_types::JsonType::Null,
                    crate::event::ScalarValue::Bool(_) => vajra_types::JsonType::Bool,
                    crate::event::ScalarValue::Integer(_) => vajra_types::JsonType::Integer,
                    crate::event::ScalarValue::Float(_) => vajra_types::JsonType::Float,
                    crate::event::ScalarValue::String(_) => vajra_types::JsonType::String,
                };
                let node = trie.get_or_create(path);
                node.metadata.observe(json_type, path.depth());
            }
            // End events don't create new trie observations
            JsonEvent::ObjectEnd { .. } | JsonEvent::ArrayEnd { .. } => {}
        }
    }

    trie
}

/// Compute [`DocumentMetadata`] from events and a trie.
#[must_use]
pub fn metadata_from_events(events: &[JsonEvent], trie: &PathTrie) -> DocumentMetadata {
    let mut total_nodes: u64 = 0;
    let mut max_depth: u32 = 0;

    for event in events {
        match event {
            JsonEvent::ObjectStart { path }
            | JsonEvent::ArrayStart { path }
            | JsonEvent::Value { path, .. } => {
                total_nodes += 1;
                let d = path.depth();
                if d > max_depth {
                    max_depth = d;
                }
            }
            JsonEvent::ObjectEnd { .. } | JsonEvent::ArrayEnd { .. } => {}
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    let distinct_paths = trie.path_count() as u32;

    DocumentMetadata {
        total_nodes,
        max_depth,
        distinct_paths,
        raw_size_bytes: 0, // Not available from events alone
    }
}

/// Run the streaming pipeline on a `serde_json::Value`, emitting events
/// and building a trie. Returns the events and trie for use by external
/// accumulators.
///
/// This is the core function that other modules call to get the event
/// stream and structural index.
#[must_use]
pub fn emit_and_index(value: &serde_json::Value) -> (Vec<JsonEvent>, PathTrie, DocumentMetadata) {
    let events = emit_events(value);
    let trie = trie_from_events(&events);
    let metadata = metadata_from_events(&events, &trie);
    (events, trie, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::path::WildcardPath;

    fn parse_value(json: &str) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn trie_from_events_matches_dom_trie() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#;
        let value = parse_value(json)?;

        // DOM path
        let doc = crate::parse::parse_str(json)?;
        let dom_paths: Vec<String> = doc
            .trie()
            .all_paths()
            .iter()
            .map(|p| p.to_string())
            .collect();

        // Event path
        let events = emit_events(&value);
        let event_trie = trie_from_events(&events);
        let event_paths: Vec<String> = event_trie
            .all_paths()
            .iter()
            .map(|p| p.to_string())
            .collect();

        assert_eq!(dom_paths, event_paths);
        Ok(())
    }

    #[test]
    fn metadata_from_events_matches_dom() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#;
        let value = parse_value(json)?;

        let doc = crate::parse::parse_str(json)?;

        let (_events, _trie, meta) = emit_and_index(&value);

        assert_eq!(meta.total_nodes, doc.metadata().total_nodes);
        assert_eq!(meta.max_depth, doc.metadata().max_depth);
        assert_eq!(meta.distinct_paths, doc.metadata().distinct_paths);
        Ok(())
    }

    #[test]
    fn emit_and_index_empty_object() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("{}")?;
        let (_events, trie, meta) = emit_and_index(&value);

        assert_eq!(meta.total_nodes, 1);
        assert_eq!(meta.distinct_paths, 1);
        assert_eq!(trie.path_count(), 1);
        Ok(())
    }

    #[test]
    fn emit_and_index_array_wildcards() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"[{"id": 1}, {"id": 2}]"#)?;
        let (_, trie, meta) = emit_and_index(&value);

        // $, $[*], $[*].id
        assert_eq!(meta.distinct_paths, 3);

        let id_path = WildcardPath::root().push_array_wildcard().push_key("id");
        let node = trie.get(&id_path);
        assert!(node.is_some());
        // count should be 2 (two array elements)
        assert_eq!(node.map(|n| n.metadata.count), Some(2));
        Ok(())
    }
}
