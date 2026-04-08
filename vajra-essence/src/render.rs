//! Deterministic text and JSON rendering for essence data.
//!
//! All output is fully deterministic: same [`EssenceData`] produces
//! identical output every time.

use std::fmt::Write;

use vajra_types::traits::{EssenceData, OutputFormat, ScoredObservation};

use crate::compact_ai::render_compact_ai;
use crate::markdown::render_markdown;

/// Render essence data as human-readable text.
///
/// The section headers and style depend on the profile name:
/// - `"staff"`: narrative sections (Document Summary, What Stands Out, What This Likely Means)
/// - `"engineer"`: technical sections (Structure, Paths, Anomalies, Statistics)
/// - Other profiles fall back to a generic layout.
#[must_use]
pub fn render_text(essence: &EssenceData, profile_name: &str) -> String {
    let mut out = String::new();

    match profile_name {
        "staff" => render_staff_text(essence, &mut out),
        "engineer" => render_engineer_text(essence, &mut out),
        "fraud" => render_fraud_text(essence, &mut out),
        "auditor" => render_auditor_text(essence, &mut out),
        "ai" => render_ai_text(essence, &mut out),
        _ => render_generic_text(essence, profile_name, &mut out),
    }

    out
}

/// Render essence data as a structured JSON value.
///
/// The returned value contains:
/// - `identity`: document identity string
/// - `structure`: node count, distinct paths, max depth
/// - `observations`: array of scored observations
/// - `anomalies`: array of observations with `anomaly_strength` > 0
#[must_use]
pub fn render_json(essence: &EssenceData) -> serde_json::Value {
    let observations: Vec<serde_json::Value> = essence
        .observations
        .iter()
        .map(observation_to_json)
        .collect();

    let anomalies: Vec<serde_json::Value> = essence
        .observations
        .iter()
        .filter(|obs| obs.observation.anomaly_strength > 0.0)
        .map(observation_to_json)
        .collect();

    serde_json::json!({
        "identity": essence.document_identity,
        "structure": {
            "total_nodes": essence.total_nodes,
            "distinct_paths": essence.distinct_paths,
            "max_depth": essence.max_depth
        },
        "observations": observations,
        "anomalies": anomalies
    })
}

fn observation_to_json(obs: &ScoredObservation) -> serde_json::Value {
    serde_json::json!({
        "path": obs.observation.path,
        "description": obs.observation.description,
        "score": obs.score,
        "rarity": obs.observation.rarity,
        "instability": obs.observation.instability,
        "entropy_signal": obs.observation.entropy_signal,
        "structural_coverage": obs.observation.structural_coverage,
        "anomaly_strength": obs.observation.anomaly_strength,
        "concern_relevance": obs.observation.concern_relevance
    })
}

/// Render essence data in the given output format.
///
/// Dispatches to the appropriate renderer based on format:
/// - `CompactAi` -> compact-AI JSON
/// - `Markdown` -> GitHub-flavored markdown
/// - `Json` -> structured JSON
/// - `Text` / `Ndjson` -> profile-specific text
#[must_use]
pub fn render_format(essence: &EssenceData, profile_name: &str, format: OutputFormat) -> String {
    match format {
        OutputFormat::CompactAi => {
            let json = render_compact_ai(essence, false);
            serde_json::to_string_pretty(&json).unwrap_or_default()
        }
        OutputFormat::Markdown => render_markdown(essence, profile_name),
        OutputFormat::Json => {
            let json = render_json(essence);
            serde_json::to_string_pretty(&json).unwrap_or_default()
        }
        _ => render_text(essence, profile_name),
    }
}

