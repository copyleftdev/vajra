//! Structured chunking for large essences that exceed a token budget.
//!
//! Each chunk includes document context so an LLM can process it independently.

use vajra_types::traits::{EssenceData, ScoredObservation};

use crate::tokens::estimate_tokens;

/// A single chunk of an essence, suitable for independent LLM processing.
#[derive(Debug, Clone)]
pub struct EssenceChunk {
    /// Zero-based index of this chunk.
    pub chunk_index: usize,
    /// Total number of chunks.
    pub total_chunks: usize,
    /// The chunk's data as JSON.
    pub content: serde_json::Value,
    /// Structural context preserved across chunks.
    pub context: ChunkContext,
}

/// Structural context included in every chunk for independent processing.
#[derive(Debug, Clone)]
pub struct ChunkContext {
    /// Document identity string.
    pub document_identity: String,
    /// Total nodes in the full document.
    pub total_nodes: u64,
    /// Distinct paths in the full document.
    pub distinct_paths: u32,
    /// What this chunk covers (e.g., "anomalies", "structure", "path:$.claims").
    pub chunk_focus: String,
}

/// Split an essence into token-budget-sized chunks.
///
/// Each chunk includes document context so an LLM can process it independently.
///
/// Chunking strategy:
/// 1. Always include document summary in every chunk (identity, node count, paths)
/// 2. Group observations by category: anomalies, structural, statistical
/// 3. Each chunk gets one category (or part of one if it's too large)
/// 4. Each chunk is self-contained
#[must_use]
pub fn chunk_essence(essence: &EssenceData, tokens_per_chunk: usize) -> Vec<EssenceChunk> {
    // Categorize observations
    let anomalies: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| o.observation.anomaly_strength > 0.0)
        .collect();

    let structural: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| {
            o.observation.anomaly_strength == 0.0
                && (o.observation.structural_coverage > 0.0
                    || o.observation.path.starts_with("subtree:"))
        })
        .collect();

    let statistical: Vec<&ScoredObservation> = essence
        .observations
        .iter()
        .filter(|o| {
            o.observation.anomaly_strength == 0.0
                && o.observation.structural_coverage == 0.0
                && !o.observation.path.starts_with("subtree:")
        })
        .collect();

    // Estimate the context overhead (document summary included in every chunk)
    let context_json = build_context_json(essence);
    let context_overhead =
        estimate_tokens(&serde_json::to_string(&context_json).unwrap_or_default());

    let available_per_chunk = if tokens_per_chunk > context_overhead {
        tokens_per_chunk - context_overhead
    } else {
        // Even if budget is tiny, allow at least 1 observation per chunk
        1
    };

    // Build category groups
    let categories: Vec<(&str, Vec<&ScoredObservation>)> = vec![
        ("anomalies", anomalies),
        ("structural", structural),
        ("statistical", statistical),
    ];

    let mut raw_chunks: Vec<(String, Vec<serde_json::Value>)> = Vec::new();

    for (category_name, obs_list) in &categories {
        if obs_list.is_empty() {
            continue;
        }

        let mut current_items: Vec<serde_json::Value> = Vec::new();
        let mut current_tokens: usize = 0;

        for obs in obs_list {
            let obs_json = observation_to_compact_json(obs);
            let obs_text = serde_json::to_string(&obs_json).unwrap_or_default();
            let obs_tokens = estimate_tokens(&obs_text);

            if current_tokens + obs_tokens > available_per_chunk && !current_items.is_empty() {
                // Start a new chunk for this category
                raw_chunks.push((category_name.to_string(), current_items));
                current_items = Vec::new();
                current_tokens = 0;
            }

            current_items.push(obs_json);
            current_tokens += obs_tokens;
        }

        if !current_items.is_empty() {
            raw_chunks.push((category_name.to_string(), current_items));
        }
    }

    // If no observations at all, produce a single empty chunk
    if raw_chunks.is_empty() {
        raw_chunks.push(("structure".to_string(), Vec::new()));
    }

    let total_chunks = raw_chunks.len();

    raw_chunks
        .into_iter()
        .enumerate()
        .map(|(idx, (focus, items))| {
            let mut content = context_json.clone();
            if let Some(obj) = content.as_object_mut() {
                obj.insert("focus".to_string(), serde_json::json!(focus));
                obj.insert("observations".to_string(), serde_json::Value::Array(items));
                obj.insert("chunk".to_string(), serde_json::json!(idx + 1));
                obj.insert("total_chunks".to_string(), serde_json::json!(total_chunks));
            }

            EssenceChunk {
                chunk_index: idx,
                total_chunks,
                content,
                context: ChunkContext {
                    document_identity: essence.document_identity.clone(),
                    total_nodes: essence.total_nodes,
                    distinct_paths: essence.distinct_paths,
                    chunk_focus: focus,
                },
            }
        })
        .collect()
}

