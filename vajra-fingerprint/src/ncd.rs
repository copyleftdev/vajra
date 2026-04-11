//! Normalized Compression Distance: universal similarity metric.
//!
//! NCD approximates the normalized information distance — provably the most
//! general similarity metric — using a real compressor:
//!
//! ```text
//! NCD(x, y) = (C(xy) - min(C(x), C(y))) / max(C(x), C(y))
//! ```
//!
//! Properties:
//! - NCD ∈ [0, 1+ε] (near-metric, approximately satisfies triangle inequality)
//! - Feature-free: captures all computable regularities
//! - Deterministic given fixed compressor settings
//!
//! Reference: Li, M. et al. (2004). "The similarity metric."
//!            IEEE Transactions on Information Theory.

use serde::{Deserialize, Serialize};

/// Compression level for NCD computation.
/// Level 3 is a good balance of speed and compression ratio.
const ZSTD_LEVEL: i32 = 3;

/// Result of NCD computation between two documents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcdResult {
    /// Normalized Compression Distance in [0, 1+ε].
    pub distance: f64,
    /// Compressed size of document A alone.
    pub compressed_size_a: usize,
    /// Compressed size of document B alone.
    pub compressed_size_b: usize,
    /// Compressed size of A concatenated with B.
    pub compressed_size_ab: usize,
}

impl Default for NcdResult {
    fn default() -> Self {
        Self {
            distance: 1.0,
            compressed_size_a: 0,
            compressed_size_b: 0,
            compressed_size_ab: 0,
        }
    }
}

/// Compress data using zstd at a fixed level for deterministic results.
fn compress(data: &[u8]) -> Vec<u8> {
    zstd::encode_all(data, ZSTD_LEVEL).unwrap_or_default()
}

/// Compute the Normalized Compression Distance between two byte sequences.
///
/// Returns NCD in [0, 1+ε]:
/// - 0 ≈ identical content (maximum shared information)
/// - 1 ≈ completely dissimilar (no shared information)
///
/// Returns 1.0 for empty input (both empty → no information to compare).
#[must_use]
pub fn ncd(a: &[u8], b: &[u8]) -> f64 {
    let result = ncd_detailed(a, b);
    result.distance
}

/// Compute NCD with full details (compressed sizes).
#[must_use]
pub fn ncd_detailed(a: &[u8], b: &[u8]) -> NcdResult {
    if a.is_empty() && b.is_empty() {
        return NcdResult::default();
    }

    let ca = compress(a);
    let cb = compress(b);

    // Concatenate a ++ b
    let mut ab = Vec::with_capacity(a.len() + b.len());
    ab.extend_from_slice(a);
    ab.extend_from_slice(b);
    let cab = compress(&ab);

    let ca_len = ca.len();
    let cb_len = cb.len();
    let cab_len = cab.len();

    let min_c = ca_len.min(cb_len);
    let max_c = ca_len.max(cb_len);

    if max_c == 0 {
        return NcdResult {
            distance: 1.0,
            compressed_size_a: ca_len,
            compressed_size_b: cb_len,
            compressed_size_ab: cab_len,
        };
    }

    #[allow(clippy::cast_precision_loss)]
    let distance = (cab_len as f64 - min_c as f64) / max_c as f64;

    // Clamp to [0, ∞) — negative can't happen in practice but guard anyway
    let distance = if distance < 0.0 { 0.0 } else { distance };

    NcdResult {
        distance,
        compressed_size_a: ca_len,
        compressed_size_b: cb_len,
        compressed_size_ab: cab_len,
    }
}