fn render_staff_text(essence: &EssenceData, out: &mut String) {
    // Document Summary
    out.push_str("Document Summary\n");
    out.push_str("================\n\n");
    let _ = write!(
        out,
        "This document contains {} nodes across {} distinct paths, \
         reaching a maximum depth of {}.\n\n",
        essence.total_nodes, essence.distinct_paths, essence.max_depth
    );

    // What Stands Out
    out.push_str("What Stands Out\n");
    out.push_str("===============\n\n");

    if essence.observations.is_empty() {
        out.push_str("Nothing unusual was found in this document.\n\n");
    } else {
        for obs in &essence.observations {
            let _ = writeln!(
                out,
                "- {}: {}",
                obs.observation.path, obs.observation.description
            );
        }
        out.push('\n');
    }

    // What This Likely Means
    out.push_str("What This Likely Means\n");
    out.push_str("======================\n\n");

    let anomaly_count = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .count();

    if anomaly_count == 0 {
        out.push_str("The document appears straightforward with no notable anomalies.\n");
    } else {
        let _ = writeln!(
            out,
            "There {} {} anomal{} worth reviewing in this document.",
            if anomaly_count == 1 { "is" } else { "are" },
            anomaly_count,
            if anomaly_count == 1 { "y" } else { "ies" }
        );
    }
}

fn render_engineer_text(essence: &EssenceData, out: &mut String) {
    // Structure
    out.push_str("Structure\n");
    out.push_str("---------\n");
    let _ = writeln!(out, "  total_nodes: {}", essence.total_nodes);
    let _ = writeln!(out, "  distinct_paths: {}", essence.distinct_paths);
    let _ = writeln!(out, "  max_depth: {}\n", essence.max_depth);

    // Paths
    out.push_str("Paths\n");
    out.push_str("-----\n");
    let paths: Vec<&str> = essence
        .observations
        .iter()
        .map(|o| o.observation.path.as_str())
        .collect();
    if paths.is_empty() {
        out.push_str("  (none)\n\n");
    } else {
        // Deduplicate paths while preserving order
        let mut seen = std::collections::BTreeSet::new();
        for path in &paths {
            if seen.insert(*path) {
                let _ = writeln!(out, "  {path}");
            }
        }
        out.push('\n');
    }

    // Anomalies
    out.push_str("Anomalies\n");
    out.push_str("---------\n");
    let anomalies: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .collect();
    if anomalies.is_empty() {
        out.push_str("  (none)\n\n");
    } else {
        for obs in &anomalies {
            let _ = writeln!(
                out,
                "  [{:.4}] {} -- {}",
                obs.score, obs.observation.path, obs.observation.description
            );
        }
        out.push('\n');
    }

    // Statistics
    out.push_str("Statistics\n");
    out.push_str("----------\n");
    let stats_obs: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength == 0.0)
        .collect();
    if stats_obs.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for obs in &stats_obs {
            let _ = writeln!(
                out,
                "  [{:.4}] {} -- {}",
                obs.score, obs.observation.path, obs.observation.description
            );
        }
    }
}

fn render_fraud_text(essence: &EssenceData, out: &mut String) {
    // Risk Indicators
    out.push_str("Risk Indicators\n");
    out.push_str("================\n\n");

    let anomalies: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .collect();

    if anomalies.is_empty() {
        out.push_str("No risk indicators identified.\n\n");
    } else {
        for obs in &anomalies {
            let _ = writeln!(
                out,
                "  [RISK {:.4}] {} -- {}",
                obs.score, obs.observation.path, obs.observation.description
            );
        }
        out.push('\n');
    }

    // Numeric Anomalies
    out.push_str("Numeric Anomalies\n");
    out.push_str("==================\n\n");

    let numeric: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| {
            o.observation.description.contains("outlier")
                || o.observation.description.contains("numeric")
        })
        .collect();

    if numeric.is_empty() {
        out.push_str("No numeric anomalies detected.\n\n");
    } else {
        for obs in &numeric {
            let _ = writeln!(
                out,
                "  [ALERT {:.4}] {} -- {}",
                obs.score, obs.observation.path, obs.observation.description
            );
        }
        out.push('\n');
    }

    // Pattern Analysis
    out.push_str("Pattern Analysis\n");
    out.push_str("=================\n\n");

    let patterns: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| {
            o.observation.rarity > 0.0
                || o.observation.description.contains("rare")
                || o.observation.description.contains("repeated")
                || o.observation.description.contains("enum-like")
        })
        .collect();

    if patterns.is_empty() {
        out.push_str("No suspicious patterns found.\n");
    } else {
        for obs in &patterns {
            let _ = writeln!(
                out,
                "  [{:.4}] {} -- {}",
                obs.score, obs.observation.path, obs.observation.description
            );
        }
    }
}

