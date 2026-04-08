//! Compact-AI renderer: maximally token-efficient, structured for LLM parsing.
//!
//! This is the flagship AI output format. It uses short keys, collapses
//! motifs, separates anomalies from notable observations, and optionally
//! includes chain-ready drill-down information.

use std::collections::BTreeMap;

use vajra_types::traits::{EssenceData, ScoredObservation};

/// Render essence data in the compact-AI format.
///
/// If `chain_ready` is true, a `"drill"` section is appended listing
/// paths with deeper analysis available.
#[must_use]
pub fn render_compact_ai(essence: &EssenceData, chain_ready: bool) -> serde_json::Value {
    let mut root = serde_json::Map::new();

    // Version tag
    root.insert("v".to_string(), serde_json::json!("vajra/1"));

    // Document summary with short keys
    root.insert(
        "doc".to_string(),
        serde_json::json!({
            "nodes": essence.total_nodes,
            "paths": essence.distinct_paths,
            "depth": essence.max_depth
        }),
    );

    // Motifs: collapse repeated structures into one entry with count
    let motifs = build_motifs(&essence.observations);
    root.insert("motifs".to_string(), serde_json::Value::Array(motifs));

    // Anomalies: observations with anomaly_strength > 0
    let anomalies = build_anomalies(&essence.observations);
    root.insert("anomalies".to_string(), serde_json::Value::Array(anomalies));

    // Notable: non-anomaly observations
    let notable = build_notable(&essence.observations);
    root.insert("notable".to_string(), serde_json::Value::Array(notable));

    // Meta
    let mut meta = serde_json::Map::new();
    meta.insert("profile".to_string(), serde_json::json!("ai"));
    meta.insert(
        "truncated".to_string(),
        serde_json::json!(essence.truncated),
    );
    if let Some(used) = essence.budget_used {
        meta.insert("budget_used".to_string(), serde_json::json!(used));
    }
    if let Some(limit) = essence.budget_limit {
        meta.insert("budget_limit".to_string(), serde_json::json!(limit));
    }
    root.insert("meta".to_string(), serde_json::Value::Object(meta));

    // Chain-ready drill section
    if chain_ready {
        let drill = build_drill(&essence.observations);
        root.insert("drill".to_string(), serde_json::Value::Array(drill));
    }

    serde_json::Value::Object(root)
}

/// Build motif entries: collapse repeated-structure observations by hash.
fn build_motifs(observations: &[ScoredObservation]) -> Vec<serde_json::Value> {
    // Group motifs by their subtree hash prefix
    let mut motif_map: BTreeMap<String, MotifAccum> = BTreeMap::new();

    for obs in observations {
        if obs.observation.description.contains("repeated structure")
            || obs.observation.path.starts_with("subtree:")
        {
            let hash = if obs.observation.path.starts_with("subtree:") {
                obs.observation
                    .path
                    .strip_prefix("subtree:")
                    .unwrap_or("")
                    .to_string()
            } else {
                // Fallback: use a hash of the description
                format!("{:08x}", simple_hash(&obs.observation.description))
            };

            let entry = motif_map.entry(hash.clone()).or_insert_with(|| MotifAccum {
                hash,
                count: 0,
                fields: Vec::new(),
            });
            entry.count += 1;

            // Extract field names from the path if available
            let path = &obs.observation.path;
            if !path.is_empty() && !path.starts_with("subtree:") {
                let field = path.rsplit('.').next().unwrap_or(path);
                if !entry.fields.contains(&field.to_string()) {
                    entry.fields.push(field.to_string());
                }
            }
        }
    }

    motif_map
        .values()
        .map(|m| {
            let mut obj = serde_json::Map::new();
            obj.insert("hash".to_string(), serde_json::json!(m.hash));
            obj.insert("count".to_string(), serde_json::json!(m.count));
            obj.insert("fields".to_string(), serde_json::json!(m.fields));
            serde_json::Value::Object(obj)
        })
        .collect()
}

struct MotifAccum {
    hash: String,
    count: usize,
    fields: Vec<String>,
}

