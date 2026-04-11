//! Full statistics analyzer implementing the [`Analyzer`] and
//! [`FeatureExtractor`] traits from `vajra-types`.
//!
//! Walks a document, computes per-path entropy, cardinality, numeric
//! statistics, and populates a [`FeatureStore`].

use std::collections::BTreeMap;

use vajra_types::document::Document;
use vajra_types::error::VajraError;
use vajra_types::features::{FeatureStore, NumericFeatures, PathFeatures};
use vajra_types::path::WildcardPath;
use vajra_types::traits::{Analyzer, FeatureExtractor};

use crate::entropy;
use crate::frequency::FrequencyCounter;
use crate::lz_complexity;
use crate::numeric::{self, NumericStats};
use crate::renyi::{self, RenyiSpectrum};

/// Per-path statistics produced by the analyzer.
#[derive(Debug, Clone)]
pub struct PathStats {
    /// Shannon entropy of value distribution.
    pub entropy: f64,
    /// Normalized entropy (0 = constant, 1 = uniform).
    pub normalized_entropy: f64,
    /// Number of distinct values observed.
    pub cardinality: u64,
    /// Total observations at this path.
    pub total_count: u64,
    /// Self-information of the rarest value: `-log2(min_p)`.
    pub max_rarity: f64,
    /// Numeric distribution statistics, if the path contains numeric values.
    pub numeric_stats: Option<NumericStats>,
    /// Rényi entropy spectrum (H₀, H₁, H₂, H∞).
    pub renyi_spectrum: RenyiSpectrum,
    /// Lempel-Ziv normalized complexity for string values.
    pub lz_complexity: Option<f64>,
    /// Top-k most frequent values and their counts.
    pub top_values: Vec<(String, u64)>,
}

/// Full analysis result: per-path statistics keyed by wildcard path.
#[derive(Debug, Clone)]
pub struct StatsResult {
    /// Per-path statistics, ordered deterministically.
    pub paths: BTreeMap<WildcardPath, PathStats>,
}

/// Top-k values to retain per path.
const DEFAULT_TOP_K: usize = 10;

/// Stateless statistics analyzer.
pub struct StatsAnalyzer;

impl Analyzer for StatsAnalyzer {
    type Output = StatsResult;

    fn analyze(&self, doc: &Document) -> Result<StatsResult, VajraError> {
        let mut fc = FrequencyCounter::new();
        fc.count_document(doc);

        let numeric_values = collect_numeric_values(doc);

        let mut paths = BTreeMap::new();

        for path in fc.paths() {
            let count_vec = fc.count_values(&path);
            let cardinality = fc.cardinality(&path);

            let h = entropy::shannon_entropy_from_counts(&count_vec);
            #[allow(clippy::cast_possible_truncation)]
            let nh = entropy::normalized_entropy(h, cardinality as usize);

            let total_count: u64 = count_vec.iter().sum();

            // Max rarity: -log2(min_count / total)
            #[allow(clippy::cast_precision_loss)]
            let max_rarity = if total_count > 0 {
                let min_count = count_vec
                    .iter()
                    .copied()
                    .filter(|&c| c > 0)
                    .min()
                    .unwrap_or(1);
                let min_p = min_count as f64 / total_count as f64;
                -min_p.log2()
            } else {
                0.0
            };

            let numeric_stats = numeric_values.get(&path).and_then(|vals| {
                let mut v = vals.clone();
                numeric::compute_numeric_stats(&mut v)
            });

            let top_values = fc.top_k(&path, DEFAULT_TOP_K);

            // Rényi entropy spectrum from the same counts
            let spectrum = renyi::renyi_spectrum(&count_vec);

            // LZ complexity for string-valued paths (skip numeric-only paths)
            let lz = if numeric_stats.is_none() && !top_values.is_empty() {
                let string_vals: Vec<&str> = top_values.iter().map(|(s, _)| s.as_str()).collect();
                let result = lz_complexity::lz_complexity_for_values(&string_vals);
                Some(result.normalized)
            } else {
                None
            };

            paths.insert(
                path,
                PathStats {
                    entropy: h,
                    normalized_entropy: nh,
                    cardinality,
                    total_count,
                    max_rarity,
                    numeric_stats,
                    renyi_spectrum: spectrum,
                    lz_complexity: lz,
                    top_values,
                },
            );
        }

        Ok(StatsResult { paths })
    }
}

