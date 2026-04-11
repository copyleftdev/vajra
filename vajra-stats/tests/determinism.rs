//! Determinism verification tests for information-theoretic primitives.
//!
//! Every algorithm must produce byte-identical output across N runs
//! on identical input. Tests: Rényi spectrum, LZ complexity,
//! transfer entropy, total correlation.

use std::collections::BTreeMap;

use vajra_stats::lz_complexity::{lz76_complexity, lz_complexity_for_values};
use vajra_stats::renyi::renyi_spectrum;
use vajra_stats::total_correlation::total_correlation;
use vajra_stats::transfer_entropy::{
    bidirectional_transfer_entropy, bin_strings, transfer_entropy,
};

const N: usize = 10;

// -----------------------------------------------------------------------
// Rényi Spectrum DST
// -----------------------------------------------------------------------

#[test]
fn dst_renyi_spectrum_10_runs() {
    let counts: Vec<u64> = vec![100, 50, 25, 10, 5, 3, 2, 1];
    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let s = renyi_spectrum(&counts);
        results.push(format!(
            "h0={:.15},h1={:.15},h2={:.15},hinf={:.15},div={:.15}",
            s.hartley, s.shannon, s.collision, s.min_entropy, s.divergence
        ));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "Rényi spectrum run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_renyi_matches_shannon_10_runs() {
    let counts: Vec<u64> = vec![7, 3, 5, 2, 8, 1];
    let mut shannon_vals: Vec<String> = Vec::with_capacity(N);
    let mut renyi_h1_vals: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let h_shannon = vajra_stats::shannon_entropy_from_counts(&counts);
        let h_renyi = vajra_stats::renyi_entropy(&counts, 1.0);
        shannon_vals.push(format!("{h_shannon:.15}"));
        renyi_h1_vals.push(format!("{h_renyi:.15}"));
    }
    // Shannon must be identical across runs
    for i in 1..N {
        assert_eq!(shannon_vals[0], shannon_vals[i]);
    }
    // Rényi H₁ must be identical across runs
    for i in 1..N {
        assert_eq!(renyi_h1_vals[0], renyi_h1_vals[i]);
    }
    // They must match each other
    assert_eq!(shannon_vals[0], renyi_h1_vals[0], "H₁ must match Shannon");
}

// -----------------------------------------------------------------------
// LZ Complexity DST
// -----------------------------------------------------------------------

#[test]
fn dst_lz_complexity_10_runs() {
    let data = b"abcabcabcabcdefabcdef123456789abcabcabc";
    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let c = lz76_complexity(data);
        results.push(format!("{c:.15}"));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "LZ complexity run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_lz_for_values_10_runs() {
    let values = vec![
        "PROJ-001", "PROJ-002", "PROJ-003", "active", "active", "inactive",
    ];
    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = lz_complexity_for_values(&values);
        results.push(format!(
            "phrases={},normalized={:.15},len={}",
            r.phrase_count, r.normalized, r.input_length
        ));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "LZ for values run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// Transfer Entropy DST
// -----------------------------------------------------------------------

#[test]
fn dst_transfer_entropy_10_runs() {
    let source = vec![0u64, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
    let mut target = vec![0u64; source.len()];
    target[1..source.len()].copy_from_slice(&source[..(source.len() - 1)]);

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let te = transfer_entropy(&source, &target, 1);
        results.push(format!("{te:.15}"));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "transfer entropy run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_bidirectional_te_10_runs() {
    let source = vec![0u64, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
    let target = vec![2u64, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1];

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = bidirectional_transfer_entropy(&source, &target, 1);
        results.push(format!(
            "xy={:.15},yx={:.15},net={:.15}",
            r.te_source_to_target, r.te_target_to_source, r.net_flow
        ));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "bidirectional TE run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_bin_strings_deterministic() {
    let values = vec!["feat", "fix", "revert", "feat", "chore", "fix"];
    let mut results: Vec<Vec<u64>> = Vec::with_capacity(N);
    for _ in 0..N {
        results.push(bin_strings(&values));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "bin_strings run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// Total Correlation DST
// -----------------------------------------------------------------------

fn make_records(data: &[&[(&str, &str)]]) -> Vec<BTreeMap<String, String>> {
    data.iter()
        .map(|fields| {
            fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        })
        .collect()
}

#[test]
fn dst_total_correlation_10_runs() {
    let records = make_records(&[
        &[("city", "SF"), ("state", "CA"), ("zip", "94103")],
        &[("city", "SF"), ("state", "CA"), ("zip", "94103")],
        &[("city", "NYC"), ("state", "NY"), ("zip", "10001")],
        &[("city", "NYC"), ("state", "NY"), ("zip", "10001")],
        &[("city", "CHI"), ("state", "IL"), ("zip", "60601")],
        &[("city", "CHI"), ("state", "IL"), ("zip", "60601")],
    ]);

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = total_correlation(&records);
        results.push(format!(
            "tc={:.15},norm={:.15},joint={:.15},sum_marg={:.15}",
            r.tc_bits, r.normalized_tc, r.joint_entropy, r.sum_marginal_entropies
        ));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "total correlation run 0 vs run {i} differ"
        );
    }
}

#[test]
fn dst_tc_field_ordering_stable() {
    // Fields are processed via BTreeMap → deterministic ordering
    let records = make_records(&[
        &[("z_field", "a"), ("a_field", "x"), ("m_field", "1")],
        &[("z_field", "b"), ("a_field", "y"), ("m_field", "2")],
        &[("z_field", "a"), ("a_field", "x"), ("m_field", "1")],
    ]);

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let r = total_correlation(&records);
        results.push(format!("{:.15}", r.tc_bits));
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "TC field ordering run 0 vs run {i} differ"
        );
    }
}