/// Build anomaly entries using short keys.
fn build_anomalies(observations: &[ScoredObservation]) -> Vec<serde_json::Value> {
    observations
        .iter()
        .filter(|obs| obs.observation.anomaly_strength > 0.0)
        .filter(|obs| !obs.observation.path.starts_with("subtree:"))
        .map(|obs| {
            let mut obj = serde_json::Map::new();
            obj.insert("p".to_string(), serde_json::json!(obs.observation.path));

            // Determine anomaly type from description
            let t = classify_anomaly(&obs.observation.description);
            obj.insert("t".to_string(), serde_json::json!(t));

            obj.insert(
                "s".to_string(),
                serde_json::json!(round_f64(obs.observation.anomaly_strength, 3)),
            );

            // Add value and z-score for numeric outliers
            if obs.observation.description.contains("outlier") {
                if let Some(v) = extract_value(&obs.observation.description) {
                    obj.insert("v".to_string(), serde_json::json!(v));
                }
                if let Some(z) = extract_z_score(&obs.observation.description) {
                    obj.insert("z".to_string(), serde_json::json!(round_f64(z, 2)));
                }
            }

            serde_json::Value::Object(obj)
        })
        .collect()
}

/// Build notable (non-anomaly) entries using short keys.
fn build_notable(observations: &[ScoredObservation]) -> Vec<serde_json::Value> {
    observations
        .iter()
        .filter(|obs| obs.observation.anomaly_strength == 0.0)
        .filter(|obs| !obs.observation.path.starts_with("subtree:"))
        .map(|obs| {
            let mut obj = serde_json::Map::new();
            obj.insert("p".to_string(), serde_json::json!(obs.observation.path));

            let t = classify_notable(&obs.observation.description);
            obj.insert("t".to_string(), serde_json::json!(t));

            // Add distinct values count for enum-like fields
            if obs.observation.description.contains("enum-like") {
                if let Some(n) = extract_count(&obs.observation.description) {
                    obj.insert("n".to_string(), serde_json::json!(n));
                }
            }

            // Add null rate for frequently-null fields
            if obs.observation.description.contains("null") {
                if let Some(rate) = extract_percentage(&obs.observation.description) {
                    obj.insert("v".to_string(), serde_json::json!(round_f64(rate, 1)));
                }
            }

            // Add score
            obj.insert("s".to_string(), serde_json::json!(round_f64(obs.score, 4)));

            serde_json::Value::Object(obj)
        })
        .collect()
}

/// Build drill-down hints for chain-ready output.
fn build_drill(observations: &[ScoredObservation]) -> Vec<serde_json::Value> {
    // Collect unique paths (excluding subtree hashes) that have observations
    let mut path_capabilities: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for obs in observations {
        let path = &obs.observation.path;
        if path.is_empty() || path.starts_with("subtree:") {
            continue;
        }

        // Determine what capabilities this path offers
        let entry = path_capabilities
            .entry(path.clone())
            .or_insert_with(Vec::new);

        if obs.observation.anomaly_strength > 0.0 && !entry.contains(&"anomalies".to_string()) {
            entry.push("anomalies".to_string());
        }
        if obs.observation.entropy_signal > 0.0 && !entry.contains(&"stats".to_string()) {
            entry.push("stats".to_string());
        }
        if obs.observation.structural_coverage > 0.0 && !entry.contains(&"motifs".to_string()) {
            entry.push("motifs".to_string());
        }
        if entry.is_empty() && !entry.contains(&"stats".to_string()) {
            entry.push("stats".to_string());
        }
    }

    path_capabilities
        .into_iter()
        .map(|(path, available)| {
            serde_json::json!({
                "path": path,
                "available": available
            })
        })
        .collect()
}

/// Classify an anomaly type from its description.
fn classify_anomaly(description: &str) -> &'static str {
    if description.contains("outlier") {
        "numeric_outlier"
    } else if description.contains("type instability") || description.contains("mixed types") {
        "type_instability"
    } else if description.contains("rare") {
        "rare_value"
    } else {
        "anomaly"
    }
}

