//! Chaos and adversarial input tests for NCD.
//!
//! Feed pathological inputs and verify NO PANICS and graceful degradation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use vajra_fingerprint::ncd::{ncd, ncd_cluster, ncd_detailed, ncd_matrix};

// -----------------------------------------------------------------------
// NCD Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_ncd_empty_both() {
    let d = ncd(b"", b"");
    assert!(d.is_finite());
}

#[test]
fn chaos_ncd_empty_one() {
    let d = ncd(b"hello world test content", b"");
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_identical() {
    let data = b"identical content for testing";
    let d = ncd(data, data);
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_single_byte() {
    let d = ncd(b"a", b"b");
    assert!(d.is_finite());
}

#[test]
fn chaos_ncd_single_byte_identical() {
    let d = ncd(b"x", b"x");
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_very_long_document() {
    // 100KB document
    let a: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let b: Vec<u8> = (0..100_000).map(|i| ((i + 128) % 256) as u8).collect();
    let d = ncd(&a, &b);
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_incompressible_random() {
    // Pseudo-random data that compresses poorly
    let mut a = vec![0u8; 1000];
    let mut b = vec![0u8; 1000];
    for (i, byte) in a.iter_mut().enumerate() {
        *byte = ((i * 2654435761) & 0xFF) as u8;
    }
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = ((i * 1640531527) & 0xFF) as u8;
    }
    let d = ncd(&a, &b);
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_all_zeros() {
    let a = vec![0u8; 10_000];
    let b = vec![0u8; 10_000];
    let d = ncd(&a, &b);
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

#[test]
fn chaos_ncd_one_all_zeros_one_random() {
    let a = vec![0u8; 1000];
    let mut b = vec![0u8; 1000];
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = ((i * 2654435761) & 0xFF) as u8;
    }
    let d = ncd(&a, &b);
    assert!(d.is_finite());
}

#[test]
fn chaos_ncd_detailed_empty() {
    let r = ncd_detailed(b"", b"");
    assert!(r.distance.is_finite());
}

#[test]
fn chaos_ncd_asymmetric_lengths() {
    let a = b"short";
    let b: Vec<u8> = vec![b'x'; 50_000];
    let d = ncd(a, &b);
    assert!(d.is_finite());
    assert!(d >= 0.0);
}

// -----------------------------------------------------------------------
// NCD Matrix Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_ncd_matrix_empty() {
    let matrix = ncd_matrix(&[]);
    assert!(matrix.is_empty());
}

#[test]
fn chaos_ncd_matrix_single() {
    let docs: Vec<&[u8]> = vec![b"alone"];
    let matrix = ncd_matrix(&docs);
    assert_eq!(matrix.len(), 1);
    assert!(matrix[0][0].is_finite());
}

#[test]
fn chaos_ncd_matrix_with_empties() {
    let docs: Vec<&[u8]> = vec![b"", b"hello", b""];
    let matrix = ncd_matrix(&docs);
    for row in &matrix {
        for &val in row {
            assert!(val.is_finite(), "matrix contains non-finite value");
        }
    }
}

// -----------------------------------------------------------------------
// NCD Cluster Chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_ncd_cluster_empty() {
    assert!(ncd_cluster(&[], 0.5).is_empty());
}

#[test]
fn chaos_ncd_cluster_zero_threshold() {
    let docs: Vec<&[u8]> = vec![b"a", b"b", b"c"];
    let clusters = ncd_cluster(&docs, 0.0);
    assert_eq!(clusters.len(), 3);
}

#[test]
fn chaos_ncd_cluster_threshold_one() {
    let docs: Vec<&[u8]> = vec![
        b"content alpha for testing",
        b"content beta for testing",
        b"content gamma for testing",
    ];
    let clusters = ncd_cluster(&docs, 1.0);
    // Threshold 1.0 → everything should cluster together
    assert_eq!(clusters.len(), 3);
    assert_eq!(clusters[0], clusters[1]);
    assert_eq!(clusters[1], clusters[2]);
}

#[test]
fn chaos_ncd_cluster_negative_threshold() {
    let docs: Vec<&[u8]> = vec![b"a", b"b"];
    let clusters = ncd_cluster(&docs, -1.0);
    // Negative threshold → nothing clusters
    assert_eq!(clusters.len(), 2);
}
