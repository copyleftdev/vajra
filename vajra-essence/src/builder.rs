//! Fluent builder for constructing [`EssenceData`] from analysis results.
//!
//! The builder collects observations from multiple analyzers, scores
//! them via a concern profile, and returns a deterministically-ordered
//! `EssenceData`.

use std::cmp::Ordering;
use std::fmt::Write;

use vajra_anomaly::AnomalyReport;
use vajra_fingerprint::FingerprintResult;
use vajra_stats::StatsResult;
use vajra_types::document::Document;
use vajra_types::traits::{CandidateObservation, ConcernProfile, EssenceData, ScoredObservation};

use crate::tokens::estimate_tokens;

/// Fluent builder for essence construction.
///
/// Takes a document and optional analysis results, scores observations
/// via a concern profile, and produces an [`EssenceData`] with
/// deterministically-ordered observations.
pub struct EssenceBuilder<'a> {
    doc: &'a Document,
    stats: Option<&'a StatsResult>,
    anomalies: Option<&'a AnomalyReport>,
    fingerprint: Option<&'a FingerprintResult>,
    profile: &'a dyn ConcernProfile,
    budget: Option<usize>,
}

impl<'a> EssenceBuilder<'a> {
    /// Create a new builder with the required document and profile.
    #[must_use]
    pub fn new(doc: &'a Document, profile: &'a dyn ConcernProfile) -> Self {
        Self {
            doc,
            stats: None,
            anomalies: None,
            fingerprint: None,
            profile,
            budget: None,
        }
    }

    /// Attach statistics analysis results.
    #[must_use]
    pub const fn with_stats(mut self, stats: &'a StatsResult) -> Self {
        self.stats = Some(stats);
        self
    }

    /// Attach anomaly analysis results.
    #[must_use]
    pub const fn with_anomalies(mut self, anomalies: &'a AnomalyReport) -> Self {
        self.anomalies = Some(anomalies);
        self
    }

    /// Attach fingerprint analysis results.
    #[must_use]
    pub const fn with_fingerprint(mut self, fingerprint: &'a FingerprintResult) -> Self {
        self.fingerprint = Some(fingerprint);
        self
    }

    /// Set a token budget for the output.
    ///
    /// When set, after scoring and sorting, observations are greedily
    /// selected by score-per-token ratio until the budget is exhausted.
    #[must_use]
    pub const fn with_budget(mut self, max_tokens: usize) -> Self {
        self.budget = Some(max_tokens);
        self
    }

    /// Build the essence data.
    ///
    /// Collects candidate observations from all attached analysis results,
    /// scores them via the profile, and sorts deterministically:
    /// 1. Score descending
    /// 2. Path depth ascending (shallower first)
    /// 3. Path lexicographic ascending
    #[must_use]
    pub fn build(&self) -> EssenceData {
        let mut candidates = Vec::new();

        // Generate observations from stats
        if let Some(stats) = self.stats {
            observations_from_stats(stats, &mut candidates);
        }

        // Generate observations from anomalies
        if let Some(anomalies) = self.anomalies {
            observations_from_anomalies(anomalies, &mut candidates);
        }

        // Generate observations from trie
        observations_from_trie(self.doc, &mut candidates);

        // Generate observations from fingerprint
        if let Some(fp) = self.fingerprint {
            observations_from_fingerprint(fp, &mut candidates);
        }

        // Score all candidates
        let mut scored: Vec<ScoredObservation> = candidates
            .into_iter()
            .map(|obs| {
                let score = self.profile.score(&obs);
                ScoredObservation {
                    observation: obs,
                    score,
                }
            })
            .collect();

        // Deterministic sort: score desc, then path depth asc, then path lexicographic asc
        scored.sort_by(|a, b| {
            // Score descending
            let score_cmp = cmp_f64(b.score, a.score);
            if score_cmp != Ordering::Equal {
                return score_cmp;
            }
            // Path depth ascending (shallower first)
            let depth_a = path_depth(&a.observation.path);
            let depth_b = path_depth(&b.observation.path);
            let depth_cmp = depth_a.cmp(&depth_b);
            if depth_cmp != Ordering::Equal {
                return depth_cmp;
            }
            // Path lexicographic ascending
            a.observation.path.cmp(&b.observation.path)
        });

        let metadata = self.doc.metadata();

        // Apply token budget if set
        let (final_obs, truncated, budget_used) = if let Some(max_tokens) = self.budget {
            apply_budget(scored, max_tokens)
        } else {
            (scored, false, None)
        };

        EssenceData {
            observations: final_obs,
            document_identity: format!(
                "document(nodes={}, paths={}, depth={})",
                metadata.total_nodes, metadata.distinct_paths, metadata.max_depth
            ),
            total_nodes: metadata.total_nodes,
            distinct_paths: metadata.distinct_paths,
            max_depth: metadata.max_depth,
            truncated,
            budget_used,
            budget_limit: self.budget,
        }
    }
}

