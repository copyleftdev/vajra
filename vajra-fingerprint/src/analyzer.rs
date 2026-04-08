//! `Analyzer` trait implementation for structural fingerprinting.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vajra_types::{Analyzer, Document, VajraError};

use crate::merkle::merkle_with_motifs;
use crate::path_set::path_set_fingerprint;
use crate::typed_path::typed_path_fingerprint;

/// Result of structural fingerprinting analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintResult {
    /// Hash of the sorted set of distinct wildcard paths.
    pub path_set: [u8; 32],
    /// Hash of sorted (path, `dominant_type`) pairs.
    pub typed_path: [u8; 32],
    /// Merkle hash of the structural shape of the value tree.
    pub shape: [u8; 32],
    /// Subtree hash frequency map for motif detection.
    pub subtree_frequencies: BTreeMap<[u8; 32], u64>,
}

/// Analyzer that computes all structural fingerprints for a document.
pub struct FingerprintAnalyzer;

impl Analyzer for FingerprintAnalyzer {
    type Output = FingerprintResult;

    fn analyze(&self, doc: &Document) -> Result<Self::Output, VajraError> {
        let path_set = path_set_fingerprint(doc);
        let typed_path = typed_path_fingerprint(doc);
        let (shape, subtree_frequencies) = merkle_with_motifs(doc.value());

        Ok(FingerprintResult {
            path_set,
            typed_path,
            shape,
            subtree_frequencies,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_core::parse::parse_str;

    #[test]
    fn analyzer_produces_result() {
        let doc = parse_str(r#"{"a": 1, "b": [2, 3]}"#)
            .unwrap_or_else(|e| panic!("parse failed: {e}"));
        let result = FingerprintAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("analyze failed: {e}"));

        assert_eq!(result.path_set.len(), 32);
        assert_eq!(result.typed_path.len(), 32);
        assert_eq!(result.shape.len(), 32);
        assert!(!result.subtree_frequencies.is_empty());
    }

    #[test]
    fn analyzer_is_deterministic() {
        let doc = parse_str(r#"{"x": [1, 2], "y": {"z": true}}"#)
            .unwrap_or_else(|e| panic!("parse failed: {e}"));
        let r1 = FingerprintAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("analyze failed: {e}"));
        let r2 = FingerprintAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("analyze failed: {e}"));

        assert_eq!(r1.path_set, r2.path_set);
        assert_eq!(r1.typed_path, r2.typed_path);
        assert_eq!(r1.shape, r2.shape);
        assert_eq!(r1.subtree_frequencies, r2.subtree_frequencies);
    }
}
