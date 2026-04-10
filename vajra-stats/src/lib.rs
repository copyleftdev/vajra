//! Statistical summaries and descriptive statistics for Vajra.
//!
//! Phase 1: exact counting (no probabilistic sketches yet).
//!
//! - **entropy** — Shannon entropy and normalized entropy
//! - **mad** — Median Absolute Deviation and modified z-scores
//! - **numeric** — Numeric distribution summary (min/max/mean/percentiles)
//! - **frequency** — Exact per-path frequency counting
//! - **analyzer** — Full stats analyzer implementing `Analyzer` + `FeatureExtractor`
//! - **benford** — Benford's Law analysis for leading digit distributions
//! - **temporal** — Temporal pattern analysis for date/time data
//! - **governance** — Bus factor, Gini coefficient, contributor churn

pub mod analyzer;
pub mod benford;
pub mod cms;
pub mod ddsketch;
pub mod entropy;
pub mod frequency;
pub mod governance;
pub mod mad;
pub mod numeric;
pub mod relationships;
pub mod space_saving;
pub mod streaming;
pub mod temporal;

pub use analyzer::{PathStats, StatsAnalyzer, StatsResult};
pub use benford::{
    analyze_benford, benford_chi_squared, benford_mad_score, expected_benford_distribution,
    leading_digit_distribution, BenfordResult,
};
pub use cms::CountMinSketch;
pub use ddsketch::DDSketch;
pub use entropy::{normalized_entropy, shannon_entropy_from_counts};
pub use frequency::FrequencyCounter;
pub use governance::{
    governance_analysis, render_markdown as governance_markdown, render_text as governance_text,
    ChurnMetrics, GovernanceError, GovernanceMetrics, GovernanceReport, MonthChurn,
};
pub use mad::{mad, median, modified_z_score};
pub use numeric::{compute_numeric_stats, percentile, NumericStats};
pub use relationships::{conditional_entropy, discover_relationships, pmi, FieldRelationship};
pub use space_saving::SpaceSaving;
pub use streaming::{StreamingConfig, StreamingStatsAccumulator};
pub use temporal::{
    auto_detect_time_field, bucket_by_window, detect_dates, extract_json_path, linear_regression,
    parse_iso8601, temporal_analysis, truncate_to_window, value_to_epoch, windowed_analysis,
    DateFormat, FieldWindowStats, GapInfo, TemporalReport, TemporalValue, TrendLine,
    WindowGranularity, WindowSummary, WindowedAnalysisResult,
};