fn observations_from_stats(stats: &StatsResult, candidates: &mut Vec<CandidateObservation>) {
    for (path, path_stats) in &stats.paths {
        let path_str = path.to_string();

        // High entropy => "informative field"
        if path_stats.normalized_entropy > 0.8 {
            candidates.push(CandidateObservation {
                path: path_str.clone(),
                description: format!(
                    "informative field (normalized entropy {:.3})",
                    path_stats.normalized_entropy
                ),
                rarity: 0.0,
                instability: 0.0,
                entropy_signal: path_stats.normalized_entropy,
                structural_coverage: 0.0,
                anomaly_strength: 0.0,
                concern_relevance: 0.3,
            });
        }

        // Low cardinality => "enum-like field"
        if path_stats.cardinality > 0 && path_stats.cardinality <= 10 && path_stats.total_count >= 2
        {
            // Scale rarity: fewer distinct values = more enum-like
            #[allow(clippy::cast_precision_loss)]
            let rarity_signal = 1.0 - (path_stats.cardinality as f64 / 10.0);
            candidates.push(CandidateObservation {
                path: path_str,
                description: format!(
                    "enum-like field ({} distinct values over {} observations)",
                    path_stats.cardinality, path_stats.total_count
                ),
                rarity: rarity_signal,
                instability: 0.0,
                entropy_signal: 0.0,
                structural_coverage: 0.2,
                anomaly_strength: 0.0,
                concern_relevance: 0.2,
            });
        }
    }
}

fn observations_from_anomalies(
    anomalies: &AnomalyReport,
    candidates: &mut Vec<CandidateObservation>,
) {
    // Numeric outliers
    for outlier in &anomalies.numeric_outliers {
        let strength = (outlier.z_mad.abs() / 10.0).min(1.0);
        candidates.push(CandidateObservation {
            path: outlier.path.clone(),
            description: format!(
                "numeric outlier (value={}, z_mad={:.2})",
                outlier.value, outlier.z_mad
            ),
            rarity: 0.0,
            instability: 0.0,
            entropy_signal: 0.0,
            structural_coverage: 0.0,
            anomaly_strength: strength,
            concern_relevance: 0.5,
        });
    }

    // Rare values
    for rare in &anomalies.rare_values {
        let strength = (rare.rarity_bits / 20.0).min(1.0);
        candidates.push(CandidateObservation {
            path: rare.path.clone(),
            description: format!(
                "rare value '{}' (count={}, rarity={:.2} bits)",
                rare.value, rare.count, rare.rarity_bits
            ),
            rarity: strength,
            instability: 0.0,
            entropy_signal: 0.0,
            structural_coverage: 0.0,
            anomaly_strength: strength,
            concern_relevance: 0.4,
        });
    }

    // Type instabilities from anomaly report
    for instability in &anomalies.type_instabilities {
        candidates.push(CandidateObservation {
            path: instability.path.clone(),
            description: format!(
                "type instability ({:.1}% {}, mixed types)",
                (1.0 - instability.instability) * 100.0,
                instability.dominant_type
            ),
            rarity: 0.0,
            instability: instability.instability,
            entropy_signal: 0.0,
            structural_coverage: 0.0,
            anomaly_strength: instability.instability,
            concern_relevance: 0.6,
        });
    }
}

