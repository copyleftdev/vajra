//! GitHub-flavored Markdown renderer for essence data.
//!
//! Produces structured Markdown with metadata tables, ranked observation
//! lists, anomaly sections, and path tables.

use std::fmt::Write;

use vajra_types::traits::{EssenceData, ScoredObservation};

/// Render essence data as GitHub-flavored Markdown.
#[must_use]
pub fn render_markdown(essence: &EssenceData, profile_name: &str) -> String {
    let mut out = String::new();

    // Title
    let _ = writeln!(out, "# Vajra Essence Report ({profile_name})\n");

    // Document Summary
    render_summary(essence, &mut out);

    // Observations
    render_observations(essence, &mut out);

    // Anomalies
    render_anomalies(essence, &mut out);

    // Structure
    render_structure(essence, &mut out);

    // Budget info if present
    render_budget_info(essence, &mut out);

    out
}

fn render_summary(essence: &EssenceData, out: &mut String) {
    out.push_str("## Document Summary\n\n");
    out.push_str("| Property | Value |\n");
    out.push_str("|----------|-------|\n");
    let _ = writeln!(out, "| Total Nodes | {} |", essence.total_nodes);
    let _ = writeln!(out, "| Distinct Paths | {} |", essence.distinct_paths);
    let _ = writeln!(out, "| Max Depth | {} |", essence.max_depth);
    let _ = writeln!(
        out,
        "| Observations | {} |",
        essence.observations.len()
    );
    let anomaly_count = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .count();
    let _ = writeln!(out, "| Anomalies | {anomaly_count} |");
    out.push('\n');
}

fn render_observations(essence: &EssenceData, out: &mut String) {
    out.push_str("## Observations\n\n");

    if essence.observations.is_empty() {
        out.push_str("No observations found.\n\n");
        return;
    }

    for (i, obs) in essence.observations.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}. **[{:.4}]** `{}` -- {}",
            i + 1,
            obs.score,
            obs.observation.path,
            obs.observation.description
        );
    }
    out.push('\n');
}

fn render_anomalies(essence: &EssenceData, out: &mut String) {
    out.push_str("## Anomalies\n\n");

    let anomalies: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .collect();

    if anomalies.is_empty() {
        out.push_str("No anomalies detected.\n\n");
        return;
    }

    for obs in &anomalies {
        let severity = if obs.observation.anomaly_strength > 0.7 {
            "HIGH"
        } else if obs.observation.anomaly_strength > 0.4 {
            "MEDIUM"
        } else {
            "LOW"
        };

        // High severity gets bold
        if obs.observation.anomaly_strength > 0.7 {
            let _ = writeln!(
                out,
                "- **[{severity}]** `{}` -- {} (strength: {:.3})",
                obs.observation.path,
                obs.observation.description,
                obs.observation.anomaly_strength
            );
        } else {
            let _ = writeln!(
                out,
                "- [{severity}] `{}` -- {} (strength: {:.3})",
                obs.observation.path,
                obs.observation.description,
                obs.observation.anomaly_strength
            );
        }
    }
    out.push('\n');
}

fn render_structure(essence: &EssenceData, out: &mut String) {
    out.push_str("## Structure\n\n");

    // Collect unique paths
    let mut paths: Vec<&str> = essence
        .observations
        .iter()
        .map(|o| o.observation.path.as_str())
        .filter(|p| !p.is_empty() && !p.starts_with("subtree:"))
        .collect();
    paths.sort();
    paths.dedup();

    if paths.is_empty() {
        out.push_str("No structural paths recorded.\n\n");
        return;
    }

    out.push_str("| Path | Observation Count |\n");
    out.push_str("|------|-------------------|\n");

    for path in &paths {
        let count = essence
            .observations
            .iter()
            .filter(|o| o.observation.path == *path)
            .count();
        let _ = writeln!(out, "| `{path}` | {count} |");
    }
    out.push('\n');

    // Fingerprint section: show motif hashes if any
    let motifs: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.path.starts_with("subtree:"))
        .collect();

    if !motifs.is_empty() {
        out.push_str("### Fingerprints\n\n");
        out.push_str("```\n");
        for obs in &motifs {
            let hash = obs
                .observation
                .path
                .strip_prefix("subtree:")
                .unwrap_or(&obs.observation.path);
            let _ = writeln!(out, "{hash}: {}", obs.observation.description);
        }
        out.push_str("```\n\n");
    }
}

