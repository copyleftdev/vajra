//! TOML-based custom profile configuration.
//!
//! Load custom concern profiles from TOML files, allowing users to define
//! their own scoring weights and rendering preferences.
//!
//! # Example TOML
//!
//! ```toml
//! [profile.my_review]
//! name = "my-review"
//! description = "Custom review profile"
//!
//! [profile.my_review.weights]
//! rarity = 0.15
//! instability = 0.20
//! entropy_signal = 0.10
//! structural_coverage = 0.10
//! anomaly_strength = 0.25
//! concern_relevance = 0.20
//!
//! [profile.my_review.rendering]
//! vocabulary = "plain"
//! show_paths = false
//! motif_collapse_threshold = 3
//! max_observations = 20
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use serde::Deserialize;
use vajra_types::error::VajraError;
use vajra_types::scoring::ScoreWeights;
use vajra_types::traits::{CandidateObservation, ConcernProfile, EssenceData, OutputFormat};

use crate::compact_ai::render_compact_ai;
use crate::markdown::render_markdown;
use crate::render;

// ---------------------------------------------------------------------------
// Deserialized TOML structures
// ---------------------------------------------------------------------------

/// Top-level TOML document containing one or more profile definitions.
#[derive(Debug, Deserialize)]
struct TomlDocument {
    #[serde(default)]
    profile: BTreeMap<String, ProfileConfig>,
}

/// A single profile configuration as deserialized from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileConfig {
    /// Display name of the profile.
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Score weights for the six dimensions.
    pub weights: WeightsConfig,
    /// Rendering options.
    #[serde(default)]
    pub rendering: RenderingConfig,
}

/// Score weight configuration (deserialized from TOML).
#[derive(Debug, Clone, Deserialize)]
pub struct WeightsConfig {
    pub rarity: f64,
    pub instability: f64,
    pub entropy_signal: f64,
    pub structural_coverage: f64,
    pub anomaly_strength: f64,
    pub concern_relevance: f64,
}

impl WeightsConfig {
    /// Convert to the scoring engine's `ScoreWeights`.
    #[must_use]
    pub fn to_score_weights(&self) -> ScoreWeights {
        ScoreWeights {
            rarity: self.rarity,
            instability: self.instability,
            entropy_signal: self.entropy_signal,
            structural_coverage: self.structural_coverage,
            anomaly_strength: self.anomaly_strength,
            concern_relevance: self.concern_relevance,
        }
    }

    /// Sum of all weights (ideally ~1.0).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.rarity
            + self.instability
            + self.entropy_signal
            + self.structural_coverage
            + self.anomaly_strength
            + self.concern_relevance
    }
}

/// Rendering configuration for a custom profile.
#[derive(Debug, Clone, Deserialize)]
pub struct RenderingConfig {
    /// Output vocabulary style.
    #[serde(default)]
    pub vocabulary: Vocabulary,
    /// Whether to show JSON paths in the output.
    #[serde(default = "default_show_paths")]
    pub show_paths: bool,
    /// Number of repeated motifs before collapsing into a count.
    #[serde(default = "default_motif_collapse")]
    pub motif_collapse_threshold: usize,
    /// Maximum number of observations to include in rendered output.
    #[serde(default = "default_max_observations")]
    pub max_observations: usize,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            vocabulary: Vocabulary::default(),
            show_paths: default_show_paths(),
            motif_collapse_threshold: default_motif_collapse(),
            max_observations: default_max_observations(),
        }
    }
}

const fn default_show_paths() -> bool {
    true
}

const fn default_motif_collapse() -> usize {
    3
}

const fn default_max_observations() -> usize {
    50
}

/// Vocabulary style for rendered output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Vocabulary {
    /// Plain language, no paths shown.
    Plain,
    /// Technical language with paths.
    #[default]
    Technical,
    /// Formal full-sentence descriptions.
    Formal,
}

// ---------------------------------------------------------------------------
// CustomProfile — implements ConcernProfile
// ---------------------------------------------------------------------------