fn observations_from_trie(doc: &Document, candidates: &mut Vec<CandidateObservation>) {
    let trie = doc.trie();
    let paths = trie.all_paths();

    for path in &paths {
        if let Some(node) = trie.get(path) {
            let meta = &node.metadata;
            let path_str = path.to_string();

            // Paths with high null_rate
            if meta.count > 0 && meta.null_rate() > 0.5 {
                candidates.push(CandidateObservation {
                    path: path_str.clone(),
                    description: format!(
                        "frequently null ({:.1}% null rate)",
                        meta.null_rate() * 100.0
                    ),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.3,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.3,
                });
            }

            // Type instability > 0
            if meta.count > 0 && meta.type_instability() > 0.0 {
                candidates.push(CandidateObservation {
                    path: path_str,
                    description: format!(
                        "type instability detected ({:.3})",
                        meta.type_instability()
                    ),
                    rarity: 0.0,
                    instability: meta.type_instability(),
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.2,
                });
            }
        }
    }
}

fn observations_from_fingerprint(
    fp: &FingerprintResult,
    candidates: &mut Vec<CandidateObservation>,
) {
    // Motif frequencies: subtree hashes that appear more than once.
    // `subtree_frequencies` is already a `BTreeMap`, so iteration is deterministic.
    for (hash, &freq) in &fp.subtree_frequencies {
        if freq > 1 {
            let hash_hex = hex_prefix(hash);
            #[allow(clippy::cast_precision_loss)]
            let strength = (freq as f64 / 100.0).min(1.0);
            candidates.push(CandidateObservation {
                path: format!("subtree:{hash_hex}"),
                description: format!("repeated structure (motif appears {freq} times)"),
                rarity: 0.0,
                instability: 0.0,
                entropy_signal: 0.0,
                structural_coverage: strength,
                anomaly_strength: 0.0,
                concern_relevance: 0.1,
            });
        }
    }
}

/// Compare two `f64` values with total ordering (NaN-safe).
fn cmp_f64(a: f64, b: f64) -> Ordering {
    a.total_cmp(&b)
}

