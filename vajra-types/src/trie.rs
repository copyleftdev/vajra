//! Path trie for indexing wildcard paths with aggregated metadata.
//!
//! The trie is the primary structural index for a parsed JSON document.
//! Each node stores aggregated statistics about all values observed at
//! that path position.

use crate::json_type::JsonType;
use crate::path::{PathSegment, WildcardPath};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Aggregated metadata for a single path position.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathMetadata {
    /// Total number of observations at this path.
    pub count: u64,
    /// Count per JSON type observed.
    pub type_counts: [u64; JsonType::COUNT],
    /// Maximum depth seen (from root).
    pub depth: u32,
    /// Number of null values observed.
    pub null_count: u64,
}

impl PathMetadata {
    /// Record an observation of a value at this path.
    pub fn observe(&mut self, json_type: JsonType, depth: u32) {
        self.count += 1;
        self.type_counts[json_type.index()] += 1;
        if json_type == JsonType::Null {
            self.null_count += 1;
        }
        if depth > self.depth {
            self.depth = depth;
        }
    }

    /// The dominant (most frequent) type at this path.
    #[must_use]
    pub fn dominant_type(&self) -> Option<JsonType> {
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
            .max_by_key(|t| self.type_counts[t.index()])
            .filter(|t| self.type_counts[t.index()] > 0)
            .copied()
    }

    /// Type instability ratio: 1 - (dominant_count / total).
    /// Returns 0.0 for single-type paths, higher for mixed.
    #[must_use]
    pub fn type_instability(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let max_count = self.type_counts.iter().max().copied().unwrap_or(0);
        1.0 - (max_count as f64 / self.count as f64)
    }

    /// Null rate: null_count / total.
    #[must_use]
    pub fn null_rate(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.null_count as f64 / self.count as f64
    }
}

/// A node in the path trie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrieNode {
    /// The segment this node represents.
    pub segment: PathSegment,
    /// Child nodes, ordered by segment for deterministic iteration.
    pub children: BTreeMap<PathSegment, TrieNode>,
    /// Aggregated metadata for values at this path.
    pub metadata: PathMetadata,
}

impl TrieNode {
    /// Create a new trie node for the given segment.
    #[must_use]
    pub fn new(segment: PathSegment) -> Self {
        Self {
            segment,
            children: BTreeMap::new(),
            metadata: PathMetadata::default(),
        }
    }

    /// Get or create a child node for the given segment.
    pub fn child_mut(&mut self, segment: PathSegment) -> &mut Self {
        self.children
            .entry(segment.clone())
            .or_insert_with(|| Self::new(segment))
    }

    /// Returns `true` if this node has no children.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Count of all descendant nodes (not counting self).
    #[must_use]
    pub fn descendant_count(&self) -> usize {
        self.children
            .values()
            .map(|child| 1 + child.descendant_count())
            .sum()
    }
}

/// A trie of wildcard paths indexing a JSON document's structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathTrie {
    root: TrieNode,
}

impl PathTrie {
    /// Create an empty path trie with a root node.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(PathSegment::Root),
        }
    }

    /// Returns a reference to the root node.
    #[must_use]
    pub fn root(&self) -> &TrieNode {
        &self.root
    }

    /// Returns a mutable reference to the root node.
    pub fn root_mut(&mut self) -> &mut TrieNode {
        &mut self.root
    }

    /// Navigate to the trie node for a given path, creating nodes as needed.
    pub fn get_or_create(&mut self, path: &WildcardPath) -> &mut TrieNode {
        let mut current = &mut self.root;
        // Skip the root segment (first in path)
        for segment in path.segments().iter().skip(1) {
            current = current.child_mut(segment.clone());
        }
        current
    }

    /// Navigate to the trie node for a given path, returning `None` if not found.
    #[must_use]
    pub fn get(&self, path: &WildcardPath) -> Option<&TrieNode> {
        let mut current = &self.root;
        for segment in path.segments().iter().skip(1) {
            current = current.children.get(segment)?;
        }
        Some(current)
    }

    /// Collect all wildcard paths in the trie (in deterministic order).
    #[must_use]
    pub fn all_paths(&self) -> Vec<WildcardPath> {
        let mut paths = Vec::new();
        collect_paths_recursive(&self.root, &WildcardPath::root(), &mut paths);
        paths
    }

    /// Number of distinct paths (including root).
    #[must_use]
    pub fn path_count(&self) -> usize {
        1 + self.root.descendant_count()
    }
}

fn collect_paths_recursive(
    node: &TrieNode,
    current_path: &WildcardPath,
    paths: &mut Vec<WildcardPath>,
) {
    paths.push(current_path.clone());
    for (segment, child) in &node.children {
        let child_path = match segment {
            PathSegment::Root => current_path.clone(),
            PathSegment::Key(k) => current_path.push_key(k),
            PathSegment::ArrayWildcard => current_path.push_array_wildcard(),
        };
        collect_paths_recursive(child, &child_path, paths);
    }
}

impl Default for PathTrie {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trie() {
        let trie = PathTrie::new();
        assert_eq!(trie.path_count(), 1); // just root
        assert_eq!(trie.all_paths().len(), 1);
    }

    #[test]
    fn insert_and_retrieve() {
        let mut trie = PathTrie::new();
        let path = WildcardPath::root().push_key("a").push_key("b");
        let node = trie.get_or_create(&path);
        node.metadata.observe(JsonType::String, 2);

        let found = trie.get(&path);
        assert!(found.is_some());
        assert_eq!(found.map(|n| n.metadata.count), Some(1));
    }

    #[test]
    fn all_paths_is_deterministic() {
        let mut trie = PathTrie::new();
        // Insert in arbitrary order
        trie.get_or_create(&WildcardPath::root().push_key("z"));
        trie.get_or_create(&WildcardPath::root().push_key("a"));
        trie.get_or_create(&WildcardPath::root().push_key("m"));

        let paths: Vec<String> = trie.all_paths().iter().map(|p| p.to_string()).collect();
        // BTreeMap guarantees sorted order
        assert_eq!(paths, vec!["$", "$.a", "$.m", "$.z"]);
    }

    #[test]
    fn metadata_type_instability() {
        let mut meta = PathMetadata::default();
        for _ in 0..90 {
            meta.observe(JsonType::String, 1);
        }
        for _ in 0..10 {
            meta.observe(JsonType::Integer, 1);
        }
        assert_eq!(meta.dominant_type(), Some(JsonType::String));
        let instability = meta.type_instability();
        assert!((instability - 0.1).abs() < 1e-10);
    }
}
