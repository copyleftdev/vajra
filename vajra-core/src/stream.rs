//! DOM-to-event converter and adaptive loader.
//!
//! Walks an existing `serde_json::Value` and emits [`JsonEvent`]s,
//! enabling streaming analyzers to be tested against DOM-parsed data.
//! A true SAX parser can be swapped in later without changing analysis code.

use std::path::Path;

use vajra_types::path::WildcardPath;
use vajra_types::VajraError;

use crate::event::{JsonEvent, ScalarValue};

/// Walk a parsed JSON value and emit events via DFS traversal.
///
/// Events are emitted in document order: container starts, then children,
/// then container ends.  All array indices are collapsed to wildcards.
#[must_use]
pub fn emit_events(value: &serde_json::Value) -> Vec<JsonEvent> {
    let mut events = Vec::new();
    let root = WildcardPath::root();
    walk(value, &root, &mut events);
    events
}

/// Recursively walk the JSON value tree and append events.
fn walk(value: &serde_json::Value, path: &WildcardPath, events: &mut Vec<JsonEvent>) {
    match value {
        serde_json::Value::Object(map) => {
            events.push(JsonEvent::ObjectStart { path: path.clone() });
            for (key, child) in map {
                let child_path = path.push_key(key);
                walk(child, &child_path, events);
            }
            events.push(JsonEvent::ObjectEnd { path: path.clone() });
        }
        serde_json::Value::Array(arr) => {
            events.push(JsonEvent::ArrayStart { path: path.clone() });
            let child_path = path.push_array_wildcard();
            for child in arr {
                walk(child, &child_path, events);
            }
            events.push(JsonEvent::ArrayEnd { path: path.clone() });
        }
        serde_json::Value::Null => {
            events.push(JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::Null,
            });
        }
        serde_json::Value::Bool(b) => {
            events.push(JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::Bool(*b),
            });
        }
        serde_json::Value::Number(n) => {
            let scalar = if let Some(i) = n.as_i64() {
                ScalarValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ScalarValue::Float(f)
            } else {
                // u64 that doesn't fit in i64 - represent as float
                ScalarValue::Float(n.as_f64().unwrap_or(0.0))
            };
            events.push(JsonEvent::Value {
                path: path.clone(),
                value: scalar,
            });
        }
        serde_json::Value::String(s) => {
            events.push(JsonEvent::Value {
                path: path.clone(),
                value: ScalarValue::String(s.clone()),
            });
        }
    }
}

/// Result from adaptive loading.
pub enum AnalysisMode {
    /// Standard DOM mode - document fits comfortably in memory.
    Dom(vajra_types::Document),
    /// Streaming mode - events emitted from DOM (or later, SAX parser).
    Streaming(Vec<JsonEvent>, vajra_types::DocumentMetadata),
}

/// Load a document, choosing DOM or streaming mode based on file size.
///
/// If the file is larger than `threshold_bytes`, emit events (currently
/// still from DOM, but the streaming interface is established). Otherwise
/// parse to a full `Document`.
///
/// # Errors
///
/// Returns `VajraError::Io` if the file cannot be read.
/// Returns `VajraError::Parse` if the content is not valid JSON.
pub fn load_adaptive(path: &Path, threshold_bytes: u64) -> Result<AnalysisMode, VajraError> {
    let file_size = std::fs::metadata(path)
        .map_err(|e| VajraError::Io {
            path: path.to_path_buf(),
            source: e,
        })?
        .len();

    if file_size <= threshold_bytes {
        let doc = crate::parse::parse_file(path)?;
        Ok(AnalysisMode::Dom(doc))
    } else {
        // For now, still parse DOM but convert to events.
        // A future SAX parser will replace this path.
        let content = std::fs::read_to_string(path).map_err(|e| VajraError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| VajraError::Parse {
                byte_offset: e.column(),
                message: e.to_string(),
                source_path: Some(path.to_path_buf()),
            })?;

        let events = emit_events(&value);

        let metadata = vajra_types::DocumentMetadata {
            total_nodes: count_nodes(&value),
            max_depth: compute_max_depth(&value, 0),
            distinct_paths: 0, // Will be computed from streaming accumulators
            raw_size_bytes: file_size,
        };

        Ok(AnalysisMode::Streaming(events, metadata))
    }
}

