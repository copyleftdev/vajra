//! Type instability detection across JSON paths.
//!
//! Identifies paths where the JSON type is not consistent, indicating
//! schema drift or mixed-type fields.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vajra_types::json_type::JsonType;
use vajra_types::path::{PathSegment, WildcardPath};
use vajra_types::trie::{PathTrie, TrieNode};

/// A path flagged for type instability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeInstability {
    /// The path exhibiting type instability.
    pub path: String,
    /// The instability score (0.0 = stable, approaching 1.0 = highly unstable).
    pub instability: f64,
    /// The dominant type at this path.
    pub dominant_type: String,
    /// Distribution of types observed at this path.
    pub type_distribution: BTreeMap<String, u64>,
}

/// All seven JSON type names in index order, matching [`JsonType::index`].
const TYPE_NAMES: [&str; 7] = ["null", "boolean", "integer", "float", "string", "array", "object"];

/// Compute the type instability score for a set of type counts.
///
/// Instability = 1 - (`max_count` / total).
///
/// Returns 0.0 if all observations are the same type, and approaches
/// 1.0 as the distribution becomes more uniform across types.
///
/// Returns 0.0 for zero total observations.
#[must_use]
pub fn type_instability_score(type_counts: &[u64; 7]) -> f64 {
    let total: u64 = type_counts.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let max_count = type_counts.iter().max().copied().unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let result = 1.0 - (max_count as f64 / total as f64);
    result
}

/// Detect type instabilities by walking the path trie.
///
/// Returns all paths where the type instability score exceeds `threshold`.
///
/// # Arguments
///
/// * `trie` - The path trie to walk.
/// * `threshold` - Instability threshold (default 0.01). Paths with instability
///   above this value are flagged.
#[must_use]
pub fn detect_type_instabilities(trie: &PathTrie, threshold: f64) -> Vec<TypeInstability> {
    let mut results = Vec::new();
    walk_trie(trie.root(), &WildcardPath::root(), threshold, &mut results);
    results
}

/// Recursively walk the trie, collecting instabilities.
fn walk_trie(
    node: &TrieNode,
    current_path: &WildcardPath,
    threshold: f64,
    results: &mut Vec<TypeInstability>,
) {
    let meta = &node.metadata;
    if meta.count > 0 {
        let instability = type_instability_score(&meta.type_counts);
        if instability > threshold {
            let dominant = dominant_type_name(&meta.type_counts);
            let distribution = build_type_distribution(&meta.type_counts);
            results.push(TypeInstability {
                path: current_path.to_string(),
                instability,
                dominant_type: dominant,
                type_distribution: distribution,
            });
        }
    }

    for (segment, child) in &node.children {
        let child_path = match segment {
            PathSegment::Root => current_path.clone(),
            PathSegment::Key(k) => current_path.push_key(k),
            PathSegment::ArrayWildcard => current_path.push_array_wildcard(),
        };
        walk_trie(child, &child_path, threshold, results);
    }
}

/// Get the name of the dominant type from type counts.
fn dominant_type_name(type_counts: &[u64; 7]) -> String {
    let all_types = [
        JsonType::Null,
        JsonType::Bool,
        JsonType::Integer,
        JsonType::Float,
        JsonType::String,
        JsonType::Array,
        JsonType::Object,
    ];
    all_types
        .iter()
        .max_by_key(|t| type_counts[t.index()])
        .map_or_else(|| "unknown".to_owned(), std::string::ToString::to_string)
}

