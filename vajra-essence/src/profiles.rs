//! Built-in concern profiles for essence scoring and rendering.
//!
//! Each profile defines score weights and rendering style appropriate
//! for its target audience.

use vajra_types::error::VajraError;
use vajra_types::scoring::ScoreWeights;
use vajra_types::traits::{CandidateObservation, ConcernProfile, EssenceData, OutputFormat};

use crate::compact_ai::render_compact_ai;
use crate::markdown::render_markdown;
use crate::render;

/// Staff-oriented concern profile.
///
/// Uses plain vocabulary and narrative rendering style.
/// Emphasizes anomalies and structural coverage over technical details.
pub struct StaffProfile;

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for StaffProfile {
    fn name(&self) -> &str {
        "staff"
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        ScoreWeights::staff().score(candidate)
    }

    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError> {
        match format {
            OutputFormat::Json => serde_json::to_string_pretty(&render::render_json(essence))
                .map_err(|e| VajraError::Analysis {
                    path: String::new(),
                    message: format!("JSON serialization failed: {e}"),
                }),
            OutputFormat::CompactAi => {
                serde_json::to_string_pretty(&render_compact_ai(essence, false))
                    .map_err(|e| VajraError::Analysis {
                        path: String::new(),
                        message: format!("JSON serialization failed: {e}"),
                    })
            }
            OutputFormat::Markdown => Ok(render_markdown(essence, "staff")),
            _ => Ok(render::render_text(essence, "staff")),
        }
    }
}

/// Engineer-oriented concern profile.
///
/// Uses technical vocabulary and list-based rendering style.
/// Balanced scoring across all signal dimensions.
pub struct EngineerProfile;

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for EngineerProfile {
    fn name(&self) -> &str {
        "engineer"
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        ScoreWeights::engineer().score(candidate)
    }

    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError> {
        match format {
            OutputFormat::Json => serde_json::to_string_pretty(&render::render_json(essence))
                .map_err(|e| VajraError::Analysis {
                    path: String::new(),
                    message: format!("JSON serialization failed: {e}"),
                }),
            OutputFormat::CompactAi => {
                serde_json::to_string_pretty(&render_compact_ai(essence, false))
                    .map_err(|e| VajraError::Analysis {
                        path: String::new(),
                        message: format!("JSON serialization failed: {e}"),
                    })
            }
            OutputFormat::Markdown => Ok(render_markdown(essence, "engineer")),
            _ => Ok(render::render_text(essence, "engineer")),
        }
    }
}

/// Fraud-investigation concern profile.
///
/// Uses technical vocabulary with investigative framing.
/// Emphasizes outliers, rarity, and suspicious repetition.
pub struct FraudProfile;

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for FraudProfile {
    fn name(&self) -> &str {
        "fraud"
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        ScoreWeights::fraud().score(candidate)
    }

    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError> {
        match format {
            OutputFormat::Json => serde_json::to_string_pretty(&render::render_json(essence))
                .map_err(|e| VajraError::Analysis {
                    path: String::new(),
                    message: format!("JSON serialization failed: {e}"),
                }),
            OutputFormat::CompactAi => {
                serde_json::to_string_pretty(&render_compact_ai(essence, false))
                    .map_err(|e| VajraError::Analysis {
                        path: String::new(),
                        message: format!("JSON serialization failed: {e}"),
                    })
            }
            OutputFormat::Markdown => Ok(render_markdown(essence, "fraud")),
            _ => Ok(render::render_text(essence, "fraud")),
        }
    }
}

/// Auditor-oriented concern profile.
///
/// Uses formal vocabulary and completeness-focused rendering style.
/// Emphasizes instability and concern relevance.
pub struct AuditorProfile;

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for AuditorProfile {
    fn name(&self) -> &str {
        "auditor"
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        ScoreWeights::auditor().score(candidate)
    }

    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError> {
        match format {
            OutputFormat::Json => serde_json::to_string_pretty(&render::render_json(essence))
                .map_err(|e| VajraError::Analysis {
                    path: String::new(),
                    message: format!("JSON serialization failed: {e}"),
                }),
            OutputFormat::CompactAi => {
                serde_json::to_string_pretty(&render_compact_ai(essence, false))
                    .map_err(|e| VajraError::Analysis {
                        path: String::new(),
                        message: format!("JSON serialization failed: {e}"),
                    })
            }
            OutputFormat::Markdown => Ok(render_markdown(essence, "auditor")),
            _ => Ok(render::render_text(essence, "auditor")),
        }
    }
}

/// AI/machine-handoff concern profile.
///
/// Uses compact, terse rendering style optimized for machine consumption.
/// JSON-like structure even in text mode.
pub struct AiProfile;

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for AiProfile {
    fn name(&self) -> &str {
        "ai"
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        ScoreWeights::ai().score(candidate)
    }

    fn render(&self, essence: &EssenceData, format: OutputFormat) -> Result<String, VajraError> {
        match format {
            OutputFormat::Json => serde_json::to_string_pretty(&render::render_json(essence))
                .map_err(|e| VajraError::Analysis {
                    path: String::new(),
                    message: format!("JSON serialization failed: {e}"),
                }),
            OutputFormat::CompactAi => {
                serde_json::to_string_pretty(&render_compact_ai(essence, true))
                    .map_err(|e| VajraError::Analysis {
                        path: String::new(),
                        message: format!("JSON serialization failed: {e}"),
                    })
            }
            OutputFormat::Markdown => Ok(render_markdown(essence, "ai")),
            _ => Ok(render::render_text(essence, "ai")),
        }
    }
}