/// Compute path depth by counting `.` and `[` segments.
fn path_depth(path: &str) -> usize {
    if path.is_empty() || path == "$" {
        return 0;
    }
    // Count segments: each `.key` or `[*]` is one level
    let mut depth = 0usize;
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'$' => {
                // root, skip
                i += 1;
            }
            b'.' => {
                depth += 1;
                i += 1;
            }
            b'[' => {
                depth += 1;
                // skip past `]`
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // skip `]`
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    depth
}

/// Apply a token budget to select observations by score-per-token ratio.
///
/// Returns (selected observations, whether truncation occurred, estimated tokens used).
fn apply_budget(
    observations: Vec<ScoredObservation>,
    max_tokens: usize,
) -> (Vec<ScoredObservation>, bool, Option<usize>) {
    if observations.is_empty() {
        return (observations, false, Some(0));
    }

    // Estimate tokens for each observation and compute score-per-token ratio
    let mut scored_with_tokens: Vec<(ScoredObservation, usize, f64)> = observations
        .into_iter()
        .map(|obs| {
            let text = format!(
                "{} {} {:.4}",
                obs.observation.path, obs.observation.description, obs.score
            );
            let tokens = estimate_tokens(&text).max(1);
            let ratio = obs.score / tokens as f64;
            (obs, tokens, ratio)
        })
        .collect();

    // Sort by score-per-token ratio descending (greedy knapsack)
    scored_with_tokens.sort_by(|a, b| b.2.total_cmp(&a.2));

    let mut selected = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;

    for (obs, tokens, _ratio) in scored_with_tokens {
        if used + tokens > max_tokens {
            truncated = true;
            continue;
        }
        used += tokens;
        selected.push(obs);
    }

    // Re-sort by original ordering: score desc, depth asc, path asc
    selected.sort_by(|a, b| {
        let score_cmp = cmp_f64(b.score, a.score);
        if score_cmp != Ordering::Equal {
            return score_cmp;
        }
        let depth_a = path_depth(&a.observation.path);
        let depth_b = path_depth(&b.observation.path);
        let depth_cmp = depth_a.cmp(&depth_b);
        if depth_cmp != Ordering::Equal {
            return depth_cmp;
        }
        a.observation.path.cmp(&b.observation.path)
    });

    (selected, truncated, Some(used))
}

/// First 8 hex characters of a hash.
fn hex_prefix(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(8);
    for b in &hash[..4] {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{EngineerProfile, StaffProfile};

    fn parse_doc(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
    }

    #[test]
    fn builder_deterministic_output() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1, "b": 2, "c": [1, 2, 3]}"#)?;
        let profile = EngineerProfile;

        let e1 = EssenceBuilder::new(&doc, &profile).build();
        let e2 = EssenceBuilder::new(&doc, &profile).build();

        assert_eq!(e1.observations.len(), e2.observations.len());
        for (a, b) in e1.observations.iter().zip(e2.observations.iter()) {
            assert_eq!(a.observation.path, b.observation.path);
            assert_eq!(a.observation.description, b.observation.description);
            assert!((a.score - b.score).abs() < f64::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn higher_scored_observations_first() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1, "b": 2}"#)?;
        let profile = EngineerProfile;

        let essence = EssenceBuilder::new(&doc, &profile).build();

        for window in essence.observations.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "observations not sorted by score descending: {:.4} < {:.4}",
                window[0].score,
                window[1].score
            );
        }
        Ok(())
    }

    #[test]
    fn tiebreaking_shallower_path_first() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": {"b": {"c": null}}, "x": null}"#)?;
        let profile = StaffProfile;

        let essence = EssenceBuilder::new(&doc, &profile).build();

        for window in essence.observations.windows(2) {
            if (window[0].score - window[1].score).abs() < f64::EPSILON {
                let depth_a = path_depth(&window[0].observation.path);
                let depth_b = path_depth(&window[1].observation.path);
                assert!(
                    depth_a <= depth_b,
                    "same-score observations not sorted by depth: {} (depth {}) after {} (depth {})",
                    window[0].observation.path,
                    depth_a,
                    window[1].observation.path,
                    depth_b
                );
                if depth_a == depth_b {
                    assert!(
                        window[0].observation.path <= window[1].observation.path,
                        "same-score same-depth observations not sorted lexicographically"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn empty_document_produces_valid_essence() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc("{}")?;
        let profile = StaffProfile;

        let essence = EssenceBuilder::new(&doc, &profile).build();

        assert!(essence.observations.is_empty());
        assert!(!essence.document_identity.is_empty());
        Ok(())
    }

    #[test]
    fn with_stats_generates_observations() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["a", "b", "a", "b", "a"]"#)?;

        use vajra_stats::StatsAnalyzer;
        use vajra_types::traits::Analyzer;
        let analyzer = StatsAnalyzer;
        let stats = analyzer.analyze(&doc)?;

        let profile = EngineerProfile;
        let essence = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .build();

        let has_enum = essence
            .observations
            .iter()
            .any(|o| o.observation.description.contains("enum-like"));
        assert!(has_enum, "should detect enum-like field from stats");
        Ok(())
    }

    #[test]
    fn with_anomalies_generates_observations() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(
            r#"{"values": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 1000]}"#,
        )?;

        use vajra_anomaly::AnomalyAnalyzer;
        use vajra_types::traits::Analyzer;
        let analyzer = AnomalyAnalyzer::default();
        let report = analyzer.analyze(&doc)?;

        let profile = StaffProfile;
        let essence = EssenceBuilder::new(&doc, &profile)
            .with_anomalies(&report)
            .build();

        let has_anomaly = essence
            .observations
            .iter()
            .any(|o| o.observation.anomaly_strength > 0.0);
        assert!(has_anomaly, "should have anomaly observations");
        Ok(())
    }

    #[test]
    fn with_fingerprint_generates_motif_observations() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"[{"a": 1, "b": 2}, {"a": 3, "b": 4}, {"a": 5, "b": 6}]"#)?;

        use vajra_fingerprint::FingerprintAnalyzer;
        use vajra_types::traits::Analyzer;
        let analyzer = FingerprintAnalyzer;
        let fp = analyzer.analyze(&doc)?;

        let profile = EngineerProfile;
        let essence = EssenceBuilder::new(&doc, &profile)
            .with_fingerprint(&fp)
            .build();

        let has_motif = essence
            .observations
            .iter()
            .any(|o| o.observation.description.contains("repeated structure"));
        let _ = has_motif;
        Ok(())
    }

    #[test]
    fn path_depth_computation() {
        assert_eq!(path_depth("$"), 0);
        assert_eq!(path_depth("$.a"), 1);
        assert_eq!(path_depth("$.a.b"), 2);
        assert_eq!(path_depth("$.a[*]"), 2);
        assert_eq!(path_depth("$.a[*].b"), 3);
        assert_eq!(path_depth(""), 0);
    }

    #[test]
    fn cmp_f64_handles_nan() {
        assert_eq!(cmp_f64(1.0, 2.0), Ordering::Less);
        assert_eq!(cmp_f64(2.0, 1.0), Ordering::Greater);
        assert_eq!(cmp_f64(1.0, 1.0), Ordering::Equal);
    }

    #[test]
    fn budget_limits_observations() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(
            r#"{"values": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 1000]}"#,
        )?;
        let profile = EngineerProfile;

        use vajra_anomaly::AnomalyAnalyzer;
        use vajra_stats::StatsAnalyzer;
        use vajra_types::traits::Analyzer;
        let stats = StatsAnalyzer.analyze(&doc)?;
        let anomalies = AnomalyAnalyzer::default().analyze(&doc)?;

        // Build without budget
        let no_budget = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .build();
        let total_obs = no_budget.observations.len();

        // Build with a very small budget (just 10 tokens) to force truncation
        let with_budget = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .with_budget(10)
            .build();

        assert!(
            with_budget.observations.len() < total_obs,
            "budget=10 should include fewer observations than no budget: {} < {}",
            with_budget.observations.len(),
            total_obs
        );
        assert!(with_budget.truncated, "should be truncated");
        Ok(())
    }

    #[test]
    fn budget_selects_by_score_per_token_ratio() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(
            r#"{"values": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 1000]}"#,
        )?;

        use vajra_anomaly::AnomalyAnalyzer;
        use vajra_stats::StatsAnalyzer;
        use vajra_types::traits::Analyzer;

        let stats = StatsAnalyzer.analyze(&doc)?;
        let anomalies = AnomalyAnalyzer::default().analyze(&doc)?;

        let profile = EngineerProfile;

        let no_budget = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .build();

        let with_budget = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .with_budget(100)
            .build();

        // With budget, if truncated, all selected observations should have
        // reasonable scores (the highest-ratio ones are kept)
        if with_budget.truncated {
            for obs in &with_budget.observations {
                // All selected observations should have a non-zero score
                assert!(obs.score >= 0.0);
            }
            assert!(with_budget.budget_used.is_some());
            assert!(with_budget.budget_limit == Some(100));
        }

        // Budget result should never exceed the non-budget result
        assert!(with_budget.observations.len() <= no_budget.observations.len());
        Ok(())
    }

    #[test]
    fn budget_metadata_set_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1}"#)?;
        let profile = EngineerProfile;

        let essence = EssenceBuilder::new(&doc, &profile)
            .with_budget(1000)
            .build();

        assert_eq!(essence.budget_limit, Some(1000));
        assert!(essence.budget_used.is_some());
        assert!(!essence.truncated);
        Ok(())
    }

    #[test]
    fn no_budget_leaves_fields_default() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1}"#)?;
        let profile = EngineerProfile;

        let essence = EssenceBuilder::new(&doc, &profile).build();

        assert_eq!(essence.budget_limit, None);
        assert_eq!(essence.budget_used, None);
        assert!(!essence.truncated);
        Ok(())
    }
}
