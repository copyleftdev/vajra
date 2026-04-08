//! Feature store: per-path feature vectors used by the scoring model.

use crate::path::WildcardPath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Per-path feature vector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathFeatures {
    /// Shannon entropy of values at this path.
    pub entropy: Option<f64>,
    /// Normalized entropy (entropy / log2(support)).
    pub normalized_entropy: Option<f64>,
    /// Null rate.
    pub null_rate: Option<f64>,
    /// Type instability ratio.
    pub type_instability: Option<f64>,
    /// Number of distinct values observed.
    pub cardinality: Option<u64>,
    /// Total observations at this path.
    pub count: Option<u64>,
    /// Rarity score (self-information of the rarest value).
    pub max_rarity: Option<f64>,
    /// Numeric statistics (if path contains numeric values).
    pub numeric: Option<NumericFeatures>,
}

/// Numeric distribution features for a path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NumericFeatures {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub mad: f64,
    pub p05: f64,
    pub p25: f64,
    pub p75: f64,
    pub p95: f64,
}

/// Central feature storage for a document's analysis results.
///
/// Maps wildcard paths to their computed feature vectors.
/// Uses `BTreeMap` for deterministic iteration order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureStore {
    features: BTreeMap<WildcardPath, PathFeatures>,
}

impl FeatureStore {
    /// Create an empty feature store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create the feature vector for a path.
    pub fn get_or_create(&mut self, path: &WildcardPath) -> &mut PathFeatures {
        self.features.entry(path.clone()).or_default()
    }

    /// Get the feature vector for a path, if it exists.
    #[must_use]
    pub fn get(&self, path: &WildcardPath) -> Option<&PathFeatures> {
        self.features.get(path)
    }

    /// Iterate over all (path, features) in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&WildcardPath, &PathFeatures)> {
        self.features.iter()
    }

    /// Number of paths with features.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Returns `true` if no features have been stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}
