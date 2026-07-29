//! Rayon-based batch parallel processing for multiple documents.
//!
//! Parses and analyzes each document independently using `rayon::par_iter()`,
//! then computes aggregate statistics across the entire batch.
//!
//! File selection and parsing are both injected by the caller, so batch runs
//! honour the same `--input-format` resolution as every other command instead
//! of hardcoding JSON.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;

use vajra_anomaly::{AnomalyAnalyzer, AnomalyReport};
#[cfg(test)]
use vajra_core::parse_str;
use vajra_fingerprint::{FingerprintAnalyzer, FingerprintResult};
use vajra_stats::{StatsAnalyzer, StatsResult};
use vajra_types::traits::Analyzer;
use vajra_types::Document;

/// Intermediate result from parallel per-document analysis.
type DocAnalysisResult = Result<(Document, StatsResult, AnomalyReport, FingerprintResult)>;

/// Per-document analysis results.
#[derive(Debug)]
#[allow(dead_code)]
pub struct DocumentAnalysis {
    /// Index in the original input list.
    pub index: usize,
    /// File name or identifier.
    pub file_name: String,
    /// Parsed document.
    pub document: Document,
    /// Statistics analysis result.
    pub stats: StatsResult,
    /// Anomaly detection result.
    pub anomalies: AnomalyReport,
    /// Structural fingerprint result.
    pub fingerprint: FingerprintResult,
}

/// Aggregate statistics across all documents in the batch.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateStats {
    /// Total number of documents successfully analyzed.
    pub total_documents: usize,
    /// Total nodes across all documents.
    pub total_nodes: u64,
    /// How many documents contain each path.
    pub path_frequency: BTreeMap<String, u64>,
    /// Paths present in >50% of documents.
    pub common_paths: Vec<String>,
    /// Paths present in <10% of documents.
    pub rare_paths: Vec<String>,
}

/// Results from batch analysis of multiple documents.
#[derive(Debug)]
pub struct BatchResult {
    /// Per-document analysis results (indexed same as input).
    pub per_document: Vec<DocumentAnalysis>,
    /// Aggregate statistics across all documents.
    pub aggregate: AggregateStats,
    /// Files that failed to parse or analyze, with error messages.
    pub errors: Vec<(String, String)>,
}

/// The files a directory scan chose, and the ones it passed over.
///
/// `skipped` exists so callers can report what was *not* analyzed. A batch
/// that silently drops most of a directory and reports no errors is
/// indistinguishable from a complete run.
#[derive(Debug, Default)]
pub struct BatchFiles {
    /// Files that matched the selector, sorted for deterministic ordering.
    pub selected: Vec<PathBuf>,
    /// Files present in the directory that the selector rejected, sorted.
    pub skipped: Vec<PathBuf>,
}

/// Analyze a batch of documents in parallel using Rayon.
///
/// Each document is parsed and analyzed independently, then aggregate
/// statistics are computed from the per-document results.
///
/// `load` performs the parse, so the caller decides how each file is read.
/// Passing the CLI's format-aware loader is what lets batch runs honour
/// `--input-format` rather than assuming JSON.
///
/// # Errors
///
/// Returns an error if the batch is empty. Individual file failures are
/// collected in `BatchResult::errors` rather than failing the entire batch.
pub fn analyze_batch(
    file_paths: &[PathBuf],
    load: &(dyn Fn(&Path) -> Result<Document> + Send + Sync),
) -> Result<BatchResult> {
    if file_paths.is_empty() {
        anyhow::bail!("no files to analyze (empty batch)");
    }

    // Phase 1: Parse and analyze each document in parallel
    let results: Vec<(usize, String, DocAnalysisResult)> = file_paths
        .par_iter()
        .enumerate()
        .map(|(index, path)| {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let result = load(path)
                .with_context(|| format!("failed to parse {}", path.display()))
                .and_then(analyze_document);
            (index, file_name, result)
        })
        .collect();

    // Phase 2: Separate successes from failures
    let mut per_document = Vec::new();
    let mut errors = Vec::new();

    for (index, file_name, result) in results {
        match result {
            Ok((document, stats, anomalies, fingerprint)) => {
                per_document.push(DocumentAnalysis {
                    index,
                    file_name,
                    document,
                    stats,
                    anomalies,
                    fingerprint,
                });
            }
            Err(e) => {
                errors.push((file_name, format!("{e:#}")));
            }
        }
    }

    // Sort by original index to maintain deterministic ordering
    per_document.sort_by_key(|d| d.index);

    // Phase 3: Compute aggregate statistics
    let aggregate = compute_aggregate(&per_document);

    Ok(BatchResult {
        per_document,
        aggregate,
        errors,
    })
}

