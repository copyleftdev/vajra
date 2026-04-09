//! Document model: parsed JSON value tree + path trie + metadata.

use crate::trie::PathTrie;
use serde::{Deserialize, Serialize};

/// Metadata about a parsed document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Total number of nodes in the JSON tree.
    pub total_nodes: u64,
    /// Maximum nesting depth.
    pub max_depth: u32,
    /// Number of distinct wildcard paths.
    pub distinct_paths: u32,
    /// Size in bytes of the raw input.
    pub raw_size_bytes: u64,
}

/// A parsed JSON document with structural index.
///
/// Contains the raw `serde_json::Value` tree, a path trie built
/// during parsing, and document-level metadata. Constructed via
/// `vajra_core::parse`.
#[derive(Debug, Clone)]
pub struct Document {
    /// The parsed JSON value tree.
    value: serde_json::Value,
    /// Structural path index.
    trie: PathTrie,
    /// Document-level metadata.
    metadata: DocumentMetadata,
}

impl Document {
    /// Create a new document from its components.
    ///
    /// This is called by the parser in `vajra-core`. External consumers
    /// should use `vajra_core::parse::parse_str` or `parse_file`.
    #[must_use]
    pub fn new(value: serde_json::Value, trie: PathTrie, metadata: DocumentMetadata) -> Self {
        Self {
            value,
            trie,
            metadata,
        }
    }

    /// Returns the parsed JSON value tree.
    #[must_use]
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }

    /// Returns the structural path trie.
    #[must_use]
    pub fn trie(&self) -> &PathTrie {
        &self.trie
    }

    /// Returns a mutable reference to the path trie.
    pub fn trie_mut(&mut self) -> &mut PathTrie {
        &mut self.trie
    }

    /// Returns document-level metadata.
    #[must_use]
    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    /// Consume the document and return the inner JSON value.
    ///
    /// This is useful when aggregating multiple documents (e.g., NDJSON lines)
    /// into a single array value without cloning.
    #[must_use]
    pub fn into_value(self) -> serde_json::Value {
        self.value
    }
}
