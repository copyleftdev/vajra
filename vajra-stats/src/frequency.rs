//! Exact frequency counting for JSON scalar values per wildcard path.
//!
//! Walks a [`Document`]'s value tree, hashing each scalar value into a
//! deterministic string representation, and counts occurrences per
//! wildcard path. Produces cardinality, per-value counts, and top-k
//! most frequent values.

use std::collections::BTreeMap;

use vajra_types::path::WildcardPath;
use vajra_types::Document;

/// Holds per-path frequency counts of scalar values.
///
/// Keys are wildcard paths; values are maps from the deterministic string
/// representation of the scalar to its observation count.
#[derive(Debug, Clone)]
pub struct FrequencyCounter {
    /// Per-path value counts. Inner key is the canonical string of the scalar.
    counts: BTreeMap<WildcardPath, BTreeMap<String, u64>>,
}

impl FrequencyCounter {
    /// Create an empty frequency counter.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            counts: BTreeMap::new(),
        }
    }

    /// Walk the document and count all scalar values.
    pub fn count_document(&mut self, doc: &Document) {
        let root_path = WildcardPath::root();
        self.walk(doc.value(), &root_path);
    }

    /// Returns the per-path value-count maps.
    #[must_use]
    pub const fn counts(&self) -> &BTreeMap<WildcardPath, BTreeMap<String, u64>> {
        &self.counts
    }

    /// Returns the cardinality (number of distinct values) at a path.
    #[must_use]
    pub fn cardinality(&self, path: &WildcardPath) -> u64 {
        self.counts.get(path).map_or(0, |m| m.len() as u64)
    }

    /// Returns the raw count slice for a path (values of the inner map),
    /// suitable for entropy computation.
    #[must_use]
    pub fn count_values(&self, path: &WildcardPath) -> Vec<u64> {
        self.counts
            .get(path)
            .map_or_else(Vec::new, |m| m.values().copied().collect())
    }

    /// Returns the top-k most frequent values at a path, ordered by
    /// descending count, then lexicographic value for determinism.
    #[must_use]
    pub fn top_k(&self, path: &WildcardPath, k: usize) -> Vec<(String, u64)> {
        let Some(map) = self.counts.get(path) else {
            return Vec::new();
        };
        let mut entries: Vec<(String, u64)> = map.iter().map(|(k, &v)| (k.clone(), v)).collect();
        // Sort by descending count, then ascending key for stability.
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        entries.truncate(k);
        entries
    }

    /// Returns all paths that have been observed.
    #[must_use]
    pub fn paths(&self) -> Vec<WildcardPath> {
        self.counts.keys().cloned().collect()
    }

    // ---- private ----

    fn walk(&mut self, value: &serde_json::Value, path: &WildcardPath) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let child_path = path.push_key(key);
                    self.walk(child, &child_path);
                }
            }
            serde_json::Value::Array(arr) => {
                let child_path = path.push_array_wildcard();
                for child in arr {
                    self.walk(child, &child_path);
                }
            }
            // Scalars: record the canonical string.
            scalar => {
                let key = scalar_to_string(scalar);
                *self
                    .counts
                    .entry(path.clone())
                    .or_default()
                    .entry(key)
                    .or_insert(0) += 1;
            }
        }
    }
}