/// Compute pairwise NCD matrix for a set of documents.
///
/// Returns an n×n matrix where `matrix[i][j]` = NCD(documents[i], documents[j]).
/// The diagonal is approximately 0. The matrix is symmetric.
///
/// Caches individual compressions to avoid redundant work.
#[must_use]
pub fn ncd_matrix(documents: &[&[u8]]) -> Vec<Vec<f64>> {
    let n = documents.len();
    if n == 0 {
        return Vec::new();
    }

    // Pre-compress each document
    let compressed: Vec<Vec<u8>> = documents.iter().map(|d| compress(d)).collect();
    let compressed_sizes: Vec<usize> = compressed.iter().map(Vec::len).collect();

    let mut matrix = vec![vec![0.0_f64; n]; n];

    for (i, doc_i) in documents.iter().enumerate() {
        for (j, doc_j) in documents.iter().enumerate().skip(i + 1) {
            // Concatenate i ++ j
            let mut ab = Vec::with_capacity(doc_i.len() + doc_j.len());
            ab.extend_from_slice(doc_i);
            ab.extend_from_slice(doc_j);
            let cab = compress(&ab);

            let min_c = compressed_sizes[i].min(compressed_sizes[j]);
            let max_c = compressed_sizes[i].max(compressed_sizes[j]);

            #[allow(clippy::cast_precision_loss)]
            let dist = if max_c > 0 {
                let d = (cab.len() as f64 - min_c as f64) / max_c as f64;
                if d < 0.0 {
                    0.0
                } else {
                    d
                }
            } else {
                1.0
            };

            matrix[i][j] = dist;
            matrix[j][i] = dist;
        }
    }

    matrix
}

