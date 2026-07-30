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

pub mod analyzer;
pub mod benford;
pub mod cms;
pub mod core_team;
pub mod ddsketch;
pub mod entropy;
pub mod frequency;
pub mod governance;
pub mod identity;
pub mod lz_complexity;
pub mod mad;
pub mod numeric;
pub mod relationships;
pub mod renyi;
pub mod separation;
pub mod space_saving;
pub mod streaming;
pub mod temporal;
pub mod total_correlation;
pub mod transfer_entropy;

pub use analyzer::{PathStats, StatsAnalyzer, StatsResult};
pub use benford::{
    analyze_benford, benford_chi_squared, benford_mad_score, expected_benford_distribution,
    leading_digit_distribution, BenfordResult,
};
pub use cms::CountMinSketch;
pub use core_team::{
    commit_records_from_json, detect_core_team, render_core_team_text, AuthorClassification,
    AuthorRole, CommitRecord, CoreTeamResult,
};
pub use ddsketch::DDSketch;
pub use entropy::{normalized_entropy, shannon_entropy_from_counts};
pub use frequency::FrequencyCounter;
pub use governance::{
    governance_analysis, render_markdown as render_governance_markdown,
    render_text as render_governance_text, ChurnMetrics, GovernanceError, GovernanceMetrics,
    GovernanceReport, MonthChurn,
};
pub use identity::{normalise_email, resolve_identities, Identity, IdentityResolution};
pub use lz_complexity::{
    classify_entropy_complexity, lz76_complexity, lz76_phrase_count, lz_analyze,
    lz_complexity_for_values, LzResult,
};
pub use mad::{mad, median, modified_z_score};
pub use numeric::{compute_numeric_stats, percentile, NumericStats};
pub use relationships::{
    conditional_entropy, discover_relationships, discover_relationships_binned, pmi, BinStrategy,
    FieldRelationship,
};
pub use renyi::{normalized_renyi_spectrum, renyi_entropy, renyi_spectrum, RenyiSpectrum};
pub use separation::{
    cliffs_delta, separation_analysis, FeatureSeparation, FieldKind, OperatingPoint,
    SeparationError, SeparationReport,
};
pub use space_saving::SpaceSaving;
pub use streaming::{StreamingConfig, StreamingStatsAccumulator};
pub use temporal::{
    auto_detect_time_field, bucket_by_window, detect_dates, extract_json_path, linear_regression,
    parse_iso8601, temporal_analysis, truncate_to_window, value_to_epoch, windowed_analysis,
    DateFormat, FieldWindowStats, GapInfo, TemporalReport, TemporalValue, TrendLine,
    WindowGranularity, WindowSummary, WindowedAnalysisResult,
};
pub use total_correlation::{
    total_correlation, total_correlation_for_fields, TotalCorrelationResult,
};
pub use transfer_entropy::{
    bidirectional_transfer_entropy, bin_strings, bin_values, transfer_entropy,
    TransferEntropyResult,
};