/// Analyze a batch from JSON strings (used for testing).
///
/// Each entry is `(name, json_content)`.
///
/// # Errors
///
/// Returns an error if the input is empty.
#[cfg(test)]
pub fn analyze_batch_from_strings(inputs: &[(String, String)]) -> Result<BatchResult> {
    if inputs.is_empty() {
        anyhow::bail!("no documents to analyze (empty batch)");
    }

    let results: Vec<(usize, String, DocAnalysisResult)> = inputs
        .par_iter()
        .enumerate()
        .map(|(index, (name, json))| {
            let result = analyze_single_str(json);
            (index, name.clone(), result)
        })
        .collect();

    let mut per_document = Vec::new();
    let mut errors = Vec::new();

    for (index, file_name, result) in results {
        match result {
            Ok((document, stats, anomalies, fingerprint)) => {
                per_document.push(DocumentAnalysis {
                    index,
                    file_name,
                    document,
                    stats,
                    anomalies,
                    fingerprint,
                });
            }
            Err(e) => {
                errors.push((file_name, format!("{e:#}")));
            }
        }
    }

    per_document.sort_by_key(|d| d.index);

    let aggregate = compute_aggregate(&per_document);

    Ok(BatchResult {
        per_document,
        aggregate,
        errors,
    })
}

/// Analyze a single JSON string.
#[cfg(test)]
fn analyze_single_str(
    json: &str,
) -> Result<(Document, StatsResult, AnomalyReport, FingerprintResult)> {
    let doc = parse_str(json).context("failed to parse JSON")?;
    analyze_document(doc)
}

/// Run all analyzers on a parsed document.
fn analyze_document(
    doc: Document,
) -> Result<(Document, StatsResult, AnomalyReport, FingerprintResult)> {
    let stats = StatsAnalyzer
        .analyze(&doc)
        .context("stats analysis failed")?;
    let anomalies = AnomalyAnalyzer::default()
        .analyze(&doc)
        .context("anomaly analysis failed")?;
    let fingerprint = FingerprintAnalyzer
        .analyze(&doc)
        .context("fingerprint analysis failed")?;
    Ok((doc, stats, anomalies, fingerprint))
}

/// Compute aggregate statistics from per-document results.
#[allow(clippy::cast_precision_loss)]
fn compute_aggregate(docs: &[DocumentAnalysis]) -> AggregateStats {
    let total_documents = docs.len();
    let total_nodes: u64 = docs.iter().map(|d| d.document.metadata().total_nodes).sum();

    // Count how many documents contain each path
    let mut path_frequency: BTreeMap<String, u64> = BTreeMap::new();
    for doc in docs {
        let paths = doc.document.trie().all_paths();
        for path in paths {
            *path_frequency.entry(path.to_string()).or_insert(0) += 1;
        }
    }

    // Common paths: present in >50% of documents
    let threshold_common = (total_documents as f64 * 0.5).ceil() as u64;
    let common_paths: Vec<String> = path_frequency
        .iter()
        .filter(|(_, &count)| count >= threshold_common)
        .map(|(path, _)| path.clone())
        .collect();

    // Rare paths: present in <10% of documents
    // For small batches, use a threshold of at most 1 (i.e. paths appearing
    // in only 1 doc are "rare" when there are at least 2 docs).
    let threshold_rare = if total_documents <= 10 {
        2 // paths in 1 doc are rare when batch size <= 10
    } else {
        (total_documents as f64 * 0.1).ceil() as u64
    };
    let rare_paths: Vec<String> = if total_documents > 1 {
        path_frequency
            .iter()
            .filter(|(_, &count)| count < threshold_rare)
            .map(|(path, _)| path.clone())
            .collect()
    } else {
        Vec::new()
    };

    AggregateStats {
        total_documents,
        total_nodes,
        path_frequency,
        common_paths,
        rare_paths,
    }
}

