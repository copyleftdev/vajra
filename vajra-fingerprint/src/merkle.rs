//! Merkle subtree hashing of JSON value trees.
//!
//! Produces a bottom-up structural hash where:
//! - Leaf scalars are hashed by their `JsonType` name.
//! - Objects are hashed from sorted `(key, child_hash)` pairs.
//! - Arrays are hashed from concatenated child hashes.
//!
//! The resulting hash captures the *shape* of the tree independent of
//! scalar values, making it useful for structural comparison and motif
//! detection.

use std::collections::BTreeMap;

use vajra_types::JsonType;

/// Compute a bottom-up Merkle hash of the JSON value's structural shape.
///
/// Scalar values are hashed by type name only (not value), so two integers
/// at the same position always produce the same leaf hash. Object keys are
/// sorted before hashing to ensure determinism.
#[must_use]
pub fn merkle_fingerprint(value: &serde_json::Value) -> [u8; 32] {
    merkle_hash(value)
}

/// Compute the Merkle hash and simultaneously collect subtree hash
/// frequencies into a `BTreeMap`.
///
/// Every node in the tree contributes its hash to the frequency map.
/// Repeated structural motifs (e.g., identical array element shapes)
/// will appear with count > 1, enabling motif detection downstream.
#[must_use]
pub fn merkle_with_motifs(value: &serde_json::Value) -> ([u8; 32], BTreeMap<[u8; 32], u64>) {
    let mut freqs = BTreeMap::new();
    let hash = merkle_hash_with_freqs(value, &mut freqs);
    (hash, freqs)
}

/// Recursive Merkle hash without frequency tracking.
fn merkle_hash(value: &serde_json::Value) -> [u8; 32] {
    match value {
        serde_json::Value::Object(map) => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"object{");

            // Collect keys into a BTreeMap for deterministic sorted order,
            // regardless of the order in the serde_json::Map.
            let sorted: BTreeMap<&str, &serde_json::Value> =
                map.iter().map(|(k, v)| (k.as_str(), v)).collect();

            for (key, child) in &sorted {
                hasher.update(key.as_bytes());
                hasher.update(b":");
                let child_hash = merkle_hash(child);
                hasher.update(&child_hash);
            }
            hasher.update(b"}");
            *hasher.finalize().as_bytes()
        }
        serde_json::Value::Array(arr) => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"array[");
            for child in arr {
                let child_hash = merkle_hash(child);
                hasher.update(&child_hash);
            }
            hasher.update(b"]");
            *hasher.finalize().as_bytes()
        }
        // Scalars: hash the type name only (not the value).
        other => {
            let type_name = json_type_name(JsonType::from_value(other));
            *blake3::hash(type_name.as_bytes()).as_bytes()
        }
    }
}

/// Recursive Merkle hash with frequency tracking.
fn merkle_hash_with_freqs(
    value: &serde_json::Value,
    freqs: &mut BTreeMap<[u8; 32], u64>,
) -> [u8; 32] {
    let hash = match value {
        serde_json::Value::Object(map) => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"object{");

            let sorted: BTreeMap<&str, &serde_json::Value> =
                map.iter().map(|(k, v)| (k.as_str(), v)).collect();

            for (key, child) in &sorted {
                hasher.update(key.as_bytes());
                hasher.update(b":");
                let child_hash = merkle_hash_with_freqs(child, freqs);
                hasher.update(&child_hash);
            }
            hasher.update(b"}");
            *hasher.finalize().as_bytes()
        }
        serde_json::Value::Array(arr) => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"array[");
            for child in arr {
                let child_hash = merkle_hash_with_freqs(child, freqs);
                hasher.update(&child_hash);
            }
            hasher.update(b"]");
            *hasher.finalize().as_bytes()
        }
        other => {
            let type_name = json_type_name(JsonType::from_value(other));
            *blake3::hash(type_name.as_bytes()).as_bytes()
        }
    };

    *freqs.entry(hash).or_insert(0) += 1;
    hash
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

    #[test]
    fn identical_subtrees_same_hash() {
        let a = serde_json::json!({"x": 1, "y": "hello"});
        let b = serde_json::json!({"x": 1, "y": "hello"});
        assert_eq!(merkle_fingerprint(&a), merkle_fingerprint(&b));
    }

    #[test]
    fn different_structures_different_hash() {
        let a = serde_json::json!({"x": 1});
        let b = serde_json::json!({"x": 1, "y": 2});
        assert_ne!(merkle_fingerprint(&a), merkle_fingerprint(&b));
    }

    #[test]
    fn different_scalar_types_different_hash() {
        let a = serde_json::json!({"x": 1});
        let b = serde_json::json!({"x": "one"});
        assert_ne!(merkle_fingerprint(&a), merkle_fingerprint(&b));
    }

    #[test]
    fn same_scalar_type_different_values_same_hash() {
        // Merkle hash only considers type, not value.
        let a = serde_json::json!({"x": 42});
        let b = serde_json::json!({"x": 99});
        assert_eq!(merkle_fingerprint(&a), merkle_fingerprint(&b));
    }

    #[test]
    fn key_order_does_not_matter() {
        // serde_json::Map may preserve insertion order, but our merkle
        // hash sorts keys via BTreeMap.
        let a = serde_json::json!({"z": 1, "a": 2});
        let b = serde_json::json!({"a": 2, "z": 1});
        assert_eq!(merkle_fingerprint(&a), merkle_fingerprint(&b));
    }

    #[test]
    fn motif_counting_repeated_array_elements() {
        // Three identical objects in an array should produce the same
        // subtree hash with count > 1.
        let value = serde_json::json!([
            {"id": 1, "name": "a"},
            {"id": 2, "name": "b"},
            {"id": 3, "name": "c"}
        ]);
        let (_root_hash, freqs) = merkle_with_motifs(&value);

        // The three element objects have the same shape {id: integer, name: string},
        // so their hash should appear with count == 3.
        let element_hash = merkle_fingerprint(&serde_json::json!({"id": 1, "name": "a"}));
        let count = freqs.get(&element_hash).copied().unwrap_or(0);
        assert_eq!(count, 3, "identical-shape elements should have count 3");
    }

    #[test]
    fn motif_counting_includes_leaves() {
        let value = serde_json::json!([1, 2, 3]);
        let (_root_hash, freqs) = merkle_with_motifs(&value);

        // Integer leaf hash should appear 3 times.
        let int_hash = merkle_fingerprint(&serde_json::json!(1));
        let count = freqs.get(&int_hash).copied().unwrap_or(0);
        assert_eq!(count, 3);
    }

    #[test]
    fn hash_is_32_bytes() {
        let h = merkle_fingerprint(&serde_json::json!({"a": [1, 2, 3]}));
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn motif_root_hash_matches_fingerprint() {
        let value = serde_json::json!({"a": 1, "b": [true, false]});
        let fp = merkle_fingerprint(&value);
        let (root_hash, _freqs) = merkle_with_motifs(&value);
        assert_eq!(fp, root_hash);
    }
}
