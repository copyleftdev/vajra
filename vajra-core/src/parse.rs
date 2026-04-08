//! JSON parsing with structural path trie construction.
//!
//! Parses a JSON string or file into a [`Document`] that includes the raw
//! `serde_json::Value` tree, a [`PathTrie`] built by DFS traversal, and
//! [`DocumentMetadata`] with aggregate statistics.

use std::path::Path;

use vajra_types::{Document, DocumentMetadata, JsonType, PathTrie, VajraError, WildcardPath};

/// Default maximum nesting depth to prevent stack overflow on adversarial input.
const MAX_DEPTH: u32 = 256;

/// Parse a JSON string, build the structural path trie, and return a [`Document`].
///
/// # Errors
///
/// Returns [`VajraError::Parse`] if the input is not valid JSON.
/// Returns [`VajraError::LimitExceeded`] if nesting depth exceeds 256.
pub fn parse_str(input: &str) -> Result<Document, VajraError> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| VajraError::Parse {
        byte_offset: e.column(),
        message: e.to_string(),
        source_path: None,
    })?;

    let mut trie = PathTrie::new();
    let mut metadata = DocumentMetadata {
        total_nodes: 0,
        max_depth: 0,
        distinct_paths: 0,
        raw_size_bytes: input.len() as u64,
    };

    let root_path = WildcardPath::root();
    walk(&value, &root_path, &mut trie, 0, &mut metadata)?;

    #[allow(clippy::cast_possible_truncation)]
    {
        metadata.distinct_paths = trie.path_count() as u32;
    }

    Ok(Document::new(value, trie, metadata))
}

