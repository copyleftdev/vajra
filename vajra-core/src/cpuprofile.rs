//! Parser for V8 `.cpuprofile` files.
//!
//! Converts a V8 CPU profile (JSON with `nodes`, `samples`, and `timeDeltas`)
//! into a JSON array of function-level aggregates sorted by sample count
//! descending.

use std::collections::HashMap;

use vajra_types::VajraError;

/// A single call frame from a V8 cpuprofile node.
#[derive(Debug)]
struct CallFrame {
    function_name: String,
    url: String,
}

/// Classify a URL into a source category.
///
/// - URLs containing `"node_modules"` are `"dependency"`.
/// - Empty URLs or those starting with `"node:"` are `"runtime"`.
/// - Everything else is `"app"`.
fn classify_source(url: &str) -> &'static str {
    if url.is_empty() || url.starts_with("node:") {
        "runtime"
    } else if url.contains("node_modules") {
        "dependency"
    } else {
        "app"
    }
}

/// Parse a V8 `.cpuprofile` JSON string into a JSON array of function-level
/// aggregates.
///
/// Each element has the shape:
/// ```json
/// {"function": "name", "file": "url", "source": "app|dependency|runtime",
///  "samples": 42, "pct": 3.7}
/// ```
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if the input is not valid cpuprofile JSON.
pub fn parse_cpuprofile(input: &str) -> Result<String, VajraError> {
    let root: serde_json::Value = serde_json::from_str(input).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("cpuprofile JSON parse error: {e}"),
        source_path: None,
    })?;

    // Extract nodes array
    let nodes = root
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| VajraError::Parse {
            byte_offset: 0,
            message: "cpuprofile missing 'nodes' array".to_string(),
            source_path: None,
        })?;

    // Build node lookup: id -> CallFrame
    let mut node_map: HashMap<u64, CallFrame> = HashMap::with_capacity(nodes.len());
    for node in nodes {
        let id = node
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| VajraError::Parse {
                byte_offset: 0,
                message: "cpuprofile node missing 'id'".to_string(),
                source_path: None,
            })?;

        let call_frame = node.get("callFrame").ok_or_else(|| VajraError::Parse {
            byte_offset: 0,
            message: format!("cpuprofile node {id} missing 'callFrame'"),
            source_path: None,
        })?;

        let function_name = call_frame
            .get("functionName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(anonymous)")
            .to_string();

        let url = call_frame
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();

        node_map.insert(id, CallFrame { function_name, url });
    }

    // Extract samples array
    let samples = root
        .get("samples")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| VajraError::Parse {
            byte_offset: 0,
            message: "cpuprofile missing 'samples' array".to_string(),
            source_path: None,
        })?;

    // Count samples per node
    let mut counts: HashMap<u64, u64> = HashMap::new();
    for sample in samples {
        let id = sample.as_u64().ok_or_else(|| VajraError::Parse {
            byte_offset: 0,
            message: "cpuprofile sample is not a number".to_string(),
            source_path: None,
        })?;
        *counts.entry(id).or_insert(0) += 1;
    }

    let total_samples = samples.len() as f64;
    if total_samples == 0.0 {
        return Ok("[]".to_string());
    }

    // Build aggregates, sort by count descending then by id for determinism
    let mut aggregates: Vec<(u64, u64)> = counts.into_iter().collect();
    aggregates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut result: Vec<serde_json::Value> = Vec::with_capacity(aggregates.len());
    for (id, count) in aggregates {
        let frame = node_map.get(&id);
        let function_name = frame.map_or("(unknown)", |f| f.function_name.as_str());
        let file = frame.map_or("", |f| f.url.as_str());
        let source = classify_source(file);
        let pct = (count as f64 / total_samples) * 100.0;
        let pct_rounded = (pct * 10.0).round() / 10.0;

        result.push(serde_json::json!({
            "function": function_name,
            "file": file,
            "source": source,
            "samples": count,
            "pct": pct_rounded,
        }));
    }

    serde_json::to_string(&result).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("cpuprofile JSON serialization error: {e}"),
        source_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cpuprofile() -> &'static str {
        r#"{
  "nodes": [
    {"id": 1, "callFrame": {"functionName": "(root)", "url": ""}},
    {"id": 2, "callFrame": {"functionName": "runLoop", "url": "fiberRuntime.js"}},
    {"id": 3, "callFrame": {"functionName": "processRequest", "url": "node_modules/express/lib/router.js"}},
    {"id": 4, "callFrame": {"functionName": "internalBinding", "url": "node:internal/modules/cjs/loader"}}
  ],
  "samples": [1, 2, 2, 3, 2, 4, 1, 2, 3, 2],
  "timeDeltas": [100, 200, 150, 300, 250, 100, 50, 200, 300, 100]
}"#
    }

    #[test]
    fn parse_basic_cpuprofile() {
        let result = parse_cpuprofile(sample_cpuprofile());
        assert!(result.is_ok(), "parse failed: {result:?}");
        let json_str = result.unwrap_or_default();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["function"], "runLoop");
        assert_eq!(arr[0]["samples"], 5);
        assert_eq!(arr[0]["source"], "app");
        assert_eq!(arr[0]["pct"], 50.0);
    }

    #[test]
    fn source_classification_correct() {
        let result = parse_cpuprofile(sample_cpuprofile());
        let json_str = result.unwrap_or_default();
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
        for entry in &arr {
            match entry["function"].as_str().unwrap_or("") {
                "(root)" => assert_eq!(entry["source"], "runtime"),
                "runLoop" => assert_eq!(entry["source"], "app"),
                "processRequest" => assert_eq!(entry["source"], "dependency"),
                "internalBinding" => assert_eq!(entry["source"], "runtime"),
                _ => {}
            }
        }
    }

    #[test]
    fn classify_source_categories() {
        assert_eq!(classify_source(""), "runtime");
        assert_eq!(classify_source("node:fs"), "runtime");
        assert_eq!(
            classify_source("node_modules/express/index.js"),
            "dependency"
        );
        assert_eq!(classify_source("src/app.js"), "app");
    }

    #[test]
    fn missing_nodes_returns_error() {
        let input = r#"{"samples": [1], "timeDeltas": [100]}"#;
        let result = parse_cpuprofile(input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("nodes"), "error should mention nodes: {msg}");
    }

    #[test]
    fn missing_samples_returns_error() {
        let input = r#"{"nodes": [{"id":1,"callFrame":{"functionName":"f","url":""}}]}"#;
        let result = parse_cpuprofile(input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("samples"),
            "error should mention samples: {msg}"
        );
    }

    #[test]
    fn invalid_json_returns_error() {
        let result = parse_cpuprofile("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn empty_samples_returns_empty_array() {
        let input = r#"{"nodes": [{"id":1,"callFrame":{"functionName":"f","url":""}}], "samples": [], "timeDeltas": []}"#;
        let result = parse_cpuprofile(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap_or_default(), "[]");
    }

    #[test]
    fn deterministic_output() {
        let first = parse_cpuprofile(sample_cpuprofile()).unwrap_or_default();
        for _ in 0..10 {
            let again = parse_cpuprofile(sample_cpuprofile()).unwrap_or_default();
            assert_eq!(first, again);
        }
    }
}
