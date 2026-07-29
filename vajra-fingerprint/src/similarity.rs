//! Batch similarity computation and structural clustering for documents.
//!
//! Provides exact Jaccard similarity, approximate similarity via MinHash
//! signatures, locality-sensitive hashing (LSH) for candidate pair
//! discovery, and a high-level clustering function that selects the
//! appropriate algorithm based on batch size.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use rayon::prelude::*;
use vajra_types::path::WildcardPath;
use vajra_types::Document;

// ---------------------------------------------------------------------------
// Jaccard similarity (exact)
// ---------------------------------------------------------------------------

/// Exact Jaccard similarity between two path sets.
///
/// J(A, B) = |A ∩ B| / |A ∪ B|. Returns 1.0 for two empty sets.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn jaccard_similarity(a: &[WildcardPath], b: &[WildcardPath]) -> f64 {
    let set_a: BTreeSet<&WildcardPath> = a.iter().collect();
    let set_b: BTreeSet<&WildcardPath> = b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        return 1.0;
    }

    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// MinHash signatures
// ---------------------------------------------------------------------------

/// A MinHash signature: a vector of `k` minimum hash values, one per
/// hash function.
#[derive(Debug, Clone)]
pub struct MinHashSignature {
    hashes: Vec<u64>,
}

/// Compute a MinHash signature for a set of paths.
///
/// Uses FNV-1a with `k` different seeds (`seed + i` for `i` in `0..k`).
#[must_use]
pub fn minhash_signature(paths: &[WildcardPath], k: usize, seed: u64) -> MinHashSignature {
    let mut hashes = vec![u64::MAX; k];

    for path in paths {
        let path_bytes = path.as_str().into_bytes();
        for (i, hash_slot) in hashes.iter_mut().enumerate().take(k) {
            let h = fnv1a_hash(&path_bytes, seed.wrapping_add(i as u64));
            if h < *hash_slot {
                *hash_slot = h;
            }
        }
    }

    MinHashSignature { hashes }
}

/// Estimate Jaccard similarity from two MinHash signatures.
///
/// Returns the fraction of positions where the hash values agree.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn minhash_similarity(a: &MinHashSignature, b: &MinHashSignature) -> f64 {
    if a.hashes.len() != b.hashes.len() || a.hashes.is_empty() {
        return 0.0;
    }
    let agree = a
        .hashes
        .iter()
        .zip(b.hashes.iter())
        .filter(|(ha, hb)| ha == hb)
        .count();

    agree as f64 / a.hashes.len() as f64
}

/// FNV-1a hash of bytes with a given seed.
fn fnv1a_hash(data: &[u8], seed: u64) -> u64 {
    // FNV offset basis XORed with seed for variety.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// LSH (Locality-Sensitive Hashing)
// ---------------------------------------------------------------------------

/// An LSH index that groups MinHash signatures into bands for fast
/// candidate-pair discovery.
#[derive(Debug)]
pub struct LshIndex {
    bands: usize,
    rows_per_band: usize,
    /// `buckets[band_idx]` maps a band hash to the list of document indices
    /// that hashed to that bucket.
    buckets: Vec<BTreeMap<u64, Vec<usize>>>,
}

impl LshIndex {
    /// Create a new LSH index with `bands` bands of `rows_per_band` rows.
    ///
    /// The total signature length must equal `bands * rows_per_band`.
    #[must_use]
    pub fn new(bands: usize, rows_per_band: usize) -> Self {
        Self {
            bands,
            rows_per_band,
            buckets: vec![BTreeMap::new(); bands],
        }
    }

    /// Insert a document's MinHash signature into the index.
    pub fn insert(&mut self, doc_index: usize, signature: &MinHashSignature) {
        for band in 0..self.bands {
            let start = band * self.rows_per_band;
            let end = start + self.rows_per_band;
            if end > signature.hashes.len() {
                break;
            }
            let band_hash = hash_band(&signature.hashes[start..end]);
            self.buckets[band]
                .entry(band_hash)
                .or_default()
                .push(doc_index);
        }
    }

    /// Find candidate pairs: documents that hash to the same bucket in
    /// any band.
    #[must_use]
    pub fn candidate_pairs(&self) -> Vec<(usize, usize)> {
        let mut seen = BTreeSet::new();
        let mut pairs = Vec::new();

        for band_buckets in &self.buckets {
            for docs in band_buckets.values() {
                if docs.len() < 2 {
                    continue;
                }
                for i in 0..docs.len() {
                    for j in (i + 1)..docs.len() {
                        let a = docs[i].min(docs[j]);
                        let b = docs[i].max(docs[j]);
                        if seen.insert((a, b)) {
                            pairs.push((a, b));
                        }
                    }
                }
            }
        }

        pairs
    }
}

/// Hash a band (slice of hash values) into a single u64 for bucketing.
fn hash_band(band: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &val in band {
        h ^= val;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Union-Find
// ---------------------------------------------------------------------------

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }

    fn clusters(&mut self) -> Vec<Vec<usize>> {
        let n = self.parent.len();
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            let root = self.find(i);
            groups.entry(root).or_default().push(i);
        }
        groups.into_values().collect()
    }
}