fn render_budget_info(essence: &EssenceData, out: &mut String) {
    if essence.truncated {
        out.push_str("---\n\n");
        out.push_str("*Note: Output was truncated due to token budget constraints.*\n");
        if let (Some(used), Some(limit)) = (essence.budget_used, essence.budget_limit) {
            let _ = writeln!(out, "*Budget: {used} / {limit} tokens used.*\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::CandidateObservation;

    fn make_test_essence() -> EssenceData {
        let observations = vec![
            ScoredObservation {
                observation: CandidateObservation {
                    path: "$.users[*].name".to_string(),
                    description: "enum-like field (3 distinct values over 10 observations)"
                        .to_string(),
                    rarity: 0.7,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.2,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.2,
                },
                score: 0.45,
            },
            ScoredObservation {
                observation: CandidateObservation {
                    path: "$.users[*].age".to_string(),
                    description: "numeric outlier (value=350, z_mad=4.20)".to_string(),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.42,
                    concern_relevance: 0.5,
                },
                score: 0.38,
            },
            ScoredObservation {
                observation: CandidateObservation {
                    path: "$.users[*].status".to_string(),
                    description: "type instability detected (0.150)".to_string(),
                    rarity: 0.0,
                    instability: 0.15,
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.8,
                    concern_relevance: 0.6,
                },
                score: 0.72,
            },
            ScoredObservation {
                observation: CandidateObservation {
                    path: "subtree:abcd1234".to_string(),
                    description: "repeated structure (motif appears 5 times)".to_string(),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.05,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.1,
                },
                score: 0.05,
            },
        ];

        EssenceData {
            observations,
            document_identity: "test-doc".to_string(),
            total_nodes: 100,
            distinct_paths: 15,
            max_depth: 4,
            truncated: false,
            budget_used: None,
            budget_limit: None,
        }
    }

    #[test]
    fn markdown_contains_expected_headers() {
        let essence = make_test_essence();
        let md = render_markdown(&essence, "engineer");
        assert!(md.contains("## Document Summary"), "should have Document Summary header");
        assert!(md.contains("## Observations"), "should have Observations header");
        assert!(md.contains("## Anomalies"), "should have Anomalies header");
        assert!(md.contains("## Structure"), "should have Structure header");
    }

    #[test]
    fn markdown_tables_are_valid_gfm() {
        let essence = make_test_essence();
        let md = render_markdown(&essence, "engineer");

        // Check that the summary table has header separator
        assert!(
            md.contains("|----------|-------|"),
            "summary table should have GFM separator"
        );

        // Check structure table
        assert!(
            md.contains("|------|-------------------|"),
            "structure table should have GFM separator"
        );

        // Both tables should have | delimiters
        let lines: Vec<&str> = md.lines().collect();
        let table_lines: Vec<&&str> = lines
            .iter()
            .filter(|l| l.starts_with('|') && l.ends_with('|'))
            .collect();
        assert!(
            !table_lines.is_empty(),
            "should have valid GFM table rows"
        );
    }

    #[test]
    fn markdown_anomalies_have_severity_badges() {
        let essence = make_test_essence();
        let md = render_markdown(&essence, "test");

        // High severity (anomaly_strength > 0.7) should be bold
        assert!(
            md.contains("**[HIGH]**"),
            "high severity anomalies should be bold"
        );

        // Medium severity should exist too
        assert!(
            md.contains("[MEDIUM]") || md.contains("[LOW]"),
            "should have medium or low severity badges"
        );
    }

    #[test]
    fn markdown_fingerprints_in_code_blocks() {
        let essence = make_test_essence();
        let md = render_markdown(&essence, "test");

        assert!(
            md.contains("### Fingerprints"),
            "should have fingerprints section"
        );
        assert!(md.contains("```"), "fingerprints should be in code blocks");
        assert!(
            md.contains("abcd1234"),
            "should contain the motif hash"
        );
    }

    #[test]
    fn markdown_empty_essence() {
        let essence = EssenceData {
            observations: vec![],
            document_identity: "empty".to_string(),
            total_nodes: 0,
            distinct_paths: 0,
            max_depth: 0,
            truncated: false,
            budget_used: None,
            budget_limit: None,
        };
        let md = render_markdown(&essence, "test");
        assert!(md.contains("No observations found"));
        assert!(md.contains("No anomalies detected"));
    }

    #[test]
    fn markdown_truncation_notice() {
        let mut essence = make_test_essence();
        essence.truncated = true;
        essence.budget_used = Some(400);
        essence.budget_limit = Some(500);

        let md = render_markdown(&essence, "test");
        assert!(
            md.contains("truncated"),
            "should show truncation notice"
        );
        assert!(
            md.contains("400") && md.contains("500"),
            "should show budget usage"
        );
    }

    #[test]
    fn markdown_profile_name_in_title() {
        let essence = make_test_essence();
        let md = render_markdown(&essence, "fraud");
        assert!(
            md.contains("# Vajra Essence Report (fraud)"),
            "title should include profile name"
        );
    }
}
