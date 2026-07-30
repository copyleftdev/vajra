//! Scoring weights, composite score computation, and health grading.

use std::collections::BTreeMap;
use std::fmt;

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

    /// Health profile: optimized for repository/project health assessment.
    ///
    /// Emphasizes contributor diversity (entropy) and domain-specific health
    /// patterns (concern relevance), with strong anomaly detection.
    #[must_use]
    pub fn health() -> Self {
        Self {
            entropy_signal: 0.25,
            concern_relevance: 0.25,
            anomaly_strength: 0.20,
            rarity: 0.15,
            instability: 0.10,
            structural_coverage: 0.05,
        }
    }
}

// ---------------------------------------------------------------------------
// Health scoring: letter grades and dimension-level health assessment
// ---------------------------------------------------------------------------

/// Letter grades for health scoring dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LetterGrade {
    F,
    DPlus,
    D,
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
}

impl LetterGrade {
    /// Convert a letter grade to its numeric value on a 0.0-4.0 scale.
    #[must_use]
    pub fn to_numeric(self) -> f64 {
        match self {
            Self::A => 4.0,
            Self::AMinus => 3.7,
            Self::BPlus => 3.3,
            Self::B => 3.0,
            Self::BMinus => 2.7,
            Self::CPlus => 2.3,
            Self::C => 2.0,
            Self::CMinus => 1.7,
            Self::DPlus => 1.3,
            Self::D => 1.0,
            Self::F => 0.0,
        }
    }

    /// Convert a numeric value (0.0-4.0 scale) to a letter grade.
    #[must_use]
    pub fn from_numeric(value: f64) -> Self {
        match () {
            _ if value >= 3.85 => Self::A,
            _ if value >= 3.5 => Self::AMinus,
            _ if value >= 3.15 => Self::BPlus,
            _ if value >= 2.85 => Self::B,
            _ if value >= 2.5 => Self::BMinus,
            _ if value >= 2.15 => Self::CPlus,
            _ if value >= 1.85 => Self::C,
            _ if value >= 1.5 => Self::CMinus,
            _ if value >= 1.15 => Self::DPlus,
            _ if value >= 0.5 => Self::D,
            _ => Self::F,
        }
    }

    /// Return the display string for this grade.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::AMinus => "A-",
            Self::BPlus => "B+",
            Self::B => "B",
            Self::BMinus => "B-",
            Self::CPlus => "C+",
            Self::C => "C",
            Self::CMinus => "C-",
            Self::DPlus => "D+",
            Self::D => "D",
            Self::F => "F",
        }
    }
}

impl fmt::Display for LetterGrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Score for a single health dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    /// Letter grade for this dimension.
    pub grade: LetterGrade,
    /// Raw metric value used for grading.
    pub value: f64,
    /// Name of the underlying metric.
    pub metric_name: String,
    /// Human-readable description of what this dimension measures.
    pub description: String,
}

/// Overall health score combining multiple dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    /// Overall letter grade.
    pub overall: LetterGrade,
    /// Overall numeric score on 0.0-4.0 scale.
    pub overall_numeric: f64,
    /// Per-dimension scores, keyed by dimension name.
    pub dimensions: BTreeMap<String, DimensionScore>,
}

/// Weights for the five health scoring dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthWeights {
    pub bus_factor: f64,
    pub contribution_diversity: f64,
    pub code_stability: f64,
    pub contributor_retention: f64,
    pub velocity_trend: f64,
    pub issue_response: f64,
}

impl Default for HealthWeights {
    fn default() -> Self {
        Self {
            bus_factor: 0.15,
            contribution_diversity: 0.10,
            code_stability: 0.20,
            contributor_retention: 0.20,
            velocity_trend: 0.15,
            issue_response: 0.20,
        }
    }
}

impl HealthWeights {
    /// Returns the sum of all weights (should be ~1.0).
    #[must_use]
    pub fn total(&self) -> f64 {
        self.bus_factor
            + self.contribution_diversity
            + self.code_stability
            + self.contributor_retention
            + self.velocity_trend
            + self.issue_response
    }
}