impl Default for FrequencyCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a scalar `serde_json::Value` to a deterministic string.
///
/// - `Null`    -> `"null"`
/// - `Bool`    -> `"true"` / `"false"`
/// - `Number`  -> the JSON number string (via `serde_json::Number::to_string`)
/// - `String`  -> the string value itself (no quotes)
///
/// Arrays and objects should not reach here; if they do, we produce a
/// stable fallback.
fn scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Bool(b) => {
            if *b {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        // Fallback for containers (should not happen).
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(json: &str) -> Document {
        vajra_core::parse_str(json).unwrap_or_else(|e| {
            panic!("parse failed: {e}");
        })
    }

    #[test]
    fn simple_object_counts() {
        let doc = parse_doc(r#"{"a": "hello", "b": "hello", "c": 42}"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let a_path = WildcardPath::root().push_key("a");
        let b_path = WildcardPath::root().push_key("b");
        let c_path = WildcardPath::root().push_key("c");

        assert_eq!(fc.cardinality(&a_path), 1);
        assert_eq!(fc.cardinality(&b_path), 1);
        assert_eq!(fc.cardinality(&c_path), 1);

        // "a" has value "hello" with count 1
        let a_counts = fc.counts().get(&a_path);
        assert!(a_counts.is_some());
        assert_eq!(a_counts.and_then(|m| m.get("hello")), Some(&1));
    }

    #[test]
    fn array_elements_counted() {
        let doc = parse_doc(r#"[1, 2, 1, 3, 1]"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let arr_path = WildcardPath::root().push_array_wildcard();
        assert_eq!(fc.cardinality(&arr_path), 3); // 1, 2, 3

        let counts = fc.counts().get(&arr_path);
        assert_eq!(counts.and_then(|m| m.get("1")), Some(&3));
        assert_eq!(counts.and_then(|m| m.get("2")), Some(&1));
        assert_eq!(counts.and_then(|m| m.get("3")), Some(&1));
    }

    #[test]
    fn nested_paths_counted() {
        let doc = parse_doc(
            r#"[
                {"code": "A01", "value": 10},
                {"code": "B02", "value": 20},
                {"code": "A01", "value": 10}
            ]"#,
        );
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let code_path = WildcardPath::root().push_array_wildcard().push_key("code");
        let value_path = WildcardPath::root().push_array_wildcard().push_key("value");

        assert_eq!(fc.cardinality(&code_path), 2); // A01, B02
        assert_eq!(fc.cardinality(&value_path), 2); // 10, 20

        let code_counts = fc.counts().get(&code_path);
        assert_eq!(code_counts.and_then(|m| m.get("A01")), Some(&2));
        assert_eq!(code_counts.and_then(|m| m.get("B02")), Some(&1));
    }

    #[test]
    fn top_k_ordering() {
        let doc = parse_doc(r#"["a", "b", "a", "c", "b", "a"]"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let path = WildcardPath::root().push_array_wildcard();
        let top = fc.top_k(&path, 2);

        assert_eq!(top.len(), 2);
        assert_eq!(top[0], ("a".to_owned(), 3));
        assert_eq!(top[1], ("b".to_owned(), 2));
    }

    #[test]
    fn top_k_tie_breaking() {
        let doc = parse_doc(r#"["b", "a", "c"]"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let path = WildcardPath::root().push_array_wildcard();
        let top = fc.top_k(&path, 3);

        // All count=1, sorted alphabetically
        assert_eq!(top[0].0, "a");
        assert_eq!(top[1].0, "b");
        assert_eq!(top[2].0, "c");
    }

    #[test]
    fn null_bool_scalars() {
        let doc = parse_doc(r#"{"n": null, "b": true, "c": false}"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let n_path = WildcardPath::root().push_key("n");
        let b_path = WildcardPath::root().push_key("b");
        let c_path = WildcardPath::root().push_key("c");

        let n_counts = fc.counts().get(&n_path);
        assert_eq!(n_counts.and_then(|m| m.get("null")), Some(&1));

        let b_counts = fc.counts().get(&b_path);
        assert_eq!(b_counts.and_then(|m| m.get("true")), Some(&1));

        let c_counts = fc.counts().get(&c_path);
        assert_eq!(c_counts.and_then(|m| m.get("false")), Some(&1));
    }

    #[test]
    fn containers_not_counted() {
        let doc = parse_doc(r#"{"items": [1, 2]}"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        // The path $.items (an array) should not be counted as a scalar
        let items_path = WildcardPath::root().push_key("items");
        assert_eq!(fc.cardinality(&items_path), 0);

        // But $.items[*] should have counts
        let elem_path = items_path.push_array_wildcard();
        assert_eq!(fc.cardinality(&elem_path), 2);
    }

    #[test]
    fn empty_object() {
        let doc = parse_doc("{}");
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);
        assert!(fc.paths().is_empty());
    }

    #[test]
    fn count_values_for_entropy() {
        let doc = parse_doc(r#"["x", "y", "x", "x", "y"]"#);
        let mut fc = FrequencyCounter::new();
        fc.count_document(&doc);

        let path = WildcardPath::root().push_array_wildcard();
        let mut cv = fc.count_values(&path);
        cv.sort();
        assert_eq!(cv, vec![2, 3]); // "y"=2, "x"=3
    }

    #[test]
    fn nonexistent_path() {
        let fc = FrequencyCounter::new();
        let path = WildcardPath::root().push_key("missing");
        assert_eq!(fc.cardinality(&path), 0);
        assert!(fc.count_values(&path).is_empty());
        assert!(fc.top_k(&path, 5).is_empty());
    }
}
