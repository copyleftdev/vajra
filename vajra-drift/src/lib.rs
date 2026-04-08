//! Schema and data drift detection between dataset versions.
//!
//! This crate provides tools for detecting structural and distributional
//! drift between two JSON documents:
//!
//! - **`path_diff`** — Structural path set comparison (added/removed/shared)
//! - **`jsd`** — Jensen-Shannon Divergence for categorical distributions
//! - **`wasserstein`** — 1D Wasserstein (Earth Mover's) distance for numeric distributions
//! - **`analyzer`** — Full drift analysis combining all metrics with severity classification

pub mod analyzer;
pub mod jsd;
pub mod path_diff;
pub mod wasserstein;

pub use analyzer::{
    full_drift, DistributionalDrift, DriftAnalyzer, DriftMetric, DriftSeverity, FullDriftReport,
};
pub use jsd::{jensen_shannon_divergence, jsd_metric, JsdError};
pub use path_diff::{jaccard_similarity, path_diff, PathDiffResult};
pub use wasserstein::wasserstein_1d;
