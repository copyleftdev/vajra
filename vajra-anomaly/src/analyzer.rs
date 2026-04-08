//! Anomaly analyzer implementing the [`Analyzer`] trait.
//!
//! Orchestrates numeric outlier detection, rarity scoring, and type
//! instability detection across a document.

use crate::instability::{detect_type_instabilities, TypeInstability};
use crate::numeric::{detect_numeric_outliers, NumericOutlier};
use crate::rarity::{detect_rare_values, RarityOutlier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vajra_types::document::Document;
use vajra_types::error::VajraError;
use vajra_types::path::{PathSegment, WildcardPath};
use vajra_types::traits::Analyzer;
use vajra_types::trie::TrieNode;

/// Configuration for the anomaly analyzer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyAnalyzer {
    /// Modified z-score threshold for MAD-based numeric outlier detection.
    pub mad_threshold: f64,
    /// Sigma multiplier for rarity outlier detection.
    pub rarity_threshold_sigma: f64,
    /// Type instability threshold (0.0 = flag everything, 1.0 = flag nothing).
    pub instability_threshold: f64,
}

impl Default for AnomalyAnalyzer {
    fn default() -> Self {
        Self {
            mad_threshold: 3.5,
            rarity_threshold_sigma: 2.0,
            instability_threshold: 0.01,
        }
    }
}

/// The output of anomaly analysis on a document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnomalyReport {
    /// Numeric outliers detected via MAD-based z-scores.
    pub numeric_outliers: Vec<NumericOutlier>,
    /// Values flagged as rare based on self-information.
    pub rare_values: Vec<RarityOutlier>,
    /// Paths with unstable (mixed) types.
    pub type_instabilities: Vec<TypeInstability>,
}

impl Analyzer for AnomalyAnalyzer {
    type Output = AnomalyReport;

    fn analyze(&self, doc: &Document) -> Result<Self::Output, VajraError> {
        let trie = doc.trie();

        // Detect type instabilities
        let type_instabilities = detect_type_instabilities(trie, self.instability_threshold);

        // Collect numeric values and string value counts per path by walking the JSON tree
        let mut numeric_values_by_path: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut string_counts_by_path: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        let mut string_totals_by_path: BTreeMap<String, u64> = BTreeMap::new();

        // Walk the value tree to extract numeric values and string counts
        collect_values(
            doc.value(),
            &WildcardPath::root(),
            trie.root(),
            &mut numeric_values_by_path,
            &mut string_counts_by_path,
            &mut string_totals_by_path,
        );

        // Detect numeric outliers per path
        let mut numeric_outliers = Vec::new();
        for (path, mut values) in numeric_values_by_path {
            let outlier_scores = detect_numeric_outliers(&mut values, self.mad_threshold);
            for score in outlier_scores {
                numeric_outliers.push(NumericOutlier {
                    path: path.clone(),
                    value: score.value,
                    z_mad: score.z_mad,
                    median: score.median,
                    mad: score.mad,
                });
            }
        }

        // Detect rare values per path
        let mut rare_values = Vec::new();
        for (path, counts) in &string_counts_by_path {
            let total = string_totals_by_path.get(path).copied().unwrap_or(0);
            let rare = detect_rare_values(counts, total, self.rarity_threshold_sigma);
            rare_values.extend(rare);
        }

        Ok(AnomalyReport {
            numeric_outliers,
            rare_values,
            type_instabilities,
        })
    }
}