/// Read a file and parse it as JSON.
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the file cannot be read.
/// Returns [`VajraError::Parse`] if the file content is not valid JSON.
/// Returns [`VajraError::LimitExceeded`] if nesting depth exceeds 256.
pub fn parse_file(path: &Path) -> Result<Document, VajraError> {
    let content = std::fs::read_to_string(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    parse_str(&content)
}

/// DFS walk over the JSON value tree, populating the path trie and metadata.
///
/// Each node (including the root) is observed in the trie. Array elements
/// have their concrete indices replaced with `ArrayWildcard` in the path.
fn walk(
    value: &serde_json::Value,
    current_path: &WildcardPath,
    trie: &mut PathTrie,
    depth: u32,
    metadata: &mut DocumentMetadata,
) -> Result<(), VajraError> {
    if depth > MAX_DEPTH {
        return Err(VajraError::LimitExceeded {
            message: format!(
                "nesting depth {depth} exceeds maximum of {MAX_DEPTH} at path {current_path}"
            ),
        });
    }

    // Count this node
    metadata.total_nodes += 1;
    if depth > metadata.max_depth {
        metadata.max_depth = depth;
    }

    // Observe this node in the trie
    let json_type = JsonType::from_value(value);
    let trie_node = trie.get_or_create(current_path);
    trie_node.metadata.observe(json_type, depth);

    // Recurse into children
    match value {
        serde_json::Value::Object(map) => {
            for (key, child_value) in map {
                let child_path = current_path.push_key(key);
                walk(child_value, &child_path, trie, depth + 1, metadata)?;
            }
        }
        serde_json::Value::Array(arr) => {
            // All array elements share the same wildcard path
            let child_path = current_path.push_array_wildcard();
            for child_value in arr {
                walk(child_value, &child_path, trie, depth + 1, metadata)?;
            }
        }
        // Scalars: nothing more to recurse into
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_object() {
        let input = r#"{"name": "Alice", "age": 30}"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 3); // root object + 2 scalars
        assert_eq!(doc.metadata().max_depth, 1);
        assert_eq!(doc.metadata().distinct_paths, 3); // $, $.name, $.age
    }

    #[test]
    fn parse_nested_object() {
        let input = r#"{"a": {"b": {"c": 1}}}"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 4); // $, $.a, $.a.b, $.a.b.c
        assert_eq!(doc.metadata().max_depth, 3);
        assert_eq!(doc.metadata().distinct_paths, 4); // $, $.a, $.a.b, $.a.b.c
    }

    #[test]
    fn parse_array() {
        let input = r#"[1, 2, 3]"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        // root array + 3 elements
        assert_eq!(doc.metadata().total_nodes, 4);
        assert_eq!(doc.metadata().max_depth, 1);
        // $, $[*] (all three elements share this path)
        assert_eq!(doc.metadata().distinct_paths, 2);
    }

    #[test]
    fn parse_mixed_types_array() {
        let input = r#"[1, "two", true, null]"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 5); // root + 4 elements
        assert_eq!(doc.metadata().distinct_paths, 2); // $, $[*]

        // The $[*] trie node should have count=4 with mixed types
        let wildcard_path = WildcardPath::root().push_array_wildcard();
        let node = doc
            .trie()
            .get(&wildcard_path)
            .unwrap_or_else(|| panic!("node not found"));
        assert_eq!(node.metadata.count, 4);
    }

    #[test]
    fn parse_empty_object() {
        let input = "{}";
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
        assert_eq!(doc.metadata().max_depth, 0);
        assert_eq!(doc.metadata().distinct_paths, 1); // just root
    }

    #[test]
    fn parse_empty_array() {
        let input = "[]";
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
        assert_eq!(doc.metadata().max_depth, 0);
        assert_eq!(doc.metadata().distinct_paths, 1);
    }

    #[test]
    fn parse_null_root() {
        let input = "null";
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
        assert_eq!(doc.metadata().max_depth, 0);
        assert_eq!(doc.metadata().distinct_paths, 1);

        // Root trie node should be observed as Null
        let root = doc.trie().root();
        assert_eq!(root.metadata.count, 1);
        assert_eq!(root.metadata.null_count, 1);
    }

    #[test]
    fn parse_boolean_root() {
        let input = "true";
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
    }

    #[test]
    fn parse_string_root() {
        let input = r#""hello world""#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
    }

    #[test]
    fn parse_number_root() {
        let input = "42";
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().total_nodes, 1);
    }

    #[test]
    fn trie_records_all_paths_with_wildcards() {
        let input = r#"{
            "users": [
                {"name": "Alice", "scores": [100, 95]},
                {"name": "Bob", "scores": [88]}
            ]
        }"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));

        let paths: Vec<String> = doc
            .trie()
            .all_paths()
            .iter()
            .map(|p| p.to_string())
            .collect();

        // Expected paths (sorted by BTreeMap ordering):
        assert!(paths.contains(&"$".to_string()));
        assert!(paths.contains(&"$.users".to_string()));
        assert!(paths.contains(&"$.users[*]".to_string()));
        assert!(paths.contains(&"$.users[*].name".to_string()));
        assert!(paths.contains(&"$.users[*].scores".to_string()));
        assert!(paths.contains(&"$.users[*].scores[*]".to_string()));

        // Exactly 6 distinct paths
        assert_eq!(paths.len(), 6);
        assert_eq!(doc.metadata().distinct_paths, 6);
    }

    #[test]
    fn array_indices_collapsed_to_wildcard() {
        // Two array elements with the same key should produce one wildcard path
        let input = r#"[{"id": 1}, {"id": 2}, {"id": 3}]"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));

        // Paths: $, $[*], $[*].id
        assert_eq!(doc.metadata().distinct_paths, 3);

        // $[*].id should have count=3 (one per array element)
        let id_path = WildcardPath::root().push_array_wildcard().push_key("id");
        let node = doc
            .trie()
            .get(&id_path)
            .unwrap_or_else(|| panic!("node not found"));
        assert_eq!(node.metadata.count, 3);
    }

    #[test]
    fn metadata_counts_correct_complex() {
        let input = r#"{
            "a": 1,
            "b": [2, 3],
            "c": {"d": true, "e": null}
        }"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));

        // Nodes: root{}, a:1, b:[], 2, 3, c:{}, d:true, e:null = 8
        assert_eq!(doc.metadata().total_nodes, 8);
        // max_depth: $.c.d or $.c.e = 2; $.b[*] = 2 -> max is 2
        assert_eq!(doc.metadata().max_depth, 2);
        // Paths: $, $.a, $.b, $.b[*], $.c, $.c.d, $.c.e = 7
        assert_eq!(doc.metadata().distinct_paths, 7);
    }

    #[test]
    fn root_node_observed() {
        let input = r#"{"key": "value"}"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));

        let root = doc.trie().root();
        assert_eq!(root.metadata.count, 1);
        // Root is an object
        assert_eq!(root.metadata.type_counts[JsonType::Object.index()], 1);
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let result = parse_str("{invalid json}");
        assert!(result.is_err());
        match result {
            Err(VajraError::Parse { .. }) => {} // expected
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn raw_size_bytes_recorded() {
        let input = r#"{"a": 1}"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().raw_size_bytes, input.len() as u64);
    }

    #[test]
    fn parse_deeply_nested_within_limit() {
        // serde_json has a default recursion limit of 128, so we test at 127 levels
        // (127 nested objects + 1 innermost integer = depth 127).
        // Our walk function allows up to 256 which is beyond serde_json's limit.
        let depth = 127;
        let mut json = String::new();
        for _ in 0..depth {
            json.push_str(r#"{"a":"#);
        }
        json.push('1');
        for _ in 0..depth {
            json.push('}');
        }
        let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(doc.metadata().max_depth, depth as u32);
    }

    #[test]
    fn parse_file_nonexistent() {
        let result = parse_file(Path::new("/nonexistent/path/file.json"));
        assert!(result.is_err());
        match result {
            Err(VajraError::Io { .. }) => {} // expected
            other => panic!("expected Io error, got {other:?}"),
        }
    }
}