/// Count total nodes in a JSON value tree.
fn count_nodes(value: &serde_json::Value) -> u64 {
    match value {
        serde_json::Value::Object(map) => 1 + map.values().map(count_nodes).sum::<u64>(),
        serde_json::Value::Array(arr) => 1 + arr.iter().map(count_nodes).sum::<u64>(),
        _ => 1,
    }
}

/// Compute maximum nesting depth.
fn compute_max_depth(value: &serde_json::Value, current: u32) -> u32 {
    match value {
        serde_json::Value::Object(map) => map
            .values()
            .map(|v| compute_max_depth(v, current + 1))
            .max()
            .unwrap_or(current),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|v| compute_max_depth(v, current + 1))
            .max()
            .unwrap_or(current),
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_value(json: &str) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn emit_simple_object_correct_events() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"{"a": 1, "b": "hello"}"#)?;
        let events = emit_events(&value);

        // ObjectStart($), Value($.a), Value($.b), ObjectEnd($)
        assert!(events.len() >= 4);

        // First event should be ObjectStart at root
        assert!(
            matches!(&events[0], JsonEvent::ObjectStart { path } if path == &WildcardPath::root())
        );

        // Last event should be ObjectEnd at root
        assert!(
            matches!(events.last(), Some(JsonEvent::ObjectEnd { path }) if path == &WildcardPath::root())
        );
        Ok(())
    }

    #[test]
    fn emit_nested_arrays_correct_wildcard_paths() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"[[1, 2], [3]]"#)?;
        let events = emit_events(&value);

        // Check that inner elements have $[*][*] paths
        let inner_values: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, JsonEvent::Value { .. }))
            .collect();

        let expected_path = WildcardPath::root()
            .push_array_wildcard()
            .push_array_wildcard();

        for ev in &inner_values {
            if let JsonEvent::Value { path, .. } = ev {
                assert_eq!(path, &expected_path);
            }
        }
        assert_eq!(inner_values.len(), 3);
        Ok(())
    }

    #[test]
    fn emit_event_count_matches_node_count() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#)?;
        let events = emit_events(&value);

        // Count Value events + container start events (which also represent nodes)
        let node_events: usize = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    JsonEvent::Value { .. }
                        | JsonEvent::ObjectStart { .. }
                        | JsonEvent::ArrayStart { .. }
                )
            })
            .count();

        // Root object + a:1 + b:[] + 2 + 3 + c:{} + d:true = 7
        // But we count ObjectStart/ArrayStart as container nodes, plus scalar Values
        // ObjectStart($) + Value($.a) + ArrayStart($.b) + Value($.b[*]) x2 + ObjectStart($.c) + Value($.c.d)
        assert_eq!(node_events, 7);
        Ok(())
    }

    #[test]
    fn emit_empty_object() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("{}")?;
        let events = emit_events(&value);
        assert_eq!(events.len(), 2); // ObjectStart + ObjectEnd
        Ok(())
    }

    #[test]
    fn emit_empty_array() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("[]")?;
        let events = emit_events(&value);
        assert_eq!(events.len(), 2); // ArrayStart + ArrayEnd
        Ok(())
    }

    #[test]
    fn emit_scalar_root() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("42")?;
        let events = emit_events(&value);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], JsonEvent::Value {
            path,
            value: ScalarValue::Integer(42)
        } if path == &WildcardPath::root()));
        Ok(())
    }

    #[test]
    fn emit_null_root() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("null")?;
        let events = emit_events(&value);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            JsonEvent::Value {
                value: ScalarValue::Null,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn emit_bool_values() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"{"x": true, "y": false}"#)?;
        let events = emit_events(&value);

        let values: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let JsonEvent::Value { value, .. } = e {
                    Some(value)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(values.len(), 2);
        Ok(())
    }

    #[test]
    fn emit_float_value() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value("2.75")?;
        let events = emit_events(&value);
        assert_eq!(events.len(), 1);
        let JsonEvent::Value {
            value: ScalarValue::Float(f),
            ..
        } = &events[0]
        else {
            return Err("expected Float event".into());
        };
        assert!((*f - 2.75).abs() < 1e-10);
        Ok(())
    }

    #[test]
    fn count_nodes_correct() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#)?;
        assert_eq!(count_nodes(&value), 7);
        Ok(())
    }

    #[test]
    fn compute_max_depth_correct() -> Result<(), Box<dyn std::error::Error>> {
        let value = parse_value(r#"{"a": {"b": {"c": 1}}}"#)?;
        assert_eq!(compute_max_depth(&value, 0), 3);
        Ok(())
    }
}
