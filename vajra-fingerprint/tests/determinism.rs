//! Determinism verification tests for NCD (Normalized Compression Distance).
//!
//! NCD must produce byte-identical output across N runs given the same
//! input and compressor settings.

use vajra_fingerprint::ncd::{ncd, ncd_cluster, ncd_detailed, ncd_matrix};

const N: usize = 10;

// -----------------------------------------------------------------------
// NCD DST
// -----------------------------------------------------------------------

#[test]
fn dst_ncd_10_runs() {
    let a = b"the quick brown fox jumps over the lazy dog and more text for compression";
    let b = b"the slow brown cat crawls under the sleepy dog and different text entirely";

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let d = ncd(a, b);
        results.push(format!("{d:.15}"));
    }
    for i in 1..N {
        assert_eq!(results[0], results[i], "NCD run 0 vs run {i} differ");
    }
}

#[test]
fn dst_ncd_detailed_10_runs() {
    let a = b"structured json content: {\"key\": \"value\", \"items\": [1, 2, 3]}";
    let b = b"different json content: {\"name\": \"other\", \"count\": 42}";

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = ncd_detailed(a, b);
        results.push(format!(
            "dist={:.15},ca={},cb={},cab={}",
            r.distance, r.compressed_size_a, r.compressed_size_b, r.compressed_size_ab
        ));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "NCD detailed run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_ncd_symmetric_bitwise() {
    let a = b"document alpha with content";
    let b = b"document beta with content";

    // NCD(a,b) must equal NCD(b,a) bitwise across all runs
    let mut ab_results: Vec<String> = Vec::with_capacity(N);
    let mut ba_results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        ab_results.push(format!("{:.15}", ncd(a, b)));
        ba_results.push(format!("{:.15}", ncd(b, a)));
    }
    for i in 0..N {
        assert_eq!(
            ab_results[i], ba_results[i],
            "NCD(a,b) != NCD(b,a) on run {i}"
        );
    }
}

#[test]
fn dst_ncd_matrix_10_runs() {
    let docs: Vec<&[u8]> = vec![
        b"document one with some content here",
        b"document two with different text",
        b"something completely unrelated to test",
    ];

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let matrix = ncd_matrix(&docs);
        let serialized: Vec<String> = matrix
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| format!("{v:.15}"))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        results.push(serialized.join(";"));
    }
    for i in 1..N {
        assert_eq!(results[0], results[i], "NCD matrix run 0 vs run {i} differ");
    }
}

#[test]
fn dst_ncd_cluster_10_runs() {
    let docs: Vec<&[u8]> = vec![
        b"similar content pattern A",
        b"similar content pattern B",
        b"completely different unrelated text for testing",
    ];

    let mut results: Vec<Vec<usize>> = Vec::with_capacity(N);
    for _ in 0..N {
        results.push(ncd_cluster(&docs, 0.5));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "NCD cluster run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_ncd_compressor_deterministic() {
    // Verify zstd at fixed level produces identical compressed sizes
    let data = b"test data for compressor determinism verification with enough length";

    let mut sizes: Vec<usize> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = ncd_detailed(data, data);
        sizes.push(r.compressed_size_a);
    }
    for i in 1..N {
        assert_eq!(
            sizes[0], sizes[i],
            "compressed size run 0 ({}) vs run {i} ({}) differ",
            sizes[0], sizes[i]
        );
    }
}