/// Build the context JSON included in every chunk.
fn build_context_json(essence: &EssenceData) -> serde_json::Value {
    serde_json::json!({
        "v": "vajra/1",
        "doc": {
            "identity": essence.document_identity,
            "nodes": essence.total_nodes,
            "paths": essence.distinct_paths,
            "depth": essence.max_depth
        }
    })
}

/// Convert a single observation to compact JSON for inclusion in a chunk.
fn observation_to_compact_json(obs: &ScoredObservation) -> serde_json::Value {
    serde_json::json!({
        "p": obs.observation.path,
        "d": obs.observation.description,
        "s": obs.score,
        "a": obs.observation.anomaly_strength
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::CandidateObservation;

    fn make_large_essence(n_anomalies: usize, n_structural: usize, n_stats: usize) -> EssenceData {
        let mut observations = Vec::new();

        for i in 0..n_anomalies {
            observations.push(ScoredObservation {
                observation: CandidateObservation {
                    path: format!("$.anomaly_path_{i}"),
                    description: format!(
                        "numeric outlier (value={}, z_mad={:.2})",
                        i * 100,
                        3.0 + i as f64
                    ),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.5 + (i as f64 * 0.05),
                    concern_relevance: 0.5,
                },
                score: 0.8 - (i as f64 * 0.02),
            });
        }

        for i in 0..n_structural {
            observations.push(ScoredObservation {
                observation: CandidateObservation {
                    path: format!("subtree:{i:08x}"),
                    description: format!("repeated structure (motif appears {} times)", i + 2),
                    rarity: 0.0,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.3,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.1,
                },
                score: 0.3 - (i as f64 * 0.01),
            });
        }

        for i in 0..n_stats {
            observations.push(ScoredObservation {
                observation: CandidateObservation {
                    path: format!("$.stat_field_{i}"),
                    description: format!(
                        "enum-like field ({} distinct values over {} observations)",
                        i + 2,
                        (i + 1) * 10
                    ),
                    rarity: 0.5,
                    instability: 0.0,
                    entropy_signal: 0.0,
                    structural_coverage: 0.0,
                    anomaly_strength: 0.0,
                    concern_relevance: 0.2,
                },
                score: 0.2 - (i as f64 * 0.01),
            });
        }

        EssenceData {
            observations,
            document_identity: "test-doc(large)".to_string(),
            total_nodes: 500,
            distinct_paths: 50,
            max_depth: 8,
            truncated: false,
            budget_used: None,
            budget_limit: None,
        }
    }

    #[test]
    fn large_essence_splits_into_multiple_chunks() {
        let essence = make_large_essence(10, 5, 10);
        // Use a small token budget to force splitting
        let chunks = chunk_essence(&essence, 50);
        assert!(
            chunks.len() > 1,
            "large essence with small budget should split into multiple chunks, got {}",
            chunks.len()
        );
    }

    #[test]
    fn each_chunk_includes_context() {
        let essence = make_large_essence(5, 3, 5);
        let chunks = chunk_essence(&essence, 100);

        for chunk in &chunks {
            assert_eq!(
                chunk.context.document_identity, "test-doc(large)",
                "each chunk should preserve document identity"
            );
            assert_eq!(chunk.context.total_nodes, 500);
            assert_eq!(chunk.context.distinct_paths, 50);
            assert!(
                !chunk.context.chunk_focus.is_empty(),
                "chunk_focus should not be empty"
            );

            // Content should also have doc section
            let content = &chunk.content;
            assert!(
                content.get("doc").is_some(),
                "content should have doc context"
            );
        }
    }

    #[test]
    fn total_observations_across_chunks_equals_original() {
        let essence = make_large_essence(5, 3, 5);
        let total_original = essence.observations.len();
        let chunks = chunk_essence(&essence, 80);

        let total_in_chunks: usize = chunks
            .iter()
            .map(|c| {
                c.content
                    .get("observations")
                    .and_then(|v| v.as_array())
                    .map(Vec::len)
                    .unwrap_or(0)
            })
            .sum();

        assert_eq!(
            total_in_chunks, total_original,
            "total observations in chunks ({total_in_chunks}) should equal original ({total_original})"
        );
    }

    #[test]
    fn chunk_indices_correct() {
        let essence = make_large_essence(5, 3, 5);
        let chunks = chunk_essence(&essence, 80);

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i, "chunk_index should match position");
            assert_eq!(
                chunk.total_chunks,
                chunks.len(),
                "total_chunks should be consistent"
            );
        }
    }

    #[test]
    fn empty_essence_produces_single_chunk() {
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
        let chunks = chunk_essence(&essence, 500);
        assert_eq!(chunks.len(), 1, "empty essence should produce 1 chunk");
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].total_chunks, 1);
    }

    #[test]
    fn large_budget_produces_few_chunks() {
        let essence = make_large_essence(3, 2, 3);
        let chunks = chunk_essence(&essence, 100_000);
        // With a huge budget, each category should fit in one chunk
        // 3 non-empty categories => at most 3 chunks
        assert!(
            chunks.len() <= 3,
            "large budget should produce at most one chunk per category, got {}",
            chunks.len()
        );
    }
}