fn render_auditor_text(essence: &EssenceData, out: &mut String) {
    // Completeness Assessment
    out.push_str("Completeness Assessment\n");
    out.push_str("========================\n\n");

    let _ = writeln!(
        out,
        "Total nodes examined: {}\nDistinct paths: {}\nMaximum depth: {}\n",
        essence.total_nodes, essence.distinct_paths, essence.max_depth
    );

    let null_obs: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.description.contains("null"))
        .collect();

    if null_obs.is_empty() {
        out.push_str("All examined fields contain non-null values.\n\n");
    } else {
        out.push_str("Fields with null values:\n");
        for obs in &null_obs {
            let _ = writeln!(
                out,
                "  - {} -- {}",
                obs.observation.path, obs.observation.description
            );
        }
        out.push('\n');
    }

    // Consistency Findings
    out.push_str("Consistency Findings\n");
    out.push_str("=====================\n\n");

    let instability_obs: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.instability > 0.0)
        .collect();

    if instability_obs.is_empty() {
        out.push_str("No type consistency issues found.\n\n");
    } else {
        for obs in &instability_obs {
            let _ = writeln!(
                out,
                "  FINDING: {} -- {} (instability={:.4})",
                obs.observation.path, obs.observation.description, obs.observation.instability
            );
        }
        out.push('\n');
    }

    // Traceability Notes
    out.push_str("Traceability Notes\n");
    out.push_str("===================\n\n");

    if essence.observations.is_empty() {
        out.push_str("No observations recorded.\n");
    } else {
        let _ = writeln!(
            out,
            "Total observations: {}\nScored and ranked by auditor concern weights.\n",
            essence.observations.len()
        );
        for (i, obs) in essence.observations.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {}. [{:.4}] {} -- {}",
                i + 1,
                obs.score,
                obs.observation.path,
                obs.observation.description
            );
        }
    }
}

fn render_ai_text(essence: &EssenceData, out: &mut String) {
    // structure section
    out.push_str("structure\n");
    let _ = writeln!(
        out,
        "  nodes={} paths={} depth={}\n",
        essence.total_nodes, essence.distinct_paths, essence.max_depth
    );

    // observations section
    out.push_str("observations\n");
    if essence.observations.is_empty() {
        out.push_str("  (none)\n\n");
    } else {
        for obs in &essence.observations {
            let _ = writeln!(
                out,
                "  score={:.4} path={} desc=\"{}\" rarity={:.3} instability={:.3} entropy={:.3} coverage={:.3} anomaly={:.3}",
                obs.score,
                obs.observation.path,
                obs.observation.description,
                obs.observation.rarity,
                obs.observation.instability,
                obs.observation.entropy_signal,
                obs.observation.structural_coverage,
                obs.observation.anomaly_strength,
            );
        }
        out.push('\n');
    }

    // anomalies section
    out.push_str("anomalies\n");
    let anomalies: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .collect();
    if anomalies.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for obs in &anomalies {
            let _ = writeln!(
                out,
                "  score={:.4} path={} anomaly_strength={:.3} desc=\"{}\"",
                obs.score,
                obs.observation.path,
                obs.observation.anomaly_strength,
                obs.observation.description,
            );
        }
    }
}

fn render_generic_text(essence: &EssenceData, profile_name: &str, out: &mut String) {
    let _ = writeln!(out, "Essence ({profile_name})");
    let _ = writeln!(
        out,
        "nodes={} paths={} depth={}\n",
        essence.total_nodes, essence.distinct_paths, essence.max_depth
    );

    for obs in &essence.observations {
        let _ = writeln!(
            out,
            "  [{:.4}] {} -- {}",
            obs.score, obs.observation.path, obs.observation.description
        );
    }
}
