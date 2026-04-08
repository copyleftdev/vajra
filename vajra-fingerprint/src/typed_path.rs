//! Typed path fingerprinting: BLAKE3 hash of sorted (path, `dominant_type`) pairs.

use vajra_types::{Document, JsonType};

/// Compute a BLAKE3 hash of sorted `(wildcard_path, dominant_type)` pairs.
///
/// For each path in the trie we look up the dominant JSON type observed at
/// that position and include both the path string and the type name in the
/// hash input. Paths with no observations (count == 0) are included with an
/// empty type tag so the fingerprint still captures their presence.
#[must_use]
pub fn typed_path_fingerprint(doc: &Document) -> [u8; 32] {
    let trie = doc.trie();
    let paths = trie.all_paths();
    let mut hasher = blake3::Hasher::new();

    for path in &paths {
        hasher.update(path.as_str().as_bytes());
        hasher.update(b":");

        let type_name = trie
            .get(path)
            .and_then(|node| node.metadata.dominant_type())
            .map_or("none", json_type_name);

        hasher.update(type_name.as_bytes());
        hasher.update(b"\x00");
    }

    *hasher.finalize().as_bytes()
}

/// Stable string name for a `JsonType`.
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
    use vajra_core::parse::parse_str;

    #[test]
    fn deterministic() {
        let input = r#"{"a": 1, "b": "hello"}"#;
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let h1 = typed_path_fingerprint(&doc);
        let h2 = typed_path_fingerprint(&doc);
        assert_eq!(h1, h2);
    }

    #[test]
    fn changes_when_types_change() {
        // Same keys, different value types.
        let doc_a =
            parse_str(r#"{"a": 1, "b": 2}"#).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let doc_b = parse_str(r#"{"a": "one", "b": "two"}"#)
            .unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_ne!(
            typed_path_fingerprint(&doc_a),
            typed_path_fingerprint(&doc_b)
        );
    }

    #[test]
    fn same_types_same_hash() {
        // Same structure and types, different values.
        let doc_a =
            parse_str(r#"{"a": 1, "b": 2}"#).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let doc_b =
            parse_str(r#"{"a": 99, "b": 42}"#).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(
            typed_path_fingerprint(&doc_a),
            typed_path_fingerprint(&doc_b)
        );
    }

    #[test]
    fn hash_is_32_bytes() {
        let doc = parse_str(r#"{"x": 1}"#).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(typed_path_fingerprint(&doc).len(), 32);
    }
}
