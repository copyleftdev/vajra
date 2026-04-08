//! Anomaly and outlier detection for data quality monitoring.
//!
//! This crate provides three categories of anomaly detection:
//!
//! - **Numeric outliers** (`numeric`): MAD-based modified z-score detection
//! - **Rarity scoring** (`rarity`): Self-information based rare value detection
//! - **Type instability** (`instability`): Detection of paths with mixed JSON types
//!
//! The [`AnomalyAnalyzer`] orchestrates all three and implements the
//! [`Analyzer`](vajra_types::traits::Analyzer) trait for integration into the
//! Vajra pipeline.

pub mod analyzer;
pub mod instability;
pub mod numeric;
pub mod rarity;

pub use analyzer::{AnomalyAnalyzer, AnomalyReport};
pub use instability::{detect_type_instabilities, TypeInstability};
pub use numeric::{detect_numeric_outliers, NumericOutlier, NumericOutlierScore};
pub use rarity::{detect_rare_values, rarity_score, RarityOutlier};