// ---------------------------------------------------------------------------
// Grading rubrics: deterministic threshold-based grading
// ---------------------------------------------------------------------------

/// Grade commit entropy (Shannon entropy of author distribution).
///
/// Higher entropy means more diverse contributions (better bus factor).
#[must_use]
pub fn grade_commit_entropy(entropy: f64) -> LetterGrade {
    match () {
        _ if entropy > 5.0 => LetterGrade::A,
        _ if entropy > 4.5 => LetterGrade::AMinus,
        _ if entropy > 4.0 => LetterGrade::BPlus,
        _ if entropy > 3.5 => LetterGrade::B,
        _ if entropy > 3.0 => LetterGrade::C,
        _ if entropy > 2.5 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Grade the top contributor's share of commits — the bus factor proper.
///
/// Graded instead of author entropy, which is dominated by the long tail of
/// one-commit contributors and so is nearly blind to concentration at the top:
/// a repository where one person authored 52% of commits and 75% of
/// contributors committed exactly once scored B on entropy while its true
/// `bus_factor_50` was 1. Lower is better. See #89.
#[must_use]
pub fn grade_top1_share(share: f64) -> LetterGrade {
    match () {
        _ if share < 0.15 => LetterGrade::A,
        _ if share < 0.20 => LetterGrade::AMinus,
        _ if share < 0.25 => LetterGrade::BPlus,
        _ if share < 0.30 => LetterGrade::B,
        _ if share < 0.35 => LetterGrade::BMinus,
        _ if share < 0.40 => LetterGrade::C,
        _ if share < 0.50 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Grade fix ratio (fix commits / total commits).
///
/// Lower fix ratio means more stable code.
#[must_use]
pub fn grade_fix_ratio(ratio: f64) -> LetterGrade {
    match () {
        _ if ratio < 0.05 => LetterGrade::A,
        _ if ratio < 0.10 => LetterGrade::B,
        _ if ratio < 0.15 => LetterGrade::C,
        _ if ratio < 0.20 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Grade one-commit contributor rate.
///
/// Lower one-commit rate means better contributor retention.
#[must_use]
pub fn grade_one_commit_rate(rate: f64) -> LetterGrade {
    match () {
        _ if rate < 0.20 => LetterGrade::A,
        _ if rate < 0.35 => LetterGrade::B,
        _ if rate < 0.50 => LetterGrade::C,
        _ if rate < 0.65 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Grade velocity trend from a slope value.
///
/// Positive slope means increasing velocity (good).
/// The slope is normalized: values near zero are stable.
#[must_use]
pub fn grade_velocity_trend(slope: f64) -> LetterGrade {
    match () {
        _ if slope > 2.0 => LetterGrade::A,
        _ if slope > 1.0 => LetterGrade::BPlus,
        _ if slope > 0.0 => LetterGrade::B,
        _ if slope > -0.5 => LetterGrade::C,
        _ if slope > -1.0 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Grade zero-comment issue rate.
///
/// Lower zero-comment rate means better issue responsiveness.
#[must_use]
pub fn grade_zero_comment_rate(rate: f64) -> LetterGrade {
    match () {
        _ if rate < 0.10 => LetterGrade::A,
        _ if rate < 0.20 => LetterGrade::B,
        _ if rate < 0.35 => LetterGrade::C,
        _ if rate < 0.50 => LetterGrade::D,
        _ => LetterGrade::F,
    }
}

/// Input metrics for computing a health score.
///
/// Each field is `Option` so that scoring works with partial data.
/// Missing dimensions are excluded from the weighted average.
#[derive(Debug, Clone, Default)]
pub struct HealthMetrics {
    /// Shannon entropy of the commit author distribution.
    pub commit_entropy: Option<f64>,
    /// Share of commits authored by the single most active contributor.
    pub top1_share: Option<f64>,
    /// Fix commits / total commits.
    pub fix_ratio: Option<f64>,
    /// Fraction of contributors with exactly one commit.
    pub one_commit_rate: Option<f64>,
    /// Slope from linear regression of commit counts over time windows.
    pub velocity_slope: Option<f64>,
    /// Fraction of issues with zero comments.
    pub zero_comment_rate: Option<f64>,
}

/// Compute the overall health score from the provided metrics and weights.
///
/// Dimensions with `None` values are excluded from the weighted average,
/// and their weight is redistributed proportionally among available dimensions.
/// Returns `None` if no dimensions have data.
#[must_use]
pub fn compute_health_score(
    metrics: &HealthMetrics,
    weights: &HealthWeights,
) -> Option<HealthScore> {
    let mut dimensions = BTreeMap::new();
    let mut weighted_sum = 0.0_f64;
    let mut total_weight = 0.0_f64;

    if let Some(share) = metrics.top1_share {
        let grade = grade_top1_share(share);
        dimensions.insert(
            "bus_factor".to_owned(),
            DimensionScore {
                grade,
                value: share,
                metric_name: "top1_share".to_owned(),
                description: "Continuity risk measured by the top contributor's share of commits"
                    .to_owned(),
            },
        );
        weighted_sum += weights.bus_factor * grade.to_numeric();
        total_weight += weights.bus_factor;
    }

    if let Some(entropy) = metrics.commit_entropy {
        let grade = grade_commit_entropy(entropy);
        dimensions.insert(
            "contribution_diversity".to_owned(),
            DimensionScore {
                grade,
                value: entropy,
                metric_name: "commit_entropy".to_owned(),
                description: "Breadth of contribution measured by commit author entropy".to_owned(),
            },
        );
        weighted_sum += weights.contribution_diversity * grade.to_numeric();
        total_weight += weights.contribution_diversity;
    }

    if let Some(ratio) = metrics.fix_ratio {
        let grade = grade_fix_ratio(ratio);
        dimensions.insert(
            "code_stability".to_owned(),
            DimensionScore {
                grade,
                value: ratio,
                metric_name: "fix_ratio".to_owned(),
                description: "Code stability measured by fix commit ratio".to_owned(),
            },
        );
        weighted_sum += weights.code_stability * grade.to_numeric();
        total_weight += weights.code_stability;
    }

    if let Some(rate) = metrics.one_commit_rate {
        let grade = grade_one_commit_rate(rate);
        dimensions.insert(
            "contributor_retention".to_owned(),
            DimensionScore {
                grade,
                value: rate,
                metric_name: "one_commit_rate".to_owned(),
                description: "Contributor retention measured by one-commit contributor rate"
                    .to_owned(),
            },
        );
        weighted_sum += weights.contributor_retention * grade.to_numeric();
        total_weight += weights.contributor_retention;
    }

    if let Some(slope) = metrics.velocity_slope {
        let grade = grade_velocity_trend(slope);
        dimensions.insert(
            "velocity_trend".to_owned(),
            DimensionScore {
                grade,
                value: slope,
                metric_name: "velocity_slope".to_owned(),
                description: "Development velocity trend from commit frequency over time"
                    .to_owned(),
            },
        );
        weighted_sum += weights.velocity_trend * grade.to_numeric();
        total_weight += weights.velocity_trend;
    }

    if let Some(rate) = metrics.zero_comment_rate {
        let grade = grade_zero_comment_rate(rate);
        dimensions.insert(
            "issue_response".to_owned(),
            DimensionScore {
                grade,
                value: rate,
                metric_name: "zero_comment_rate".to_owned(),
                description: "Issue responsiveness measured by zero-comment issue rate".to_owned(),
            },
        );
        weighted_sum += weights.issue_response * grade.to_numeric();
        total_weight += weights.issue_response;
    }

    if total_weight < f64::EPSILON {
        return None;
    }

    let overall_numeric = weighted_sum / total_weight;
    let overall = LetterGrade::from_numeric(overall_numeric);

    Some(HealthScore {
        overall,
        overall_numeric,
        dimensions,
    })
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
            ScoreWeights::health(),
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
        assert!(
            (score - 0.7).abs() < 1e-10,
            "expected 0.5*0.8 + 0.5*0.6 = 0.7, got {score}"
        );
    }

    // ---- LetterGrade ----

    #[test]
    fn letter_grade_display() {
        assert_eq!(LetterGrade::A.to_string(), "A");
        assert_eq!(LetterGrade::AMinus.to_string(), "A-");
        assert_eq!(LetterGrade::BPlus.to_string(), "B+");
        assert_eq!(LetterGrade::B.to_string(), "B");
        assert_eq!(LetterGrade::BMinus.to_string(), "B-");
        assert_eq!(LetterGrade::CPlus.to_string(), "C+");
        assert_eq!(LetterGrade::C.to_string(), "C");
        assert_eq!(LetterGrade::CMinus.to_string(), "C-");
        assert_eq!(LetterGrade::DPlus.to_string(), "D+");
        assert_eq!(LetterGrade::D.to_string(), "D");
        assert_eq!(LetterGrade::F.to_string(), "F");
    }

    #[test]
    fn letter_grade_numeric_roundtrip() {
        let grades = [
            LetterGrade::A,
            LetterGrade::AMinus,
            LetterGrade::BPlus,
            LetterGrade::B,
            LetterGrade::BMinus,
            LetterGrade::CPlus,
            LetterGrade::C,
            LetterGrade::CMinus,
            LetterGrade::DPlus,
            LetterGrade::D,
            LetterGrade::F,
        ];
        for grade in &grades {
            let numeric = grade.to_numeric();
            assert!(
                (0.0..=4.0).contains(&numeric),
                "grade {grade} numeric {numeric} out of range"
            );
            let back = LetterGrade::from_numeric(numeric);
            assert_eq!(
                *grade, back,
                "roundtrip failed: {grade} -> {numeric} -> {back}"
            );
        }
    }

    #[test]
    fn from_numeric_boundary_values() {
        assert_eq!(LetterGrade::from_numeric(4.0), LetterGrade::A);
        assert_eq!(LetterGrade::from_numeric(3.85), LetterGrade::A);
        assert_eq!(LetterGrade::from_numeric(3.84), LetterGrade::AMinus);
        assert_eq!(LetterGrade::from_numeric(0.49), LetterGrade::F);
        assert_eq!(LetterGrade::from_numeric(0.0), LetterGrade::F);
        assert_eq!(LetterGrade::from_numeric(0.5), LetterGrade::D);
    }

    // ---- Individual grading rubrics ----

    #[test]
    fn grade_commit_entropy_thresholds() {
        assert_eq!(grade_commit_entropy(6.0), LetterGrade::A);
        assert_eq!(grade_commit_entropy(5.1), LetterGrade::A);
        assert_eq!(grade_commit_entropy(4.6), LetterGrade::AMinus);
        assert_eq!(grade_commit_entropy(4.1), LetterGrade::BPlus);
        assert_eq!(grade_commit_entropy(3.6), LetterGrade::B);
        assert_eq!(grade_commit_entropy(3.1), LetterGrade::C);
        assert_eq!(grade_commit_entropy(2.6), LetterGrade::D);
        assert_eq!(grade_commit_entropy(2.0), LetterGrade::F);
        assert_eq!(grade_commit_entropy(0.0), LetterGrade::F);
    }

    #[test]
    fn grade_top1_share_thresholds() {
        assert_eq!(grade_top1_share(0.10), LetterGrade::A);
        assert_eq!(grade_top1_share(0.18), LetterGrade::AMinus);
        assert_eq!(grade_top1_share(0.22), LetterGrade::BPlus);
        assert_eq!(grade_top1_share(0.28), LetterGrade::B);
        assert_eq!(grade_top1_share(0.33), LetterGrade::BMinus);
        assert_eq!(grade_top1_share(0.38), LetterGrade::C);
        assert_eq!(grade_top1_share(0.45), LetterGrade::D);
        assert_eq!(grade_top1_share(0.50), LetterGrade::F);
        assert_eq!(grade_top1_share(1.00), LetterGrade::F);
    }

    /// The case from #89: one contributor authored 52% of commits while 60
    /// others committed once each. Entropy over that tail stays high, so the
    /// old entropy-graded `bus_factor` returned B for a repository whose real
    /// `bus_factor_50` was 1.
    #[test]
    fn a_dominant_author_cannot_grade_well_on_bus_factor() {
        let total = 125.0_f64;
        let metrics = HealthMetrics {
            // 65 of 125 commits by one person, 60 singletons.
            top1_share: Some(65.0 / total),
            commit_entropy: Some(3.68),
            ..HealthMetrics::default()
        };
        let score =
            compute_health_score(&metrics, &HealthWeights::default()).expect("scorable dimensions");

        let bus = score
            .dimensions
            .get("bus_factor")
            .expect("bus_factor dimension");
        assert_eq!(bus.metric_name, "top1_share");
        assert!(
            bus.grade.to_numeric() <= LetterGrade::C.to_numeric(),
            "52% single-author concentration graded {:?}",
            bus.grade
        );

        // The entropy signal is not lost, only renamed to what it measures.
        let diversity = score
            .dimensions
            .get("contribution_diversity")
            .expect("contribution_diversity dimension");
        assert_eq!(diversity.metric_name, "commit_entropy");
        assert_eq!(diversity.grade, LetterGrade::B);
    }

    #[test]
    fn an_evenly_shared_project_still_grades_well_on_bus_factor() {
        let metrics = HealthMetrics {
            top1_share: Some(0.12),
            commit_entropy: Some(5.2),
            ..HealthMetrics::default()
        };
        let score =
            compute_health_score(&metrics, &HealthWeights::default()).expect("scorable dimensions");
        assert_eq!(
            score
                .dimensions
                .get("bus_factor")
                .expect("bus_factor")
                .grade,
            LetterGrade::A
        );
    }

    #[test]
    fn default_weights_still_sum_to_one() {
        let total = HealthWeights::default().total();
        assert!(
            (total - 1.0).abs() < 1e-9,
            "weights must sum to 1.0, got {total}"
        );
    }

    #[test]
    fn grade_fix_ratio_thresholds() {
        assert_eq!(grade_fix_ratio(0.01), LetterGrade::A);
        assert_eq!(grade_fix_ratio(0.04), LetterGrade::A);
        assert_eq!(grade_fix_ratio(0.05), LetterGrade::B);
        assert_eq!(grade_fix_ratio(0.09), LetterGrade::B);
        assert_eq!(grade_fix_ratio(0.10), LetterGrade::C);
        assert_eq!(grade_fix_ratio(0.14), LetterGrade::C);
        assert_eq!(grade_fix_ratio(0.15), LetterGrade::D);
        assert_eq!(grade_fix_ratio(0.19), LetterGrade::D);
        assert_eq!(grade_fix_ratio(0.20), LetterGrade::F);
        assert_eq!(grade_fix_ratio(0.50), LetterGrade::F);
    }

    #[test]
    fn grade_one_commit_rate_thresholds() {
        assert_eq!(grade_one_commit_rate(0.10), LetterGrade::A);
        assert_eq!(grade_one_commit_rate(0.19), LetterGrade::A);
        assert_eq!(grade_one_commit_rate(0.20), LetterGrade::B);
        assert_eq!(grade_one_commit_rate(0.34), LetterGrade::B);
        assert_eq!(grade_one_commit_rate(0.35), LetterGrade::C);
        assert_eq!(grade_one_commit_rate(0.49), LetterGrade::C);
        assert_eq!(grade_one_commit_rate(0.50), LetterGrade::D);
        assert_eq!(grade_one_commit_rate(0.65), LetterGrade::F);
        assert_eq!(grade_one_commit_rate(0.90), LetterGrade::F);
    }

    #[test]
    fn grade_velocity_trend_thresholds() {
        assert_eq!(grade_velocity_trend(3.0), LetterGrade::A);
        assert_eq!(grade_velocity_trend(1.5), LetterGrade::BPlus);
        assert_eq!(grade_velocity_trend(0.5), LetterGrade::B);
        assert_eq!(grade_velocity_trend(-0.2), LetterGrade::C);
        assert_eq!(grade_velocity_trend(-0.7), LetterGrade::D);
        assert_eq!(grade_velocity_trend(-2.0), LetterGrade::F);
    }

    #[test]
    fn grade_zero_comment_rate_thresholds() {
        assert_eq!(grade_zero_comment_rate(0.05), LetterGrade::A);
        assert_eq!(grade_zero_comment_rate(0.15), LetterGrade::B);
        assert_eq!(grade_zero_comment_rate(0.25), LetterGrade::C);
        assert_eq!(grade_zero_comment_rate(0.45), LetterGrade::D);
        assert_eq!(grade_zero_comment_rate(0.60), LetterGrade::F);
    }

    // ---- Overall health score ----

    #[test]
    fn health_score_perfect() {
        let metrics = HealthMetrics {
            top1_share: None,
            commit_entropy: Some(6.0),
            fix_ratio: Some(0.01),
            one_commit_rate: Some(0.10),
            velocity_slope: Some(3.0),
            zero_comment_rate: Some(0.05),
        };
        let weights = HealthWeights::default();
        let score = compute_health_score(&metrics, &weights);
        assert!(score.is_some());
        let score = score.unwrap_or_else(|| HealthScore {
            overall: LetterGrade::F,
            overall_numeric: 0.0,
            dimensions: BTreeMap::new(),
        });
        assert_eq!(score.overall, LetterGrade::A);
        assert!((score.overall_numeric - 4.0).abs() < 1e-10);
        assert_eq!(score.dimensions.len(), 5);
    }

    #[test]
    fn health_score_worst() {
        let metrics = HealthMetrics {
            top1_share: None,
            commit_entropy: Some(0.0),
            fix_ratio: Some(0.50),
            one_commit_rate: Some(0.90),
            velocity_slope: Some(-2.0),
            zero_comment_rate: Some(0.80),
        };
        let weights = HealthWeights::default();
        let score = compute_health_score(&metrics, &weights);
        assert!(score.is_some());
        let score = score.unwrap_or_else(|| HealthScore {
            overall: LetterGrade::A,
            overall_numeric: 4.0,
            dimensions: BTreeMap::new(),
        });
        assert_eq!(score.overall, LetterGrade::F);
        assert!((score.overall_numeric - 0.0).abs() < 1e-10);
        assert_eq!(score.dimensions.len(), 5);
    }

    #[test]
    fn health_score_empty_input() {
        let metrics = HealthMetrics::default();
        let weights = HealthWeights::default();
        let score = compute_health_score(&metrics, &weights);
        assert!(score.is_none());
    }

    #[test]
    fn health_score_partial_data() {
        let metrics = HealthMetrics {
            top1_share: None,
            commit_entropy: Some(4.5),
            fix_ratio: Some(0.08),
            one_commit_rate: None,
            velocity_slope: None,
            zero_comment_rate: None,
        };
        let weights = HealthWeights::default();
        let score = compute_health_score(&metrics, &weights);
        assert!(score.is_some());
        let score = score.unwrap_or_else(|| HealthScore {
            overall: LetterGrade::F,
            overall_numeric: 0.0,
            dimensions: BTreeMap::new(),
        });
        assert_eq!(score.dimensions.len(), 2);
        // commit_entropy=4.5 grades to BPlus (3.3); it scores contribution_diversity,
        // weight 0.10. bus_factor is absent because top1_share is None.
        // fix_ratio=0.08 grades to B (3.0) since threshold is < 0.10
        let expected = (0.10 * 3.3 + 0.20 * 3.0) / (0.10 + 0.20);
        assert!(
            (score.overall_numeric - expected).abs() < 1e-10,
            "expected {expected}, got {}",
            score.overall_numeric
        );
    }

    #[test]
    fn health_score_mixed_grades() {
        let metrics = HealthMetrics {
            top1_share: Some(0.28),
            commit_entropy: Some(3.6),
            fix_ratio: Some(0.12),
            one_commit_rate: Some(0.40),
            velocity_slope: Some(0.5),
            zero_comment_rate: Some(0.30),
        };
        let weights = HealthWeights::default();
        let score = compute_health_score(&metrics, &weights);
        assert!(score.is_some());
        let score = score.unwrap_or_else(|| HealthScore {
            overall: LetterGrade::F,
            overall_numeric: 0.0,
            dimensions: BTreeMap::new(),
        });
        // top1_share=0.28 -> B (3.0), commit_entropy=3.6 -> B (3.0),
        // fix_ratio=0.12 -> C (2.0), one_commit_rate=0.40 -> C (2.0),
        // velocity_slope=0.5 -> B (3.0), zero_comment_rate=0.30 -> C (2.0)
        let expected = 0.15 * 3.0 + 0.10 * 3.0 + 0.20 * 2.0 + 0.20 * 2.0 + 0.15 * 3.0 + 0.20 * 2.0;
        assert!(
            (score.overall_numeric - expected).abs() < 1e-10,
            "expected {expected}, got {}",
            score.overall_numeric
        );
    }

    #[test]
    fn health_weights_default_sum_to_one() {
        let weights = HealthWeights::default();
        let total = weights.total();
        assert!(
            (total - 1.0).abs() < 1e-10,
            "default health weights sum to {total}, expected 1.0"
        );
    }

    #[test]
    fn all_grades_are_valid_variants() {
        let all_grades = [
            LetterGrade::A,
            LetterGrade::AMinus,
            LetterGrade::BPlus,
            LetterGrade::B,
            LetterGrade::BMinus,
            LetterGrade::CPlus,
            LetterGrade::C,
            LetterGrade::CMinus,
            LetterGrade::DPlus,
            LetterGrade::D,
            LetterGrade::F,
        ];
        for grade in &all_grades {
            let n = grade.to_numeric();
            assert!((0.0..=4.0).contains(&n), "grade {grade} out of range: {n}");
            assert!(!grade.as_str().is_empty());
        }
    }

    #[test]
    fn overall_numeric_in_valid_range() {
        let test_cases = [
            (6.0, 0.01, 0.10, 3.0, 0.05),
            (0.0, 0.50, 0.90, -2.0, 0.80),
            (3.6, 0.12, 0.40, 0.5, 0.30),
            (5.5, 0.03, 0.55, -0.8, 0.15),
        ];
        let weights = HealthWeights::default();
        for (ent, fix, ocr, slope, zcr) in &test_cases {
            let metrics = HealthMetrics {
                top1_share: None,
                commit_entropy: Some(*ent),
                fix_ratio: Some(*fix),
                one_commit_rate: Some(*ocr),
                velocity_slope: Some(*slope),
                zero_comment_rate: Some(*zcr),
            };
            let score = compute_health_score(&metrics, &weights);
            assert!(score.is_some());
            let score = score.unwrap_or_else(|| HealthScore {
                overall: LetterGrade::F,
                overall_numeric: 0.0,
                dimensions: BTreeMap::new(),
            });
            assert!(
                (0.0..=4.0).contains(&score.overall_numeric),
                "overall_numeric out of range: {}",
                score.overall_numeric
            );
        }
    }
}
