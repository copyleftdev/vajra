//! Shared types, traits, and contracts for the Vajra analysis pipeline.
//!
//! This crate defines the core vocabulary used across all Vajra crates:
//! path representations, document model, analysis traits, feature storage,
//! and scoring types. It depends on nothing internal.

pub mod document;
pub mod error;
pub mod features;
pub mod json_type;
pub mod path;
pub mod scoring;
pub mod traits;
pub mod trie;

pub use document::{Document, DocumentMetadata};
pub use error::VajraError;
pub use features::FeatureStore;
pub use json_type::JsonType;
pub use path::{PathSegment, WildcardPath};
pub use scoring::ScoreWeights;
pub use traits::{
    Analyzer, ConcernProfile, DriftDetector, FeatureExtractor, Fingerprinter, InferenceConfidence,
    RelationshipHint, RelationshipType, TypeRecognizer, VajraPlugin,
};
pub use trie::{PathMetadata, PathTrie, TrieNode};