/// Partition a directory's files into those `accept` selects and those it does
/// not.
///
/// Subdirectories are ignored entirely and are not reported as skipped.
///
/// # Errors
///
/// Returns an error if the path is not a directory, or cannot be read.
pub fn collect_batch_files(dir: &Path, accept: &dyn Fn(&Path) -> bool) -> Result<BatchFiles> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }

    let mut files = BatchFiles::default();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if accept(&path) {
            files.selected.push(path);
        } else {
            files.skipped.push(path);
        }
    }

    // Sort for deterministic ordering
    files.selected.sort();
    files.skipped.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn batch_identical_files() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"name": "Alice", "age": 30, "scores": [95, 88, 72]}"#;
        let inputs: Vec<(String, String)> = vec![
            ("file1.json".to_string(), json.to_string()),
            ("file2.json".to_string(), json.to_string()),
            ("file3.json".to_string(), json.to_string()),
        ];

        let result = analyze_batch_from_strings(&inputs)?;

        assert_eq!(result.per_document.len(), 3);
        assert!(result.errors.is_empty());
        assert_eq!(result.aggregate.total_documents, 3);

        // All three documents should have the same node count
        let counts: Vec<u64> = result
            .per_document
            .iter()
            .map(|d| d.document.metadata().total_nodes)
            .collect();
        assert_eq!(counts[0], counts[1]);
        assert_eq!(counts[1], counts[2]);

        // All fingerprints should be identical
        assert_eq!(
            result.per_document[0].fingerprint.path_set,
            result.per_document[1].fingerprint.path_set,
        );
        assert_eq!(
            result.per_document[1].fingerprint.path_set,
            result.per_document[2].fingerprint.path_set,
        );
        Ok(())
    }

    #[test]
    fn batch_mixed_files() -> Result<(), Box<dyn std::error::Error>> {
        let json_a = r#"{"name": "Alice", "age": 30, "city": "NYC"}"#;
        let json_b = r#"{"name": "Bob", "score": 95, "tags": ["a", "b"]}"#;
        let json_c = r#"{"name": "Charlie", "age": 25, "city": "LA"}"#;
        let inputs: Vec<(String, String)> = vec![
            ("a.json".to_string(), json_a.to_string()),
            ("b.json".to_string(), json_b.to_string()),
            ("c.json".to_string(), json_c.to_string()),
        ];

        let result = analyze_batch_from_strings(&inputs)?;

        assert_eq!(result.per_document.len(), 3);
        assert_eq!(result.aggregate.total_documents, 3);

        // $.name is common (in all 3 docs)
        assert!(
            result
                .aggregate
                .common_paths
                .contains(&"$.name".to_string()),
            "$.name should be a common path, common_paths = {:?}",
            result.aggregate.common_paths,
        );

        // $.tags and $.tags[*] are rare (only in 1 of 3 docs)
        let has_rare_tags = result
            .aggregate
            .rare_paths
            .iter()
            .any(|p| p.contains("tags"));
        assert!(
            has_rare_tags,
            "tags paths should be rare, rare_paths = {:?}",
            result.aggregate.rare_paths,
        );
        Ok(())
    }

    #[test]
    fn batch_empty_returns_error() {
        let inputs: Vec<(String, String)> = vec![];
        let result = analyze_batch_from_strings(&inputs);
        assert!(result.is_err());
    }

    #[test]
    fn batch_single_file() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"x": 1}"#;
        let inputs = vec![("only.json".to_string(), json.to_string())];
        let result = analyze_batch_from_strings(&inputs)?;

        assert_eq!(result.per_document.len(), 1);
        assert_eq!(result.aggregate.total_documents, 1);
        assert!(result.errors.is_empty());
        // Single file: all paths are common (>50%), none are rare
        assert!(!result.aggregate.common_paths.is_empty());
        assert!(result.aggregate.rare_paths.is_empty());
        Ok(())
    }

    /// The `.json` selector and loader the CLI uses by default.
    fn json_only(path: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "json")
    }

    fn load_json(path: &Path) -> Result<Document> {
        Ok(vajra_core::parse_file(path)?)
    }

    #[test]
    fn batch_from_directory() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let dir_path = dir.path();

        // Create test JSON files
        fs::write(dir_path.join("a.json"), r#"{"name": "Alice", "age": 30}"#)?;

        fs::write(dir_path.join("b.json"), r#"{"name": "Bob", "age": 25}"#)?;

        // Non-JSON file should be skipped, and reported as such.
        fs::write(dir_path.join("readme.txt"), "not json")?;

        let files = collect_batch_files(dir_path, &json_only)?;
        assert_eq!(files.selected.len(), 2);
        assert_eq!(files.skipped.len(), 1, "readme.txt must be reported");
        assert!(files.skipped[0].ends_with("readme.txt"));

        let result = analyze_batch(&files.selected, &load_json)?;
        assert_eq!(result.per_document.len(), 2);
        assert!(result.errors.is_empty());
        Ok(())
    }

    /// A directory whose files are all rejected must report every one of them
    /// rather than looking like an empty directory.
    #[test]
    fn skipped_files_are_all_reported() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        for name in ["a.js", "b.js", "c.yaml", "notes.md"] {
            fs::write(dir.path().join(name), "x")?;
        }
        fs::create_dir(dir.path().join("nested"))?;

        let files = collect_batch_files(dir.path(), &json_only)?;
        assert!(files.selected.is_empty());
        assert_eq!(files.skipped.len(), 4, "subdirectory must not be counted");
        Ok(())
    }

    #[test]
    fn collect_batch_files_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        for name in ["z.json", "a.json", "m.json", "q.txt", "b.txt"] {
            fs::write(dir.path().join(name), r#"{"a": 1}"#)?;
        }

        let first = collect_batch_files(dir.path(), &json_only)?;
        let second = collect_batch_files(dir.path(), &json_only)?;
        assert_eq!(first.selected, second.selected);
        assert_eq!(first.skipped, second.skipped);
        assert!(first.selected.windows(2).all(|w| w[0] <= w[1]), "sorted");
        assert!(first.skipped.windows(2).all(|w| w[0] <= w[1]), "sorted");
        Ok(())
    }

    /// The selector is injected, so a non-JSON batch is selectable without
    /// touching this module.
    #[test]
    fn custom_selector_changes_selection() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("keep.js"), "const a = 1;")?;
        fs::write(dir.path().join("drop.json"), r#"{"a": 1}"#)?;

        let js_only = |p: &Path| p.extension().is_some_and(|e| e == "js");
        let files = collect_batch_files(dir.path(), &js_only)?;
        assert_eq!(files.selected.len(), 1);
        assert!(files.selected[0].ends_with("keep.js"));
        assert_eq!(files.skipped.len(), 1);
        Ok(())
    }

    #[test]
    fn batch_preserves_order() -> Result<(), Box<dyn std::error::Error>> {
        let inputs: Vec<(String, String)> = vec![
            ("first.json".to_string(), r#"{"a": 1}"#.to_string()),
            ("second.json".to_string(), r#"{"b": 2}"#.to_string()),
            ("third.json".to_string(), r#"{"c": 3}"#.to_string()),
        ];

        let result = analyze_batch_from_strings(&inputs)?;

        assert_eq!(result.per_document[0].file_name, "first.json");
        assert_eq!(result.per_document[1].file_name, "second.json");
        assert_eq!(result.per_document[2].file_name, "third.json");
        assert_eq!(result.per_document[0].index, 0);
        assert_eq!(result.per_document[1].index, 1);
        assert_eq!(result.per_document[2].index, 2);
        Ok(())
    }

    #[test]
    fn batch_handles_parse_errors_gracefully() -> Result<(), Box<dyn std::error::Error>> {
        let inputs: Vec<(String, String)> = vec![
            ("good.json".to_string(), r#"{"a": 1}"#.to_string()),
            ("bad.json".to_string(), "not valid json{{{".to_string()),
            ("also_good.json".to_string(), r#"{"b": 2}"#.to_string()),
        ];

        let result = analyze_batch_from_strings(&inputs)?;

        assert_eq!(result.per_document.len(), 2);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].0, "bad.json");
        Ok(())
    }

    #[test]
    fn empty_directory_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let files = collect_batch_files(dir.path(), &json_only)?;
        assert!(files.selected.is_empty());
        assert!(files.skipped.is_empty());

        let result = analyze_batch(&files.selected, &load_json);
        assert!(result.is_err());
        Ok(())
    }
}
