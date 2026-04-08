//! Scoring weights and composite score computation.

use crate::traits::CandidateObservation;
use serde::{Deserialize, Serialize};

/// Weight vector for the six scoring dimensions.
///
/// Weights are normalized to sum to 1.0. Each profile defines
/// its own weight vector to emphasize different signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub rarity: f64,
    pub instability: f64,
    pub entropy_signal: f64,
    pub structural_coverage: f64,
    pub anomaly_strength: f64,
    pub concern_relevance: f64,
}

impl ScoreWeights {
    /// Compute the composite score for a candidate observation.
    #[must_use]
    pub fn score(&self, obs: &CandidateObservation) -> f64 {
        self.rarity * obs.rarity
            + self.instability * obs.instability
            + self.entropy_signal * obs.entropy_signal
            + self.structural_coverage * obs.structural_coverage
            + self.anomaly_strength * obs.anomaly_strength
            + self.concern_relevance * obs.concern_relevance
    }

    /// Returns the sum of all weights (should be ~1.0).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.rarity
            + self.instability
            + self.entropy_signal
            + self.structural_coverage
            + self.anomaly_strength
            + self.concern_relevance
    }

    /// Engineer profile: balanced across all dimensions.
    #[must_use]
    pub fn engineer() -> Self {
        Self {
            rarity: 0.15,
            instability: 0.25,
            entropy_signal: 0.15,
            structural_coverage: 0.15,
            anomaly_strength: 0.15,
            concern_relevance: 0.15,
        }
    }

    /// Staff profile: emphasizes anomalies and structural coverage.
    #[must_use]
    pub fn staff() -> Self {
        Self {
            rarity: 0.10,
            instability: 0.05,
            entropy_signal: 0.10,
            structural_coverage: 0.25,
            anomaly_strength: 0.30,
            concern_relevance: 0.20,
        }
    }

    /// Auditor profile: emphasizes instability and concern relevance.
    #[must_use]
    pub fn auditor() -> Self {
        Self {
            rarity: 0.10,
            instability: 0.20,
            entropy_signal: 0.10,
            structural_coverage: 0.10,
            anomaly_strength: 0.20,
            concern_relevance: 0.30,
        }
    }

    /// AI handoff profile: emphasizes entropy and coverage.
    #[must_use]
    pub fn ai() -> Self {
        Self {
            rarity: 0.15,
            instability: 0.10,
            entropy_signal: 0.20,
            structural_coverage: 0.20,
            anomaly_strength: 0.20,
            concern_relevance: 0.15,
        }
    }

    /// Fraud profile: emphasizes anomaly strength and rarity.
    #[must_use]
    pub fn fraud() -> Self {
        Self {
            rarity: 0.25,
            instability: 0.10,
            entropy_signal: 0.10,
            structural_coverage: 0.05,
            anomaly_strength: 0.35,
            concern_relevance: 0.15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_weights_sum_to_one() {
        let profiles = [
            ScoreWeights::engineer(),
            ScoreWeights::staff(),
            ScoreWeights::auditor(),
            ScoreWeights::ai(),
            ScoreWeights::fraud(),
        ];
        for profile in &profiles {
            let total = profile.total();
            assert!(
                (total - 1.0).abs() < 1e-10,
                "weights sum to {total}, expected 1.0"
            );
        }
    }

    #[test]
    fn score_is_weighted_sum() {
        let weights = ScoreWeights {
            rarity: 0.5,
            instability: 0.0,
            entropy_signal: 0.0,
            structural_coverage: 0.0,
            anomaly_strength: 0.5,
            concern_relevance: 0.0,
        };
        let obs = CandidateObservation {
            path: String::new(),
            description: String::new(),
            rarity: 0.8,
            instability: 1.0,
            entropy_signal: 1.0,
            structural_coverage: 1.0,
            anomaly_strength: 0.6,
            concern_relevance: 1.0,
        };
        let score = weights.score(&obs);
        assert!((score - 0.7).abs() < 1e-10, "expected 0.5*0.8 + 0.5*0.6 = 0.7, got {score}");
    }
}