/// A custom concern profile loaded from TOML configuration.
///
/// Implements `ConcernProfile` using the parsed weights and rendering config.
#[derive(Debug, Clone)]
pub struct CustomProfile {
    /// The profile name (used for selection).
    profile_name: String,
    /// Description of what this profile is for.
    description: String,
    /// Score weights.
    weights: ScoreWeights,
    /// Rendering configuration.
    rendering: RenderingConfig,
}

impl CustomProfile {
    /// The description of this profile.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The rendering config for this profile.
    #[must_use]
    pub fn rendering_config(&self) -> &RenderingConfig {
        &self.rendering
    }
}

#[allow(clippy::unnecessary_literal_bound)]
impl ConcernProfile for CustomProfile {
    fn name(&self) -> &str {
        &self.profile_name
    }

    fn score(&self, candidate: &CandidateObservation) -> f64 {
        self.weights.score(candidate)
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
            OutputFormat::Markdown => Ok(render_markdown(essence, &self.profile_name)),
            _ => Ok(render_custom_text(essence, &self.rendering, &self.profile_name)),
        }
    }
}

// ---------------------------------------------------------------------------
// Custom text rendering respecting vocabulary and limits
// ---------------------------------------------------------------------------

fn render_custom_text(
    essence: &EssenceData,
    config: &RenderingConfig,
    profile_name: &str,
) -> String {
    let mut out = String::new();

    // Limit observations to max_observations
    let obs_limit = config.max_observations.min(essence.observations.len());
    let observations = &essence.observations[..obs_limit];

    match config.vocabulary {
        Vocabulary::Plain => {
            // Plain: narrative style, no paths
            out.push_str("Document Summary\n");
            out.push_str("================\n\n");
            let _ = write!(
                out,
                "This document has {} nodes across {} distinct paths, \
                 with a maximum depth of {}.\n\n",
                essence.total_nodes, essence.distinct_paths, essence.max_depth
            );

            out.push_str("Notable Findings\n");
            out.push_str("================\n\n");

            if observations.is_empty() {
                out.push_str("Nothing unusual was found in this document.\n");
            } else {
                let mut motif_count = 0usize;
                for obs in observations {
                    if obs.observation.description.contains("repeated structure") {
                        motif_count += 1;
                        if motif_count > config.motif_collapse_threshold {
                            continue;
                        }
                    }
                    // Plain: no paths, just descriptions
                    let _ = writeln!(out, "- {}", obs.observation.description);
                }
                if motif_count > config.motif_collapse_threshold {
                    let _ = writeln!(
                        out,
                        "- ...and {} more repeated structures",
                        motif_count - config.motif_collapse_threshold
                    );
                }
            }
        }
        Vocabulary::Technical => {
            // Technical: list-based with paths
            let _ = writeln!(out, "Essence ({profile_name})");
            let _ = writeln!(
                out,
                "nodes={} paths={} depth={}\n",
                essence.total_nodes, essence.distinct_paths, essence.max_depth
            );

            let mut motif_count = 0usize;
            for obs in observations {
                if obs.observation.description.contains("repeated structure") {
                    motif_count += 1;
                    if motif_count > config.motif_collapse_threshold {
                        continue;
                    }
                }
                if config.show_paths {
                    let _ = writeln!(
                        out,
                        "  [{:.4}] {} -- {}",
                        obs.score, obs.observation.path, obs.observation.description
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "  [{:.4}] {}",
                        obs.score, obs.observation.description
                    );
                }
            }
            if motif_count > config.motif_collapse_threshold {
                let _ = writeln!(
                    out,
                    "  ...and {} more repeated structures",
                    motif_count - config.motif_collapse_threshold
                );
            }
        }
        Vocabulary::Formal => {
            // Formal: full sentences with paths
            let _ = writeln!(out, "Profile: {profile_name}");
            let _ = writeln!(out, "========\n");
            let _ = writeln!(
                out,
                "The analyzed document comprises {} total nodes distributed across \
                 {} distinct structural paths, reaching a maximum nesting depth of {}.\n",
                essence.total_nodes, essence.distinct_paths, essence.max_depth
            );

            if observations.is_empty() {
                out.push_str(
                    "No significant observations were identified during analysis.\n",
                );
            } else {
                let _ = writeln!(out, "Observations:");
                let _ = writeln!(out, "-------------\n");
                let mut motif_count = 0usize;
                for (i, obs) in observations.iter().enumerate() {
                    if obs.observation.description.contains("repeated structure") {
                        motif_count += 1;
                        if motif_count > config.motif_collapse_threshold {
                            continue;
                        }
                    }
                    // Formal: full sentences, always show paths
                    let _ = writeln!(
                        out,
                        "{}. At path \"{}\", the analysis identified: {} (score: {:.4}).",
                        i + 1,
                        obs.observation.path,
                        obs.observation.description,
                        obs.score
                    );
                }
                if motif_count > config.motif_collapse_threshold {
                    let _ = writeln!(
                        out,
                        "\nAdditionally, {} further repeated structures were observed.",
                        motif_count - config.motif_collapse_threshold
                    );
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Loading functions
// ---------------------------------------------------------------------------

/// Parse TOML content and return custom profiles.
///
/// # Errors
/// Returns `VajraError::Config` if the TOML is malformed or missing required fields.
pub fn load_profiles_from_toml(content: &str) -> Result<Vec<CustomProfile>, VajraError> {
    let doc: TomlDocument = toml::from_str(content).map_err(|e| VajraError::Config {
        message: format!("TOML parse error: {e}"),
    })?;

    let mut profiles = Vec::new();

    for (_key, config) in &doc.profile {
        // Validate: name must not be empty
        if config.name.is_empty() {
            return Err(VajraError::Config {
                message: "profile name must not be empty".to_string(),
            });
        }

        // Warn (via eprintln) if weights don't sum to ~1.0, but don't fail
        let total = config.weights.total();
        if (total - 1.0).abs() > 0.01 {
            eprintln!(
                "vajra: warning: weights for profile '{}' sum to {:.4} (expected ~1.0)",
                config.name, total
            );
        }

        profiles.push(CustomProfile {
            profile_name: config.name.clone(),
            description: config.description.clone(),
            weights: config.weights.to_score_weights(),
            rendering: config.rendering.clone(),
        });
    }

    Ok(profiles)
}

/// Load custom profiles from a TOML file on disk.
///
/// # Errors
/// Returns `VajraError::Io` if the file cannot be read, or `VajraError::Config`
/// if the TOML is malformed.
pub fn load_profiles_from_file(path: &Path) -> Result<Vec<CustomProfile>, VajraError> {
    let content = std::fs::read_to_string(path).map_err(|e| VajraError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    load_profiles_from_toml(&content)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::{CandidateObservation, EssenceData, ScoredObservation};

    fn sample_toml() -> &'static str {
        r#"
[profile.my_review]
name = "my-review"
description = "Custom review profile"

[profile.my_review.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.my_review.rendering]
vocabulary = "plain"
show_paths = false
motif_collapse_threshold = 3
max_observations = 20
"#
    }

    fn make_essence(obs_count: usize) -> EssenceData {
        let observations: Vec<ScoredObservation> = (0..obs_count)
            .map(|i| ScoredObservation {
                observation: CandidateObservation {
                    path: format!("$.field_{i}"),
                    description: format!("observation {i}"),
                    rarity: 0.5,
                    instability: 0.3,
                    entropy_signal: 0.4,
                    structural_coverage: 0.2,
                    anomaly_strength: 0.1,
                    concern_relevance: 0.6,
                },
                score: 1.0 - (i as f64 * 0.05),
            })
            .collect();

        EssenceData {
            observations,
            document_identity: "test-doc".to_string(),
            total_nodes: 100,
            distinct_paths: 10,
            max_depth: 3,
            truncated: false,
            budget_used: None,
            budget_limit: None,
        }
    }

    #[test]
    fn valid_toml_parses_correctly() {
        let profiles = load_profiles_from_toml(sample_toml());
        assert!(profiles.is_ok(), "should parse valid TOML: {:?}", profiles.err());
        let profiles = profiles.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(profiles.len(), 1);

        let p = &profiles[0];
        assert_eq!(p.name(), "my-review");
        assert_eq!(p.description(), "Custom review profile");
        assert_eq!(p.rendering.vocabulary, Vocabulary::Plain);
        assert!(!p.rendering.show_paths);
        assert_eq!(p.rendering.motif_collapse_threshold, 3);
        assert_eq!(p.rendering.max_observations, 20);
    }

    #[test]
    fn missing_name_field_errors() {
        let toml = r#"
[profile.bad]
description = "no name"

[profile.bad.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20
"#;
        let result = load_profiles_from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("missing field") || message.contains("TOML parse error"),
                    "unexpected error: {message}"
                );
            }
            other => panic!("expected Config error, got: {other}"),
        }
    }

    #[test]
    fn missing_weights_field_errors() {
        let toml = r#"
[profile.bad]
name = "bad-profile"

[profile.bad.weights]
rarity = 0.15
"#;
        // Missing instability, entropy_signal, etc.
        let result = load_profiles_from_toml(toml);
        assert!(result.is_err());
    }

    #[test]
    fn weights_not_summing_to_one_is_warning_not_error() {
        let toml = r#"
[profile.wonky]
name = "wonky"
description = "weights off"

[profile.wonky.weights]
rarity = 0.50
instability = 0.50
entropy_signal = 0.50
structural_coverage = 0.50
anomaly_strength = 0.50
concern_relevance = 0.50
"#;
        // Sum = 3.0 — should warn but not error
        let profiles = load_profiles_from_toml(toml);
        assert!(profiles.is_ok(), "non-unit weights should not be an error");
        let profiles = profiles.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name(), "wonky");
    }

    #[test]
    fn custom_profile_scoring_works() {
        let profiles = load_profiles_from_toml(sample_toml())
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let candidate = CandidateObservation {
            path: "$.test".to_string(),
            description: "test".to_string(),
            rarity: 1.0,
            instability: 1.0,
            entropy_signal: 1.0,
            structural_coverage: 1.0,
            anomaly_strength: 1.0,
            concern_relevance: 1.0,
        };

        let score = profile.score(&candidate);
        // All signals at 1.0, weights sum to 1.0 => score should be 1.0
        assert!(
            (score - 1.0).abs() < 0.01,
            "expected score ~1.0 for all-ones candidate, got {score}"
        );

        let candidate2 = CandidateObservation {
            path: "$.test".to_string(),
            description: "test".to_string(),
            rarity: 0.0,
            instability: 0.0,
            entropy_signal: 0.0,
            structural_coverage: 0.0,
            anomaly_strength: 0.0,
            concern_relevance: 0.0,
        };

        let score2 = profile.score(&candidate2);
        assert!(
            score2.abs() < f64::EPSILON,
            "expected score 0.0 for all-zeros candidate, got {score2}"
        );
    }

    #[test]
    fn custom_profile_rendering_plain_vocabulary() {
        let profiles = load_profiles_from_toml(sample_toml())
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let essence = make_essence(5);
        let rendered = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        // Plain vocabulary: should have narrative sections
        assert!(rendered.contains("Document Summary"), "plain should have Document Summary");
        assert!(rendered.contains("Notable Findings"), "plain should have Notable Findings");
        // Plain: should NOT show paths in observations
        // The descriptions don't contain paths, so just check structure
        assert!(rendered.contains("observation 0"), "should have observation text");
    }

    #[test]
    fn custom_profile_rendering_technical_vocabulary() {
        let toml = r#"
[profile.tech]
name = "tech"

[profile.tech.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.tech.rendering]
vocabulary = "technical"
show_paths = true
max_observations = 50
"#;
        let profiles = load_profiles_from_toml(toml)
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let essence = make_essence(3);
        let rendered = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        // Technical: shows paths and scores
        assert!(rendered.contains("$.field_0"), "technical should show paths");
        assert!(rendered.contains("Essence (tech)"), "should show profile name header");
    }

    #[test]
    fn custom_profile_rendering_formal_vocabulary() {
        let toml = r#"
[profile.formal]
name = "formal"

[profile.formal.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.formal.rendering]
vocabulary = "formal"
max_observations = 50
"#;
        let profiles = load_profiles_from_toml(toml)
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let essence = make_essence(3);
        let rendered = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        // Formal: full sentences with "the analysis identified"
        assert!(rendered.contains("the analysis identified"), "formal should use full sentences");
        assert!(rendered.contains("Profile: formal"), "should show profile header");
    }

    #[test]
    fn max_observations_respected() {
        let toml = r#"
[profile.limited]
name = "limited"

[profile.limited.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20

[profile.limited.rendering]
vocabulary = "technical"
show_paths = true
max_observations = 3
"#;
        let profiles = load_profiles_from_toml(toml)
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let essence = make_essence(10);
        let rendered = profile
            .render(&essence, OutputFormat::Text)
            .unwrap_or_else(|e| panic!("{e}"));

        // Should only have 3 observations
        assert!(rendered.contains("observation 0"));
        assert!(rendered.contains("observation 2"));
        // observation 3 should NOT be in output
        assert!(!rendered.contains("observation 3"), "should respect max_observations=3");
    }

    #[test]
    fn json_rendering_works_for_custom_profile() {
        let profiles = load_profiles_from_toml(sample_toml())
            .unwrap_or_else(|e| panic!("{e}"));
        let profile = &profiles[0];

        let essence = make_essence(3);
        let json_str = profile
            .render(&essence, OutputFormat::Json)
            .unwrap_or_else(|e| panic!("{e}"));

        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).unwrap_or_else(|e| panic!("{e}"));
        assert!(parsed.is_object());
        assert!(parsed.get("identity").is_some());
        assert!(parsed.get("observations").is_some());
    }

    #[test]
    fn multiple_profiles_in_one_file() {
        let toml = r#"
[profile.alpha]
name = "alpha"
description = "First profile"

[profile.alpha.weights]
rarity = 0.20
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.20
concern_relevance = 0.20

[profile.beta]
name = "beta"
description = "Second profile"

[profile.beta.weights]
rarity = 0.10
instability = 0.10
entropy_signal = 0.30
structural_coverage = 0.10
anomaly_strength = 0.10
concern_relevance = 0.30
"#;
        let profiles = load_profiles_from_toml(toml)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(profiles.len(), 2);
        let names: Vec<&str> = profiles.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn empty_toml_returns_no_profiles() {
        let profiles = load_profiles_from_toml("")
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(profiles.is_empty());
    }

    #[test]
    fn invalid_toml_syntax_errors() {
        let result = load_profiles_from_toml("this is not valid [[[toml");
        assert!(result.is_err());
    }

    #[test]
    fn default_rendering_config() {
        let toml = r#"
[profile.minimal]
name = "minimal"

[profile.minimal.weights]
rarity = 0.15
instability = 0.20
entropy_signal = 0.10
structural_coverage = 0.10
anomaly_strength = 0.25
concern_relevance = 0.20
"#;
        let profiles = load_profiles_from_toml(toml)
            .unwrap_or_else(|e| panic!("{e}"));
        let p = &profiles[0];
        // Should use defaults
        assert_eq!(p.rendering.vocabulary, Vocabulary::Technical);
        assert!(p.rendering.show_paths);
        assert_eq!(p.rendering.motif_collapse_threshold, 3);
        assert_eq!(p.rendering.max_observations, 50);
    }
}