/// Build a `BTreeMap` of type names to counts, omitting zero counts.
fn build_type_distribution(type_counts: &[u64; 7]) -> BTreeMap<String, u64> {
    let mut dist = BTreeMap::new();
    for (i, &count) in type_counts.iter().enumerate() {
        if count > 0 {
            if let Some(&name) = TYPE_NAMES.get(i) {
                dist.insert(name.to_owned(), count);
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    #[test]
    fn single_type_instability_zero() {
        let mut counts = [0u64; 7];
        counts[JsonType::String.index()] = 100;
        let score = type_instability_score(&counts);
        assert!(
            score.abs() < EPS,
            "single type should have instability 0, got {score}"
        );
    }

    #[test]
    fn ninety_ten_instability() {
        let mut counts = [0u64; 7];
        counts[JsonType::String.index()] = 90;
        counts[JsonType::Integer.index()] = 10;
        let score = type_instability_score(&counts);
        assert!(
            (score - 0.1).abs() < EPS,
            "90/10 split should have instability ~0.1, got {score}"
        );
    }

    #[test]
    fn all_zeros_instability_zero() {
        let counts = [0u64; 7];
        let score = type_instability_score(&counts);
        assert!(score.abs() < EPS, "empty counts should have instability 0");
    }

    #[test]
    fn uniform_instability() {
        // 7 types each with 100 observations
        let counts = [100u64; 7];
        let score = type_instability_score(&counts);
        // instability = 1 - (100 / 700) = 1 - 1/7 ~ 0.857
        let expected = 1.0 - (1.0 / 7.0);
        assert!(
            (score - expected).abs() < EPS,
            "uniform distribution should have instability ~{expected}, got {score}"
        );
    }

    #[test]
    fn detect_instabilities_empty_trie() {
        let trie = PathTrie::new();
        let result = detect_type_instabilities(&trie, 0.01);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_instabilities_stable_paths() {
        let mut trie = PathTrie::new();
        let path = WildcardPath::root().push_key("name");
        let node = trie.get_or_create(&path);
        for _ in 0..100 {
            node.metadata.observe(JsonType::String, 1);
        }

        let result = detect_type_instabilities(&trie, 0.01);
        assert!(result.is_empty(), "stable paths should not be flagged");
    }

    #[test]
    fn detect_instabilities_finds_unstable_path() {
        let mut trie = PathTrie::new();

        // Stable path
        let stable = WildcardPath::root().push_key("id");
        let node = trie.get_or_create(&stable);
        for _ in 0..100 {
            node.metadata.observe(JsonType::Integer, 1);
        }

        // Unstable path: 90% string, 10% integer
        let unstable = WildcardPath::root().push_key("value");
        let node = trie.get_or_create(&unstable);
        for _ in 0..90 {
            node.metadata.observe(JsonType::String, 1);
        }
        for _ in 0..10 {
            node.metadata.observe(JsonType::Integer, 1);
        }

        let result = detect_type_instabilities(&trie, 0.01);
        assert_eq!(result.len(), 1, "should find exactly one unstable path");
        assert_eq!(result[0].path, "$.value");
        assert!(
            (result[0].instability - 0.1).abs() < EPS,
            "instability should be ~0.1, got {}",
            result[0].instability
        );
        assert_eq!(result[0].dominant_type, "string");
    }

    #[test]
    fn detect_instabilities_walks_nested_paths() {
        let mut trie = PathTrie::new();

        // Deep nested unstable path
        let path = WildcardPath::root()
            .push_key("claims")
            .push_array_wildcard()
            .push_key("amount");
        let node = trie.get_or_create(&path);
        for _ in 0..80 {
            node.metadata.observe(JsonType::Float, 1);
        }
        for _ in 0..20 {
            node.metadata.observe(JsonType::String, 1);
        }

        let result = detect_type_instabilities(&trie, 0.01);
        assert!(!result.is_empty());
        let found = result
            .iter()
            .any(|r| r.path == "$.claims[*].amount");
        assert!(found, "should find unstable nested path");
    }

    #[test]
    fn type_distribution_excludes_zero_counts() {
        let mut counts = [0u64; 7];
        counts[JsonType::String.index()] = 90;
        counts[JsonType::Integer.index()] = 10;
        let dist = build_type_distribution(&counts);
        assert_eq!(dist.len(), 2, "should only include non-zero types");
        assert_eq!(dist.get("string"), Some(&90));
        assert_eq!(dist.get("integer"), Some(&10));
    }
}