// ---------------------------------------------------------------------------
// Batch clustering
// ---------------------------------------------------------------------------

/// Result of document clustering.
#[derive(Debug, Clone)]
pub struct ClusterResult {
    /// Each cluster is a list of document indices from the input slice.
    pub clusters: Vec<Vec<usize>>,
    /// The similarity threshold used for grouping.
    pub similarity_threshold: f64,
}

/// Default number of MinHash permutations.
const DEFAULT_K: usize = 128;
/// Default similarity threshold for clustering.
const DEFAULT_THRESHOLD: f64 = 0.5;
/// Batch-size threshold below which we use exact pairwise Jaccard.
const SMALL_BATCH_LIMIT: usize = 1000;

/// Cluster documents by structural similarity at the default threshold (0.5).
///
/// See [`cluster_documents_with_threshold`] to choose the threshold.
#[must_use]
pub fn cluster_documents(docs: &[&Document], seed: u64) -> ClusterResult {
    cluster_documents_with_threshold(docs, seed, DEFAULT_THRESHOLD)
}

/// Cluster documents by structural similarity.
///
/// - For small batches (< 1000): exact pairwise Jaccard.
/// - For large batches: MinHash + LSH with verification.
///
/// Documents with Jaccard similarity strictly greater than `threshold` are
/// grouped together via union-find. `threshold` is clamped to `[0.0, 1.0]`;
/// the value actually applied is reported in
/// [`ClusterResult::similarity_threshold`].
#[must_use]
pub fn cluster_documents_with_threshold(
    docs: &[&Document],
    seed: u64,
    threshold: f64,
) -> ClusterResult {
    let threshold = if threshold.is_nan() {
        DEFAULT_THRESHOLD
    } else {
        threshold.clamp(0.0, 1.0)
    };

    let n = docs.len();
    if n == 0 {
        return ClusterResult {
            clusters: Vec::new(),
            similarity_threshold: threshold,
        };
    }

    // Extract path sets for each document.
    let path_sets: Vec<Vec<WildcardPath>> = docs.iter().map(|d| d.trie().all_paths()).collect();

    let mut uf = UnionFind::new(n);

    if n < SMALL_BATCH_LIMIT {
        // Exact pairwise Jaccard.
        for i in 0..n {
            for j in (i + 1)..n {
                let sim = jaccard_similarity(&path_sets[i], &path_sets[j]);
                if sim > threshold {
                    uf.union(i, j);
                }
            }
        }
    } else {
        // MinHash + LSH.
        let sigs: Vec<MinHashSignature> = path_sets
            .par_iter()
            .map(|ps| minhash_signature(ps, DEFAULT_K, seed))
            .collect();

        // LSH: 16 bands × 8 rows = 128 hashes.
        let bands = 16;
        let rows_per_band = DEFAULT_K / bands;
        let mut lsh = LshIndex::new(bands, rows_per_band);
        for (i, sig) in sigs.iter().enumerate() {
            lsh.insert(i, sig);
        }

        let candidates = lsh.candidate_pairs();

        // Verify candidates with exact Jaccard.
        for (i, j) in candidates {
            let sim = jaccard_similarity(&path_sets[i], &path_sets[j]);
            if sim > threshold {
                uf.union(i, j);
            }
        }
    }

    ClusterResult {
        clusters: uf.clusters(),
        similarity_threshold: threshold,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn parse_doc(json: &str) -> Result<Document, Box<dyn std::error::Error>> {
        Ok(vajra_core::parse_str(json)?)
    }

    // ---- Jaccard similarity ----

    #[test]
    fn jaccard_identical_sets() {
        let a = vec![
            WildcardPath::root().push_key("a"),
            WildcardPath::root().push_key("b"),
        ];
        let sim = jaccard_similarity(&a, &a);
        assert!((sim - 1.0).abs() < EPS, "expected 1.0, got {sim}");
    }

    #[test]
    fn jaccard_disjoint_sets() {
        let a = vec![WildcardPath::root().push_key("a")];
        let b = vec![WildcardPath::root().push_key("b")];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.0).abs() < EPS, "expected 0.0, got {sim}");
    }

    #[test]
    fn jaccard_known_overlap() {
        // A = {a, b, c}, B = {b, c, d} → intersection = {b,c} = 2, union = {a,b,c,d} = 4
        let a = vec![
            WildcardPath::root().push_key("a"),
            WildcardPath::root().push_key("b"),
            WildcardPath::root().push_key("c"),
        ];
        let b = vec![
            WildcardPath::root().push_key("b"),
            WildcardPath::root().push_key("c"),
            WildcardPath::root().push_key("d"),
        ];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.5).abs() < EPS, "expected 0.5, got {sim}");
    }

    #[test]
    fn jaccard_empty_sets() {
        let a: Vec<WildcardPath> = vec![];
        let sim = jaccard_similarity(&a, &a);
        assert!(
            (sim - 1.0).abs() < EPS,
            "expected 1.0 for empty sets, got {sim}"
        );
    }

    #[test]
    fn jaccard_one_empty() {
        let a = vec![WildcardPath::root().push_key("a")];
        let b: Vec<WildcardPath> = vec![];
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.0).abs() < EPS, "expected 0.0, got {sim}");
    }

    // ---- MinHash ----

    #[test]
    fn minhash_identical_sets_perfect() {
        let paths = vec![
            WildcardPath::root().push_key("a"),
            WildcardPath::root().push_key("b"),
            WildcardPath::root().push_key("c"),
        ];
        let sig_a = minhash_signature(&paths, 128, 42);
        let sig_b = minhash_signature(&paths, 128, 42);
        let sim = minhash_similarity(&sig_a, &sig_b);
        assert!(
            (sim - 1.0).abs() < EPS,
            "identical sets should give similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn minhash_disjoint_sets_low() {
        let a: Vec<WildcardPath> = (0..50)
            .map(|i| WildcardPath::root().push_key(&format!("a{i}")))
            .collect();
        let b: Vec<WildcardPath> = (0..50)
            .map(|i| WildcardPath::root().push_key(&format!("b{i}")))
            .collect();
        let sig_a = minhash_signature(&a, 256, 42);
        let sig_b = minhash_signature(&b, 256, 42);
        let sim = minhash_similarity(&sig_a, &sig_b);
        assert!(
            sim < 0.1,
            "disjoint sets should have very low similarity, got {sim}"
        );
    }

    #[test]
    fn minhash_converges_to_true_jaccard() {
        // Create 10 random-ish set pairs and check MinHash estimate is close.
        let k = 512;
        let seed = 123;
        let mut max_error = 0.0_f64;

        for trial in 0..10 {
            // Create sets with known overlap.
            let shared: Vec<WildcardPath> = (0..20)
                .map(|i| WildcardPath::root().push_key(&format!("s{i}")))
                .collect();
            let only_a: Vec<WildcardPath> = (0..(trial * 3 + 5))
                .map(|i| WildcardPath::root().push_key(&format!("a{trial}_{i}")))
                .collect();
            let only_b: Vec<WildcardPath> = (0..(trial * 2 + 8))
                .map(|i| WildcardPath::root().push_key(&format!("b{trial}_{i}")))
                .collect();

            let mut set_a = shared.clone();
            set_a.extend(only_a);
            let mut set_b = shared.clone();
            set_b.extend(only_b);

            let exact = jaccard_similarity(&set_a, &set_b);
            let sig_a = minhash_signature(&set_a, k, seed);
            let sig_b = minhash_signature(&set_b, k, seed);
            let approx = minhash_similarity(&sig_a, &sig_b);

            let error = (exact - approx).abs();
            if error > max_error {
                max_error = error;
            }
        }

        // With k=512, we expect the maximum error across 10 trials to be < 0.15.
        assert!(max_error < 0.15, "MinHash max error too large: {max_error}");
    }

    // ---- LSH ----

    #[test]
    fn lsh_similar_docs_are_candidates() {
        // Two very similar signatures should hash to the same bucket.
        let paths = vec![
            WildcardPath::root().push_key("a"),
            WildcardPath::root().push_key("b"),
            WildcardPath::root().push_key("c"),
        ];
        let sig = minhash_signature(&paths, 128, 42);

        let mut lsh = LshIndex::new(16, 8);
        lsh.insert(0, &sig);
        lsh.insert(1, &sig); // identical signature

        let pairs = lsh.candidate_pairs();
        assert!(
            pairs.contains(&(0, 1)),
            "identical signatures should be candidates"
        );
    }

    #[test]
    fn lsh_dissimilar_docs_not_candidates() {
        let a: Vec<WildcardPath> = (0..50)
            .map(|i| WildcardPath::root().push_key(&format!("x{i}")))
            .collect();
        let b: Vec<WildcardPath> = (0..50)
            .map(|i| WildcardPath::root().push_key(&format!("y{i}")))
            .collect();
        let sig_a = minhash_signature(&a, 128, 42);
        let sig_b = minhash_signature(&b, 128, 42);

        let mut lsh = LshIndex::new(16, 8);
        lsh.insert(0, &sig_a);
        lsh.insert(1, &sig_b);

        let pairs = lsh.candidate_pairs();
        // Completely disjoint large sets should not become candidates.
        assert!(
            !pairs.contains(&(0, 1)),
            "disjoint sets should not be candidates"
        );
    }

    // ---- Clustering ----

    #[test]
    fn cluster_identical_documents() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1, "b": 2}"#)?;
        let docs: Vec<&Document> = vec![&doc, &doc, &doc];
        let result = cluster_documents(&docs, 42);
        assert_eq!(
            result.clusters.len(),
            1,
            "identical documents should form one cluster"
        );
        assert_eq!(result.clusters[0].len(), 3);
        Ok(())
    }

    #[test]
    fn cluster_different_documents() -> Result<(), Box<dyn std::error::Error>> {
        let doc_a = parse_doc(r#"{"a": 1}"#)?;
        let doc_b = parse_doc(r#"{"z": 99, "w": 88, "v": 77}"#)?;
        let docs: Vec<&Document> = vec![&doc_a, &doc_b];
        let result = cluster_documents(&docs, 42);
        assert_eq!(
            result.clusters.len(),
            2,
            "completely different documents should be in separate clusters, got {} clusters",
            result.clusters.len()
        );
        Ok(())
    }

    #[test]
    fn cluster_empty_input() {
        let docs: Vec<&Document> = vec![];
        let result = cluster_documents(&docs, 42);
        assert!(result.clusters.is_empty());
    }

    #[test]
    fn cluster_single_document() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1}"#)?;
        let docs: Vec<&Document> = vec![&doc];
        let result = cluster_documents(&docs, 42);
        assert_eq!(result.clusters.len(), 1);
        assert_eq!(result.clusters[0], vec![0]);
        Ok(())
    }

    #[test]
    fn cluster_mixed_similarity() -> Result<(), Box<dyn std::error::Error>> {
        // Two similar docs and one different.
        let doc_a = parse_doc(r#"{"name": "Alice", "age": 30, "city": "NY"}"#)?;
        let doc_b = parse_doc(r#"{"name": "Bob", "age": 25, "city": "LA"}"#)?;
        let doc_c = parse_doc(r#"{"totally": "different", "schema": true}"#)?;
        let docs: Vec<&Document> = vec![&doc_a, &doc_b, &doc_c];
        let result = cluster_documents(&docs, 42);

        // doc_a and doc_b share {$, $.name, $.age, $.city} → Jaccard=1.0
        // doc_c shares only {$} → low Jaccard
        // So we expect 2 clusters: {0,1} and {2}
        assert_eq!(
            result.clusters.len(),
            2,
            "expected 2 clusters, got {}",
            result.clusters.len()
        );
        Ok(())
    }

    #[test]
    fn cluster_threshold_is_reported() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1}"#)?;
        let docs: Vec<&Document> = vec![&doc];
        let result = cluster_documents(&docs, 42);
        assert!(
            (result.similarity_threshold - 0.5).abs() < EPS,
            "expected threshold 0.5, got {}",
            result.similarity_threshold
        );
        Ok(())
    }

    /// Two documents sharing 3 of 4 paths (Jaccard = 0.6) must group below
    /// that similarity and split above it.
    #[test]
    fn cluster_threshold_controls_grouping() -> Result<(), Box<dyn std::error::Error>> {
        let doc_a = parse_doc(r#"{"x": 1, "y": 2, "z": 3}"#)?;
        let doc_b = parse_doc(r#"{"x": 1, "y": 2, "w": 4}"#)?;
        let docs: Vec<&Document> = vec![&doc_a, &doc_b];

        let loose = cluster_documents_with_threshold(&docs, 42, 0.5);
        assert_eq!(loose.clusters.len(), 1, "0.5 should group them");

        let strict = cluster_documents_with_threshold(&docs, 42, 0.9);
        assert_eq!(strict.clusters.len(), 2, "0.9 should separate them");
        Ok(())
    }

    #[test]
    fn cluster_threshold_is_clamped_and_reported() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_doc(r#"{"a": 1}"#)?;
        let docs: Vec<&Document> = vec![&doc];

        let high = cluster_documents_with_threshold(&docs, 42, 7.5);
        assert!((high.similarity_threshold - 1.0).abs() < EPS);

        let low = cluster_documents_with_threshold(&docs, 42, -3.0);
        assert!((low.similarity_threshold - 0.0).abs() < EPS);

        let nan = cluster_documents_with_threshold(&docs, 42, f64::NAN);
        assert!(
            (nan.similarity_threshold - 0.5).abs() < EPS,
            "NaN falls back"
        );
        Ok(())
    }

    /// The empty-input early return must report the requested threshold, not
    /// the default.
    #[test]
    fn cluster_empty_input_reports_requested_threshold() {
        let docs: Vec<&Document> = vec![];
        let result = cluster_documents_with_threshold(&docs, 42, 0.85);
        assert!(result.clusters.is_empty());
        assert!((result.similarity_threshold - 0.85).abs() < EPS);
    }

    /// `cluster_documents` must stay identical to the threshold-aware call at
    /// the default threshold.
    #[test]
    fn cluster_default_matches_explicit() -> Result<(), Box<dyn std::error::Error>> {
        let doc_a = parse_doc(r#"{"name": "Alice", "age": 30}"#)?;
        let doc_b = parse_doc(r#"{"name": "Bob", "age": 25}"#)?;
        let doc_c = parse_doc(r#"{"totally": "different"}"#)?;
        let docs: Vec<&Document> = vec![&doc_a, &doc_b, &doc_c];

        let implicit = cluster_documents(&docs, 42);
        let explicit = cluster_documents_with_threshold(&docs, 42, 0.5);
        assert_eq!(implicit.clusters, explicit.clusters);
        assert!(
            (implicit.similarity_threshold - explicit.similarity_threshold).abs() < EPS,
            "thresholds must agree"
        );
        Ok(())
    }
}
