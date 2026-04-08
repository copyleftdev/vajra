#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use vajra_drift::jsd::{jensen_shannon_divergence, jsd_metric};
use vajra_drift::wasserstein::wasserstein_1d;

/// Generate a valid probability distribution that sums to 1.0.
fn arb_probability_distribution(min_len: usize, max_len: usize) -> impl Strategy<Value = Vec<f64>> {
    prop::collection::vec(0.001f64..1.0, min_len..max_len).prop_map(|raw| {
        let sum: f64 = raw.iter().sum();
        raw.iter().map(|v| v / sum).collect()
    })
}

/// Generate a pair of distributions with the same length.
fn arb_distribution_pair(
    min_len: usize,
    max_len: usize,
) -> impl Strategy<Value = (Vec<f64>, Vec<f64>)> {
    (min_len..max_len).prop_flat_map(move |len| {
        let dist = prop::collection::vec(0.001f64..1.0, len..=len).prop_map(|raw| {
            let sum: f64 = raw.iter().sum();
            raw.iter().map(|v| v / sum).collect::<Vec<f64>>()
        });
        (dist.clone(), dist)
    })
}

// =========================================================================
// JSD properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 1. JSD symmetry: JSD(P, Q) == JSD(Q, P)
    #[test]
    fn prop_jsd_symmetry((p, q) in arb_distribution_pair(2, 20)) {
        let jsd_pq = jensen_shannon_divergence(&p, &q).unwrap();
        let jsd_qp = jensen_shannon_divergence(&q, &p).unwrap();
        prop_assert!((jsd_pq - jsd_qp).abs() < 1e-12,
            "JSD(P,Q)={jsd_pq} != JSD(Q,P)={jsd_qp}");
    }

    // 2. JSD identity: JSD(P, P) == 0
    #[test]
    fn prop_jsd_identity(p in arb_probability_distribution(2, 20)) {
        let jsd = jensen_shannon_divergence(&p, &p).unwrap();
        prop_assert!((jsd - 0.0).abs() < 1e-12,
            "JSD(P,P) should be 0, got {jsd}");
    }

    // 3. JSD bounded: 0 <= JSD(P, Q) <= 1
    #[test]
    fn prop_jsd_bounded((p, q) in arb_distribution_pair(2, 20)) {
        let jsd = jensen_shannon_divergence(&p, &q).unwrap();
        prop_assert!(jsd >= -1e-12,
            "JSD should be >= 0, got {jsd}");
        prop_assert!(jsd <= 1.0 + 1e-12,
            "JSD should be <= 1.0, got {jsd}");
    }

    // 4. JSD non-negative: JSD >= 0 always
    #[test]
    fn prop_jsd_non_negative((p, q) in arb_distribution_pair(2, 20)) {
        let jsd = jensen_shannon_divergence(&p, &q).unwrap();
        prop_assert!(jsd >= -1e-12,
            "JSD should be non-negative, got {jsd}");
    }

}

// Triangle inequality needs three distributions of the same length.
// We handle this with a custom strategy.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_jsd_metric_triangle_inequality(
        len in 3usize..15,
        raw_p in prop::collection::vec(0.001f64..1.0, 3..15),
        raw_q in prop::collection::vec(0.001f64..1.0, 3..15),
        raw_r in prop::collection::vec(0.001f64..1.0, 3..15),
    ) {
        // Truncate all to the same length.
        let n = len.min(raw_p.len()).min(raw_q.len()).min(raw_r.len());
        if n < 2 {
            return Ok(());
        }

        let normalize = |raw: &[f64], n: usize| -> Vec<f64> {
            let slice = &raw[..n];
            let sum: f64 = slice.iter().sum();
            slice.iter().map(|v| v / sum).collect()
        };

        let p = normalize(&raw_p, n);
        let q = normalize(&raw_q, n);
        let r = normalize(&raw_r, n);

        let d_pr = jsd_metric(&p, &r).unwrap();
        let d_pq = jsd_metric(&p, &q).unwrap();
        let d_qr = jsd_metric(&q, &r).unwrap();

        prop_assert!(d_pr <= d_pq + d_qr + 1e-10,
            "triangle inequality violated: d(P,R)={d_pr} > d(P,Q)+d(Q,R)={}",
            d_pq + d_qr);
    }
}

// =========================================================================
// Wasserstein properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 6. Wasserstein identity: W(A, A) == 0
    #[test]
    fn prop_wasserstein_identity(
        values in prop::collection::vec(-1000.0f64..1000.0, 1..100)
    ) {
        let mut a = values.clone();
        let mut b = values;
        let d = wasserstein_1d(&mut a, &mut b);
        prop_assert!((d - 0.0).abs() < 1e-10,
            "W(A,A) should be 0, got {d}");
    }

    // 7. Wasserstein non-negative: W >= 0
    #[test]
    fn prop_wasserstein_non_negative(
        a in prop::collection::vec(-1000.0f64..1000.0, 1..50),
        b in prop::collection::vec(-1000.0f64..1000.0, 1..50),
    ) {
        let mut a = a;
        let mut b = b;
        let d = wasserstein_1d(&mut a, &mut b);
        prop_assert!(d >= -1e-12,
            "Wasserstein distance should be non-negative, got {d}");
    }

    // 8. Wasserstein shift: W([x+c for x in A], A) == |c| for uniform shift
    #[test]
    fn prop_wasserstein_shift(
        values in prop::collection::vec(-100.0f64..100.0, 1..50),
        shift in -100.0f64..100.0,
    ) {
        let mut a = values.clone();
        let mut b: Vec<f64> = values.iter().map(|v| v + shift).collect();
        let d = wasserstein_1d(&mut a, &mut b);
        prop_assert!((d - shift.abs()).abs() < 1e-8,
            "W(A+c, A) should be |c|={}, got {d}", shift.abs());
    }
}

// =========================================================================
// Extended CI tests
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    #[ignore]
    fn prop_jsd_symmetry_extended((p, q) in arb_distribution_pair(2, 30)) {
        let jsd_pq = jensen_shannon_divergence(&p, &q).unwrap();
        let jsd_qp = jensen_shannon_divergence(&q, &p).unwrap();
        prop_assert!((jsd_pq - jsd_qp).abs() < 1e-12);
    }

    #[test]
    #[ignore]
    fn prop_wasserstein_shift_extended(
        values in prop::collection::vec(-1000.0f64..1000.0, 1..100),
        shift in -1000.0f64..1000.0,
    ) {
        let mut a = values.clone();
        let mut b: Vec<f64> = values.iter().map(|v| v + shift).collect();
        let d = wasserstein_1d(&mut a, &mut b);
        prop_assert!((d - shift.abs()).abs() < 1e-6);
    }
}