/// Recursively collect numeric values and string value counts from the JSON tree.
fn collect_values(
    value: &serde_json::Value,
    current_path: &WildcardPath,
    trie_node: &TrieNode,
    numeric_values: &mut BTreeMap<String, Vec<f64>>,
    string_counts: &mut BTreeMap<String, BTreeMap<String, u64>>,
    string_totals: &mut BTreeMap<String, u64>,
) {
    match value {
        serde_json::Value::Number(n) => {
            let path_str = current_path.to_string();
            if let Some(f) = n.as_f64() {
                numeric_values.entry(path_str).or_default().push(f);
            }
        }
        serde_json::Value::String(s) => {
            let path_str = current_path.to_string();
            *string_counts
                .entry(path_str.clone())
                .or_default()
                .entry(s.clone())
                .or_insert(0) += 1;
            *string_totals.entry(path_str).or_insert(0) += 1;
        }
        serde_json::Value::Array(arr) => {
            let child_path = current_path.push_array_wildcard();
            let child_node = trie_node.children.get(&PathSegment::ArrayWildcard);
            for item in arr {
                if let Some(node) = child_node {
                    collect_values(
                        item,
                        &child_path,
                        node,
                        numeric_values,
                        string_counts,
                        string_totals,
                    );
                }
            }
        }
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                let child_path = current_path.push_key(key);
                let child_node = trie_node.children.get(&PathSegment::Key(key.clone()));
                if let Some(node) = child_node {
                    collect_values(
                        val,
                        &child_path,
                        node,
                        numeric_values,
                        string_counts,
                        string_totals,
                    );
                }
            }
        }
        // Null, Bool - not collected for anomaly analysis
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::document::{Document, DocumentMetadata};
    use vajra_types::json_type::JsonType;
    use vajra_types::trie::PathTrie;

    fn make_document(json_str: &str) -> Document {
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
        let mut trie = PathTrie::new();
        build_trie(&value, &WildcardPath::root(), &mut trie);
        let metadata = DocumentMetadata {
            total_nodes: 0,
            max_depth: 0,
            #[allow(clippy::cast_possible_truncation)]
            distinct_paths: trie.path_count() as u32,
            #[allow(clippy::cast_possible_truncation)]
            raw_size_bytes: json_str.len() as u64,
        };
        Document::new(value, trie, metadata)
    }

    fn build_trie(value: &serde_json::Value, path: &WildcardPath, trie: &mut PathTrie) {
        let jt = JsonType::from_value(value);
        let depth = path.depth();
        trie.get_or_create(path).metadata.observe(jt, depth);

        match value {
            serde_json::Value::Object(obj) => {
                for (key, val) in obj {
                    let child_path = path.push_key(key);
                    build_trie(val, &child_path, trie);
                }
            }
            serde_json::Value::Array(arr) => {
                let child_path = path.push_array_wildcard();
                for item in arr {
                    build_trie(item, &child_path, trie);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn analyzer_default_thresholds() {
        let analyzer = AnomalyAnalyzer::default();
        assert!((analyzer.mad_threshold - 3.5).abs() < f64::EPSILON);
        assert!((analyzer.rarity_threshold_sigma - 2.0).abs() < f64::EPSILON);
        assert!((analyzer.instability_threshold - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn analyzer_detects_type_instability() {
        // Build a trie with mixed types manually
        let mut trie = PathTrie::new();

        let path = WildcardPath::root().push_key("amount");
        let node = trie.get_or_create(&path);
        for _ in 0..90 {
            node.metadata.observe(JsonType::Float, 1);
        }
        for _ in 0..10 {
            node.metadata.observe(JsonType::String, 1);
        }

        let root_meta = &mut trie.root_mut().metadata;
        root_meta.observe(JsonType::Object, 0);

        let metadata = DocumentMetadata {
            total_nodes: 101,
            max_depth: 1,
            distinct_paths: 2,
            raw_size_bytes: 100,
        };
        let doc = Document::new(serde_json::json!({"amount": 42.0}), trie, metadata);

        let analyzer = AnomalyAnalyzer::default();
        let report = analyzer.analyze(&doc);
        assert!(report.is_ok());
        let report = report.unwrap_or_default();
        assert!(
            !report.type_instabilities.is_empty(),
            "should detect type instability"
        );
    }

    #[test]
    fn analyzer_empty_document() {
        let doc = make_document("{}");
        let analyzer = AnomalyAnalyzer::default();
        let report = analyzer.analyze(&doc);
        assert!(report.is_ok());
        let report = report.unwrap_or_default();
        assert!(report.numeric_outliers.is_empty());
        assert!(report.rare_values.is_empty());
        assert!(report.type_instabilities.is_empty());
    }

    #[test]
    fn analyzer_numeric_outlier_in_array() {
        // Array with normal values and one outlier
        let json = r#"{"values": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 1000]}"#;
        let doc = make_document(json);
        let analyzer = AnomalyAnalyzer::default();
        let report = analyzer.analyze(&doc);
        assert!(report.is_ok());
        let report = report.unwrap_or_default();
        let has_outlier = report
            .numeric_outliers
            .iter()
            .any(|o| (o.value - 1000.0).abs() < 0.1);
        assert!(has_outlier, "should detect 1000 as numeric outlier");
    }
}