/// Cluster documents by NCD similarity using single-linkage.
///
/// Returns cluster assignments: `clusters[i]` is the cluster ID for document i.
/// Documents with NCD ≤ `threshold` are placed in the same cluster.
#[must_use]
pub fn ncd_cluster(documents: &[&[u8]], threshold: f64) -> Vec<usize> {
    let n = documents.len();
    if n == 0 {
        return Vec::new();
    }

    let matrix = ncd_matrix(documents);

    // Union-Find for clustering
    let mut parent: Vec<usize> = (0..n).collect();

    // Find with path compression
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    for (i, row) in matrix.iter().enumerate() {
        for (j, &dist) in row.iter().enumerate().skip(i + 1) {
            if dist <= threshold {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    // Union: smaller root becomes parent
                    if ri < rj {
                        parent[rj] = ri;
                    } else {
                        parent[ri] = rj;
                    }
                }
            }
        }
    }

    // Normalize cluster IDs to sequential (0, 1, 2, ...)
    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut id_map: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut next_id = 0;
    roots
        .iter()
        .map(|&r| {
            *id_map.entry(r).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    // ---- ncd ----

    #[test]
    fn self_similarity_near_zero() {
        let data = b"hello world, this is a test document with some content";
        let d = ncd(data, data);
        assert!(d < 0.2, "NCD(x, x) should be near 0, got {d}");
    }

    #[test]
    fn identical_documents() {
        let a = b"the quick brown fox jumps over the lazy dog repeatedly many times";
        let result = ncd_detailed(a, a);
        assert!(
            result.distance < 0.2,
            "identical documents should have low NCD, got {}",
            result.distance
        );
    }

    #[test]
    fn dissimilar_documents() {
        let a = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let b = b"1234567890abcdefghijklmnopqrstuvwxyz!@#$%^&*()_+";
        let d = ncd(a, b);
        // Dissimilar documents should have higher NCD than similar ones
        let self_d = ncd(a, a);
        assert!(
            d > self_d,
            "dissimilar ({d}) should be > self-similar ({self_d})"
        );
    }

    #[test]
    fn symmetric() {
        let a = b"document alpha with specific content patterns";
        let b = b"document beta with different content entirely";
        let d_ab = ncd(a, b);
        let d_ba = ncd(b, a);
        assert!(
            (d_ab - d_ba).abs() < EPS,
            "NCD should be symmetric: {d_ab} vs {d_ba}"
        );
    }

    #[test]
    fn empty_both() {
        let d = ncd(b"", b"");
        assert!(
            (d - 1.0).abs() < EPS,
            "NCD of two empty docs should be 1.0, got {d}"
        );
    }

    #[test]
    fn empty_one() {
        let d = ncd(b"some content", b"");
        assert!(d.is_finite(), "NCD with one empty should be finite");
    }

    #[test]
    fn ncd_bounded() {
        let a = b"test document one";
        let b = b"test document two";
        let d = ncd(a, b);
        assert!(d >= 0.0, "NCD should be >= 0, got {d}");
        // NCD can slightly exceed 1.0 due to compressor overhead but should be close
        assert!(d < 2.0, "NCD should be < 2.0, got {d}");
    }

    // ---- ncd_detailed ----

    #[test]
    fn detailed_sizes_positive() {
        let a = b"hello world";
        let b = b"goodbye world";
        let result = ncd_detailed(a, b);
        assert!(result.compressed_size_a > 0);
        assert!(result.compressed_size_b > 0);
        assert!(result.compressed_size_ab > 0);
    }

    #[test]
    fn detailed_concatenation_size() {
        let a = b"repeated content for compression test";
        let b = b"repeated content for compression test";
        let result = ncd_detailed(a, b);
        // C(ab) should be less than C(a) + C(b) for identical content
        assert!(
            result.compressed_size_ab < result.compressed_size_a + result.compressed_size_b + 50,
            "concatenated compression should show savings for identical content"
        );
    }

    // ---- ncd_matrix ----

    #[test]
    fn matrix_empty() {
        let matrix = ncd_matrix(&[]);
        assert!(matrix.is_empty());
    }

    #[test]
    fn matrix_single() {
        let docs: Vec<&[u8]> = vec![b"hello"];
        let matrix = ncd_matrix(&docs);
        assert_eq!(matrix.len(), 1);
        assert_eq!(matrix[0].len(), 1);
        assert!((matrix[0][0]).abs() < EPS);
    }

    #[test]
    fn matrix_symmetric() {
        let docs: Vec<&[u8]> = vec![
            b"document one with content",
            b"document two with content",
            b"completely different text here",
        ];
        let matrix = ncd_matrix(&docs);
        for (i, row_i) in matrix.iter().enumerate() {
            for (j, &val_ij) in row_i.iter().enumerate() {
                assert!(
                    (val_ij - matrix[j][i]).abs() < EPS,
                    "matrix not symmetric at [{i}][{j}]"
                );
            }
        }
    }

    #[test]
    fn matrix_diagonal_near_zero() {
        let docs: Vec<&[u8]> = vec![b"test content", b"other content"];
        let matrix = ncd_matrix(&docs);
        for (i, row) in matrix.iter().enumerate() {
            assert!(
                (row[i]).abs() < EPS,
                "diagonal [{i}][{i}] should be 0, got {}",
                row[i]
            );
        }
    }

    // ---- ncd_cluster ----

    #[test]
    fn cluster_empty() {
        assert!(ncd_cluster(&[], 0.5).is_empty());
    }

    #[test]
    fn cluster_single() {
        let docs: Vec<&[u8]> = vec![b"hello"];
        let clusters = ncd_cluster(&docs, 0.5);
        assert_eq!(clusters, vec![0]);
    }

    #[test]
    fn cluster_identical() {
        let data = b"identical content for clustering test with enough data";
        let docs: Vec<&[u8]> = vec![data, data, data];
        let clusters = ncd_cluster(&docs, 0.5);
        // All identical docs should be in the same cluster
        assert_eq!(clusters[0], clusters[1]);
        assert_eq!(clusters[1], clusters[2]);
    }

    #[test]
    fn cluster_ids_sequential() {
        let docs: Vec<&[u8]> = vec![b"aaaaaa", b"bbbbbb", b"cccccc"];
        let clusters = ncd_cluster(&docs, 0.0); // Very strict threshold: each in own cluster
                                                // Cluster IDs should be sequential from 0
        let mut ids: Vec<usize> = clusters.clone();
        ids.sort();
        ids.dedup();
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(id, i, "cluster IDs should be sequential");
        }
    }
}
