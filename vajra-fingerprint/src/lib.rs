//! Structural fingerprinting for data artifacts.
//!
//! This crate provides deterministic, content-independent hashes that
//! capture the *shape* of a JSON document at three levels of granularity:
//!
//! - **Path set**: hash of the distinct wildcard paths (schema skeleton).
//! - **Typed path**: hash of (path, `dominant_type`) pairs (typed schema).
//! - **Merkle shape**: bottom-up hash of the JSON tree structure.
//!
//! All hashes use BLAKE3 and are 32 bytes. An [`Analyzer`](vajra_types::Analyzer)
//! implementation ties the three together for pipeline integration.

pub mod analyzer;
pub mod merkle;
pub mod path_set;
pub mod similarity;
pub mod streaming;
pub mod typed_path;

pub use analyzer::{FingerprintAnalyzer, FingerprintResult};
pub use merkle::{merkle_fingerprint, merkle_with_motifs};
pub use path_set::path_set_fingerprint;
pub use similarity::{
    cluster_documents, jaccard_similarity, minhash_signature, minhash_similarity,
    ClusterResult, LshIndex, MinHashSignature,
};
pub use streaming::{StreamingFingerprintAccumulator, StreamingFingerprintResult};
pub use typed_path::typed_path_fingerprint;