impl FeatureExtractor for StatsAnalyzer {
    fn extract(&self, doc: &Document, features: &mut FeatureStore) -> Result<(), VajraError> {
        let result = self.analyze(doc)?;

        for (path, stats) in &result.paths {
            let pf: &mut PathFeatures = features.get_or_create(path);
            pf.entropy = Some(stats.entropy);
            pf.normalized_entropy = Some(stats.normalized_entropy);
            pf.cardinality = Some(stats.cardinality);
            pf.count = Some(stats.total_count);
            pf.max_rarity = Some(stats.max_rarity);

            // Rényi spectrum
            pf.renyi_hartley = Some(stats.renyi_spectrum.hartley);
            pf.renyi_collision = Some(stats.renyi_spectrum.collision);
            pf.renyi_min_entropy = Some(stats.renyi_spectrum.min_entropy);
            pf.renyi_divergence = Some(stats.renyi_spectrum.divergence);

            // LZ complexity
            pf.lz_complexity = stats.lz_complexity;

            // Pull null_rate and type_instability from the trie if available.
            if let Some(node) = doc.trie().get(path) {
                pf.null_rate = Some(node.metadata.null_rate());
                pf.type_instability = Some(node.metadata.type_instability());
            }

            if let Some(ref ns) = stats.numeric_stats {
                pf.numeric = Some(NumericFeatures {
                    min: ns.min,
                    max: ns.max,
                    mean: ns.mean,
                    median: ns.median,
                    mad: ns.mad,
                    p05: ns.p05,
                    p25: ns.p25,
                    p75: ns.p75,
                    p95: ns.p95,
                });
            }
        }

        Ok(())
    }
}

/// Walk the document's JSON tree and collect numeric values per wildcard path.
fn collect_numeric_values(doc: &Document) -> BTreeMap<WildcardPath, Vec<f64>> {
    let mut result: BTreeMap<WildcardPath, Vec<f64>> = BTreeMap::new();
    let root_path = WildcardPath::root();
    walk_numeric(doc.value(), &root_path, &mut result);
    result
}

