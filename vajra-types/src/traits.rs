//! Core analysis traits that all Vajra analyzers implement.

use crate::document::Document;
use crate::error::VajraError;
use crate::features::FeatureStore;

/// Primary analysis trait for DOM-mode analysis.
///
/// Each analyzer examines a document and produces a typed output.
/// Analyzers must be deterministic: same document → same output, always.
pub trait Analyzer {
    /// The analysis result type.
    type Output;

    /// Analyze a document and produce a result.
    ///
    /// # Errors
    /// Returns `VajraError::Analysis` if analysis fails for a specific path,
    /// or `VajraError::LimitExceeded` if the document exceeds configured limits.
    fn analyze(&self, doc: &Document) -> Result<Self::Output, VajraError>;
}

/// Feature extraction into the shared feature store.
///
/// Feature extractors contribute per-path features that feed into
/// the scoring model for essence generation.
pub trait FeatureExtractor {
    /// Extract features from a document into the feature store.
    ///
    /// # Errors
    /// Returns `VajraError::Analysis` on extraction failure.
    fn extract(&self, doc: &Document, features: &mut FeatureStore) -> Result<(), VajraError>;
}

/// Concern profile for scoring and rendering essences.
pub trait ConcernProfile {
    /// Profile name (e.g., "staff", "auditor", "engineer").
    fn name(&self) -> &str;

    /// Score a candidate observation. Higher = more important.
    fn score(&self, candidate: &CandidateObservation) -> f64;

    /// Render an essence to a string in the given format.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError>;
}

/// Structural fingerprinting.
pub trait Fingerprinter {
    /// Compute a fingerprint for a document.
    fn fingerprint(&self, doc: &Document) -> Fingerprint;
}

/// Drift detection between two analyzed documents.
pub trait DriftDetector {
    /// Compare two documents and produce a drift report.
    fn compare(&self, lhs: &Document, rhs: &Document) -> DriftReport;
}

/// A candidate observation for scoring.
#[derive(Debug, Clone)]
pub struct CandidateObservation {
    /// The path this observation relates to.
    pub path: String,
    /// Human-readable description.
    pub description: String,
    /// Rarity signal (0-1).
    pub rarity: f64,
    /// Type instability signal (0-1).
    pub instability: f64,
    /// Entropy-derived signal (0-1).
    pub entropy_signal: f64,
    /// Structural coverage signal (0-1).
    pub structural_coverage: f64,
    /// Anomaly strength signal (0-1).
    pub anomaly_strength: f64,
    /// Concern relevance (set by profile, 0-1).
    pub concern_relevance: f64,
}

/// Output format for rendered essences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Ndjson,
    Markdown,
    CompactAi,
}

/// A structural fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    /// The hash bytes.
    pub hash: [u8; 32],
    /// What kind of fingerprint this is.
    pub kind: FingerprintKind,
}

/// What structural property a fingerprint captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FingerprintKind {
    /// Hash of the set of distinct wildcard paths.
    PathSet,
    /// Hash of (path, dominant_type) pairs.
    TypedPath,
    /// Merkle hash of the structural shape.
    Shape,
}

/// Essence data before rendering.
#[derive(Debug, Clone)]
pub struct EssenceData {
    /// Scored and ranked observations.
    pub observations: Vec<ScoredObservation>,
    /// Document metadata.
    pub document_identity: String,
    /// Input statistics.
    pub total_nodes: u64,
    /// Distinct paths.
    pub distinct_paths: u32,
    /// Max depth.
    pub max_depth: u32,
    /// Whether the output was truncated due to a token budget.
    pub truncated: bool,
    /// Estimated tokens used (set when a budget is active).
    pub budget_used: Option<usize>,
    /// The token budget limit (set when a budget is active).
    pub budget_limit: Option<usize>,
}

/// An observation with its composite score.
#[derive(Debug, Clone)]
pub struct ScoredObservation {
    /// The underlying observation.
    pub observation: CandidateObservation,
    /// Composite score from the profile's weight vector.
    pub score: f64,
}

/// A drift report between two documents.
#[derive(Debug, Clone)]
pub struct DriftReport {
    /// Paths added in the RHS.
    pub added_paths: Vec<String>,
    /// Paths removed from the LHS.
    pub removed_paths: Vec<String>,
    /// Paths where the dominant type changed.
    pub type_changes: Vec<TypeChange>,
}

/// A type change at a specific path.
#[derive(Debug, Clone)]
pub struct TypeChange {
    /// The path where the type changed.
    pub path: String,
    /// Type in the LHS document.
    pub from: String,
    /// Type in the RHS document.
    pub to: String,
}

/// Confidence level for type inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferenceConfidence {
    /// Low confidence: heuristic match.
    Heuristic,
    /// Medium confidence: dominant pattern.
    Dominant,
    /// High confidence: definite match.
    Definite,
}

/// A type recognizer that identifies domain-specific value types.
pub trait TypeRecognizer: Send + Sync {
    /// The name of the type this recognizer detects.
    fn type_name(&self) -> &str;
    /// Returns true if the value matches this type.
    fn matches(&self, value: &str) -> bool;
    /// The confidence level of this recognizer's matches.
    fn confidence(&self) -> InferenceConfidence;
    /// Priority for ordering: higher values are checked first.
    fn priority(&self) -> u32;
}

/// A hint about expected field relationships in a domain.
#[derive(Debug, Clone)]
pub struct RelationshipHint {
    /// Name of this relationship hint.
    pub name: String,
    /// Wildcard path patterns for fields involved.
    pub fields: Vec<String>,
    /// The type of relationship between the fields.
    pub relationship: RelationshipType,
    /// Weight for scoring (0.0 - 1.0).
    pub weight: f64,
}

/// The type of relationship between fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipType {
    /// Fields tend to appear together.
    CoOccurrence,
    /// One field functionally determines another.
    FunctionalDependency,
    /// One field's presence depends on another's value.
    ConditionalPresence,
    /// A code field paired with its description.
    CodeDescriptionPair,
}

/// A domain plugin that extends Vajra's analysis capabilities.
pub trait VajraPlugin: Send + Sync {
    /// Plugin name.
    fn name(&self) -> &str;
    /// Plugin version.
    fn version(&self) -> &str;
    /// Type recognizers provided by this plugin.
    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![]
    }
    /// Relationship hints provided by this plugin.
    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        vec![]
    }
}
