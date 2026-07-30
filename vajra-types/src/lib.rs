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

/// Version of the JSON output contract, bumped when the shape or meaning of
/// emitted fields changes.
///
/// Separate from the crate version, which is a release artefact. Eight output
/// changes shipped under an unchanged `0.5.0` — a nullable `self_fix_rate`, a
/// changed `hot_entities` sort key, `bus_factor.value` switching from bits to a
/// share — with nothing in the output letting a consumer detect any of them.
/// A single integer is something to branch on without parsing semver. See #104.
pub const OUTPUT_SCHEMA_VERSION: u32 = 1;

pub use document::{Document, DocumentMetadata};
pub use error::VajraError;
pub use features::FeatureStore;
pub use json_type::JsonType;
pub use path::{PathSegment, WildcardPath};
pub use scoring::{
    compute_health_score, grade_commit_entropy, grade_fix_ratio, grade_one_commit_rate,
    grade_velocity_trend, grade_zero_comment_rate, DimensionScore, HealthMetrics, HealthScore,
    HealthWeights, LetterGrade, ScoreWeights,
};
pub use traits::{
    Analyzer, ConcernProfile, DriftDetector, FeatureExtractor, Fingerprinter, InferenceConfidence,
    RelationshipHint, RelationshipType, TypeRecognizer, VajraPlugin,
};
pub use trie::{PathMetadata, PathTrie, TrieNode};