fn walk_numeric(
    value: &serde_json::Value,
    path: &WildcardPath,
    out: &mut BTreeMap<WildcardPath, Vec<f64>>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let child_path = path.push_key(key);
                walk_numeric(child, &child_path, out);
            }
        }
        serde_json::Value::Array(arr) => {
            let child_path = path.push_array_wildcard();
            for child in arr {
                walk_numeric(child, &child_path, out);
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                out.entry(path.clone()).or_default().push(f);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn parse_doc(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
    }

    #[test]
    fn analyze_simple_object() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1, "b": 2}"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let a_path = WildcardPath::root().push_key("a");
        let a_stats = result.paths.get(&a_path).ok_or("expected stats for $.a")?;

        assert!((a_stats.entropy - 0.0).abs() < EPS);
        assert_eq!(a_stats.cardinality, 1);
        assert_eq!(a_stats.total_count, 1);
        Ok(())
    }

    #[test]
    fn analyze_array_with_repeats() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["x", "y", "x", "x", "y"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;

        assert_eq!(stats.cardinality, 2);
        assert_eq!(stats.total_count, 5);

        // Entropy for p=[3/5, 2/5]
        let expected_h =
            -(3.0 / 5.0 * (3.0_f64 / 5.0).log2()) - (2.0 / 5.0 * (2.0_f64 / 5.0).log2());
        assert!((stats.entropy - expected_h).abs() < 1e-8);
        Ok(())
    }

    #[test]
    fn analyze_numeric_stats_present() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"[1, 2, 3, 4, 5]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;

        assert!(stats.numeric_stats.is_some());
        let ns = stats.numeric_stats.as_ref().ok_or("expected numeric")?;
        assert!((ns.min - 1.0).abs() < EPS);
        assert!((ns.max - 5.0).abs() < EPS);
        assert!((ns.mean - 3.0).abs() < EPS);
        assert!((ns.median - 3.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn analyze_string_path_no_numeric() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["a", "b", "c"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;
        assert!(stats.numeric_stats.is_none());
        Ok(())
    }

    #[test]
    fn analyze_max_rarity() -> Result<(), Box<dyn std::error::Error>> {
        // Array: ["a", "a", "a", "rare"]
        // min_count = 1 ("rare"), total = 4, rarity = -log2(1/4) = 2.0
        let doc = parse_doc(r#"["a", "a", "a", "rare"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;
        assert!((stats.max_rarity - 2.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn feature_extractor_populates_store() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"scores": [10, 20, 30]}"#)?;
        let analyzer = StatsAnalyzer;
        let mut store = FeatureStore::new();
        analyzer.extract(&doc, &mut store)?;

        let path = WildcardPath::root()
            .push_key("scores")
            .push_array_wildcard();
        let features = store.get(&path).ok_or("expected features")?;

        assert!(features.entropy.is_some());
        assert!(features.cardinality.is_some());
        assert!(features.count.is_some());
        assert!(features.max_rarity.is_some());
        assert!(features.numeric.is_some());

        let nf = features.numeric.as_ref().ok_or("expected numeric")?;
        assert!((nf.min - 10.0).abs() < EPS);
        assert!((nf.max - 30.0).abs() < EPS);
        assert!((nf.mean - 20.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn feature_extractor_includes_trie_features() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"[1, null, 2]"#)?;
        let analyzer = StatsAnalyzer;
        let mut store = FeatureStore::new();
        analyzer.extract(&doc, &mut store)?;

        let path = WildcardPath::root().push_array_wildcard();
        let features = store.get(&path).ok_or("expected features")?;

        // The trie records null_rate for $[*] as 1/3
        assert!(features.null_rate.is_some());
        let nr = features.null_rate.unwrap_or(0.0);
        assert!((nr - 1.0 / 3.0).abs() < 1e-8);

        // type_instability should be > 0 because we have Integer and Null
        assert!(features.type_instability.is_some());
        let ti = features.type_instability.unwrap_or(0.0);
        assert!(ti > 0.0);
        Ok(())
    }

    #[test]
    fn empty_document() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc("{}")?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;
        assert!(result.paths.is_empty());
        Ok(())
    }

    #[test]
    fn analyze_top_values() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["a", "b", "a", "c", "a", "b"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;

        // top values ordered by count desc
        assert_eq!(stats.top_values[0], ("a".to_owned(), 3));
        assert_eq!(stats.top_values[1], ("b".to_owned(), 2));
        assert_eq!(stats.top_values[2], ("c".to_owned(), 1));
        Ok(())
    }

    #[test]
    fn constant_entropy_normalized_zero() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["x", "x", "x", "x"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;

        assert!((stats.entropy - 0.0).abs() < EPS);
        assert!((stats.normalized_entropy - 0.0).abs() < EPS);
        Ok(())
    }

    #[test]
    fn uniform_entropy_normalized_one() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"["a", "b", "c", "d"]"#)?;
        let analyzer = StatsAnalyzer;
        let result = analyzer.analyze(&doc)?;

        let path = WildcardPath::root().push_array_wildcard();
        let stats = result.paths.get(&path).ok_or("missing")?;

        // H = log2(4) = 2.0, normalized = 1.0
        assert!((stats.entropy - 2.0).abs() < EPS);
        assert!((stats.normalized_entropy - 1.0).abs() < EPS);
        Ok(())
    }
}