/// Classify a notable observation type from its description.
fn classify_notable(description: &str) -> &'static str {
    if description.contains("enum-like") {
        "enum"
    } else if description.contains("null") {
        "null_rate"
    } else if description.contains("informative") || description.contains("entropy") {
        "high_entropy"
    } else if description.contains("type instability") {
        "type_instability"
    } else {
        "observation"
    }
}

/// Extract a numeric value from a description like "value=350".
fn extract_value(description: &str) -> Option<f64> {
    let marker = "value=";
    let start = description.find(marker)? + marker.len();
    let rest = &description[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract a z-score from a description like "z_mad=4.20".
fn extract_z_score(description: &str) -> Option<f64> {
    let marker = "z_mad=";
    let start = description.find(marker)? + marker.len();
    let rest = &description[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract count from a description like "3 distinct values".
fn extract_count(description: &str) -> Option<usize> {
    // Pattern: "enum-like field (N distinct values over M observations)"
    let paren_start = description.find('(')? + 1;
    let rest = &description[paren_start..];
    let end = rest.find(' ')?;
    rest[..end].parse().ok()
}

/// Extract percentage from a description like "66.7% null rate".
fn extract_percentage(description: &str) -> Option<f64> {
    let pct_pos = description.find('%')?;
    // Walk backwards to find the start of the number
    let before = &description[..pct_pos];
    let start = before
        .rfind(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].parse().ok()
}

/// Round an f64 to a given number of decimal places.
fn round_f64(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

/// Simple string hash for fallback motif identification.
fn simple_hash(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u32::from(byte));
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::{CandidateObservation, ScoredObservation};

    fn make_test_essence() -> EssenceData {
        let observations = vec![
            ScoredObservation {
                observation: CandidateObservation {
                    path: "$.claims[*].allowed".to_string(),
                    description: "type instability (90.0% string, mixed types)".to_string(),
                    rarity: 0.0,
                    instability: 0.4,
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.4,
                    concern_relevance: 0.6,
                },
                score: 0.42,
            },
            ScoredObservation {
                observation: CandidateObservation {
                    path: "$.claims[*].charge".to_string(),
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
                    path: "$.claims[*].status".to_string(),
                    description: "enum-like field (3 distinct values over 5 observations)"
                        .to_string(),
                    rarity: 0.7,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.2,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.2,
                },
                score: 0.25,
            },
            ScoredObservation {
                observation: CandidateObservation {
                    path: "subtree:f2c1abcd".to_string(),
                    description: "repeated structure (motif appears 14 times)".to_string(),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.14,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.1,
                },
                score: 0.1,
            },
        ];

        EssenceData {
            observations,
            document_identity: "document(nodes=51, paths=24, depth=5)".to_string(),
            total_nodes: 51,
            distinct_paths: 24,
            max_depth: 5,
            truncated: false,
            budget_used: None,
            budget_limit: None,
        }
    }

    #[test]
    fn compact_ai_output_is_valid_json() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        assert!(
            result.is_object(),
            "compact-AI output should be a JSON object"
        );
    }

    #[test]
    fn compact_ai_has_expected_top_level_keys() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        let obj = result.as_object().unwrap();
        assert!(obj.contains_key("v"), "should have version key 'v'");
        assert!(obj.contains_key("doc"), "should have 'doc' key");
        assert!(obj.contains_key("motifs"), "should have 'motifs' key");
        assert!(obj.contains_key("anomalies"), "should have 'anomalies' key");
        assert!(obj.contains_key("notable"), "should have 'notable' key");
        assert!(obj.contains_key("meta"), "should have 'meta' key");
    }

    #[test]
    fn compact_ai_uses_short_keys() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        let serialized = serde_json::to_string(&result).unwrap();

        // Should NOT contain verbose keys
        assert!(
            !serialized.contains("\"path\""),
            "should use 'p' not 'path' for observations"
        );
        assert!(
            !serialized.contains("\"description\""),
            "should not contain 'description' key"
        );

        // Should contain short keys
        let anomalies = result["anomalies"].as_array().unwrap();
        if !anomalies.is_empty() {
            let first = anomalies[0].as_object().unwrap();
            assert!(first.contains_key("p"), "anomaly should use 'p' for path");
            assert!(first.contains_key("t"), "anomaly should use 't' for type");
            assert!(
                first.contains_key("s"),
                "anomaly should use 's' for score/strength"
            );
        }
    }

    #[test]
    fn compact_ai_chain_ready_includes_drill() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, true);
        assert!(
            result.get("drill").is_some(),
            "chain-ready output should include 'drill' section"
        );
        let drill = result["drill"].as_array().unwrap();
        assert!(
            !drill.is_empty(),
            "drill section should have entries for paths"
        );
        // Each drill entry should have path and available
        for entry in drill {
            assert!(entry.get("path").is_some(), "drill entry needs 'path'");
            assert!(
                entry.get("available").is_some(),
                "drill entry needs 'available'"
            );
        }
    }

    #[test]
    fn compact_ai_no_drill_when_not_chain_ready() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        assert!(
            result.get("drill").is_none(),
            "non-chain-ready output should NOT include 'drill'"
        );
    }

    #[test]
    fn compact_ai_motifs_collapsed() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        let motifs = result["motifs"].as_array().unwrap();
        // We have one motif observation; it should be collapsed
        assert!(!motifs.is_empty(), "should have at least one motif entry");
        let first_motif = motifs[0].as_object().unwrap();
        assert!(first_motif.contains_key("hash"), "motif should have 'hash'");
        assert!(
            first_motif.contains_key("count"),
            "motif should have 'count'"
        );
        assert!(
            first_motif.contains_key("fields"),
            "motif should have 'fields'"
        );
    }

    #[test]
    fn compact_ai_anomalies_separate_from_notable() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);

        let anomalies = result["anomalies"].as_array().unwrap();
        let notable = result["notable"].as_array().unwrap();

        // The type_instability and numeric_outlier should be in anomalies
        assert!(!anomalies.is_empty(), "should have anomaly entries");
        // The enum-like should be in notable
        assert!(!notable.is_empty(), "should have notable entries");

        // Verify no overlap: anomaly paths should not appear in notable
        let anomaly_paths: Vec<&str> = anomalies.iter().filter_map(|a| a["p"].as_str()).collect();
        let notable_paths: Vec<&str> = notable.iter().filter_map(|n| n["p"].as_str()).collect();

        for ap in &anomaly_paths {
            // A path CAN appear in both if it has both anomaly and non-anomaly observations
            // but our test data has each path only once
            assert!(
                !notable_paths.contains(ap),
                "path {ap} should not be in both anomalies and notable"
            );
        }
    }

    #[test]
    fn compact_ai_version_tag() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        assert_eq!(result["v"].as_str().unwrap(), "vajra/1");
    }

    #[test]
    fn compact_ai_doc_section_matches_metadata() {
        let essence = make_test_essence();
        let result = render_compact_ai(&essence, false);
        let doc = result.get("doc").unwrap();
        assert_eq!(doc["nodes"].as_u64().unwrap(), 51);
        assert_eq!(doc["paths"].as_u64().unwrap(), 24);
        assert_eq!(doc["depth"].as_u64().unwrap(), 5);
    }

    #[test]
    fn compact_ai_meta_truncation_field() {
        let mut essence = make_test_essence();
        essence.truncated = true;
        essence.budget_used = Some(450);
        essence.budget_limit = Some(500);

        let result = render_compact_ai(&essence, false);
        let meta = result.get("meta").unwrap();
        assert_eq!(meta["truncated"].as_bool().unwrap(), true);
        assert_eq!(meta["budget_used"].as_u64().unwrap(), 450);
        assert_eq!(meta["budget_limit"].as_u64().unwrap(), 500);
    }

    #[test]
    fn compact_ai_empty_essence() {
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
        let result = render_compact_ai(&essence, true);
        assert!(result.is_object());
        assert_eq!(result["anomalies"].as_array().unwrap().len(), 0);
        assert_eq!(result["notable"].as_array().unwrap().len(), 0);
        assert_eq!(result["motifs"].as_array().unwrap().len(), 0);
        // drill should exist but be empty
        assert_eq!(result["drill"].as_array().unwrap().len(), 0);
    }
}
