//! Structural path diffing between two path tries.
//!
//! Compares the sets of wildcard paths present in two [`PathTrie`]s and
//! partitions them into added, removed, and shared sets.

use std::collections::BTreeSet;

use vajra_types::path::WildcardPath;
use vajra_types::trie::PathTrie;

/// The result of comparing two path tries structurally.
#[derive(Debug, Clone)]
pub struct PathDiffResult {
    /// Paths present in the RHS but not in the LHS.
    pub added: Vec<WildcardPath>,
    /// Paths present in the LHS but not in the RHS.
    pub removed: Vec<WildcardPath>,
    /// Paths present in both.
    pub shared: Vec<WildcardPath>,
}

/// Compare the set of all wildcard paths between two tries.
///
/// Returns a [`PathDiffResult`] partitioning paths into added, removed,
/// and shared sets. All output vectors are in deterministic order.
#[must_use]
pub fn path_diff(lhs: &PathTrie, rhs: &PathTrie) -> PathDiffResult {
    let lhs_paths: BTreeSet<WildcardPath> = lhs.all_paths().into_iter().collect();
    let rhs_paths: BTreeSet<WildcardPath> = rhs.all_paths().into_iter().collect();

    let added: Vec<WildcardPath> = rhs_paths.difference(&lhs_paths).cloned().collect();
    let removed: Vec<WildcardPath> = lhs_paths.difference(&rhs_paths).cloned().collect();
    let shared: Vec<WildcardPath> = lhs_paths.intersection(&rhs_paths).cloned().collect();

    PathDiffResult {
        added,
        removed,
        shared,
    }
}

/// Compute the Jaccard similarity between two sets of paths.
///
/// Returns 1.0 when the sets are identical, 0.0 when disjoint,
/// and `f64::NAN` when both sets are empty (vacuously similar).
#[must_use]
pub fn jaccard_similarity(lhs: &PathTrie, rhs: &PathTrie) -> f64 {
    let lhs_paths: BTreeSet<WildcardPath> = lhs.all_paths().into_iter().collect();
    let rhs_paths: BTreeSet<WildcardPath> = rhs.all_paths().into_iter().collect();

    let intersection = lhs_paths.intersection(&rhs_paths).count();
    let union = lhs_paths.union(&rhs_paths).count();

    if union == 0 {
        // Both empty — vacuously similar, but return 1.0 as a sensible default.
        return 1.0;
    }

    intersection as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::JsonType;

    /// Helper: build a trie with the given key paths (all off root).
    fn trie_with_keys(keys: &[&str]) -> PathTrie {
        let mut trie = PathTrie::new();
        trie.root_mut().metadata.observe(JsonType::Object, 0);
        for key in keys {
            let path = WildcardPath::root().push_key(key);
            let node = trie.get_or_create(&path);
            node.metadata.observe(JsonType::String, 1);
        }
        trie
    }

    #[test]
    fn identical_tries_no_diff() {
        let trie = trie_with_keys(&["a", "b", "c"]);
        let result = path_diff(&trie, &trie);

        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        // shared includes root + 3 keys = 4
        assert_eq!(result.shared.len(), 4);
    }

    #[test]
    fn completely_different_tries() {
        let lhs = trie_with_keys(&["a", "b"]);
        let rhs = trie_with_keys(&["c", "d"]);
        let result = path_diff(&lhs, &rhs);

        // Root is shared
        assert_eq!(result.shared.len(), 1);
        assert_eq!(result.added.len(), 2);
        assert_eq!(result.removed.len(), 2);
    }

    #[test]
    fn added_paths_detected() {
        let lhs = trie_with_keys(&["a"]);
        let rhs = trie_with_keys(&["a", "b"]);
        let result = path_diff(&lhs, &rhs);

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].as_str(), "$.b");
        assert!(result.removed.is_empty());
    }

    #[test]
    fn removed_paths_detected() {
        let lhs = trie_with_keys(&["a", "b"]);
        let rhs = trie_with_keys(&["a"]);
        let result = path_diff(&lhs, &rhs);

        assert!(result.added.is_empty());
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0].as_str(), "$.b");
    }

    #[test]
    fn jaccard_identical() {
        let trie = trie_with_keys(&["a", "b", "c"]);
        let sim = jaccard_similarity(&trie, &trie);
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_disjoint() {
        let lhs = trie_with_keys(&["a", "b"]);
        let rhs = trie_with_keys(&["c", "d"]);
        // Root ($) is shared, so not fully disjoint: intersection=1, union=5
        let sim = jaccard_similarity(&lhs, &rhs);
        assert!((sim - 1.0 / 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_empty_tries() {
        let lhs = PathTrie::new();
        let rhs = PathTrie::new();
        let sim = jaccard_similarity(&lhs, &rhs);
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }
}
