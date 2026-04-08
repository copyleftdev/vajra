//! Determinism verification tests.
//!
//! Validates that the full Vajra analysis pipeline is deterministic:
//! running N times on identical input must produce byte-identical output.

use vajra_anomaly::AnomalyAnalyzer;
use vajra_core::canonical::canonicalize;
use vajra_core::parse::parse_str;
use vajra_essence::profiles::{
    AiProfile, AuditorProfile, EngineerProfile, FraudProfile, StaffProfile,
};
use vajra_essence::render::{render_json, render_text};
use vajra_essence::EssenceBuilder;
use vajra_fingerprint::similarity::{minhash_signature, minhash_similarity};
use vajra_fingerprint::FingerprintAnalyzer;
use vajra_stats::StatsAnalyzer;
use vajra_types::traits::{Analyzer, ConcernProfile, OutputFormat};

const COMPLEX_JSON: &str = include_str!("fixtures/complex.json");
const N: usize = 10;

// -----------------------------------------------------------------------
// 1. Parse determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_parse_10_runs() {
    let mut serialized: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
        let paths: Vec<String> = doc
            .trie()
            .all_paths()
            .iter()
            .map(|p| p.to_string())
            .collect();
        serialized.push(format!("{paths:?}"));
    }
    for i in 1..N {
        assert_eq!(
            serialized[0], serialized[i],
            "parse run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// 2. Canonicalization determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_canonicalization_10_runs() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let mut outputs: Vec<Vec<u8>> = Vec::with_capacity(N);
    for _ in 0..N {
        let canon = canonicalize(doc.value()).unwrap_or_else(|e| panic!("canon failed: {e}"));
        outputs.push(canon);
    }
    for i in 1..N {
        assert_eq!(
            outputs[0], outputs[i],
            "canonicalization run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// 3. Fingerprint determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_fingerprint_10_runs() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let analyzer = FingerprintAnalyzer;

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let fp = analyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("fingerprint failed: {e}"));
        // Serialize all fields to a canonical debug string.
        let serialized = format!(
            "path_set={:?} typed_path={:?} shape={:?} subtree_freqs={:?}",
            fp.path_set, fp.typed_path, fp.shape, fp.subtree_frequencies
        );
        results.push(serialized);
    }
    for i in 1..N {
        assert_eq!(
            results[0], results[i],
            "fingerprint run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// 4. Stats determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_stats_10_runs() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let analyzer = StatsAnalyzer;

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let stats = analyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("stats failed: {e}"));
        // Serialize every path's entropy and cardinality deterministically.
        let mut entries: Vec<String> = stats
            .paths
            .iter()
            .map(|(path, ps)| {
                format!(
                    "{}:entropy={:.15},norm_entropy={:.15},card={},count={},max_rarity={:.15},top={:?}",
                    path, ps.entropy, ps.normalized_entropy, ps.cardinality,
                    ps.total_count, ps.max_rarity, ps.top_values
                )
            })
            .collect();
        entries.sort();
        results.push(entries.join("\n"));
    }
    for i in 1..N {
        assert_eq!(results[0], results[i], "stats run 0 vs run {i} differ");
    }
}

// -----------------------------------------------------------------------
// 5. Anomaly determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_anomaly_10_runs() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let analyzer = AnomalyAnalyzer::default();

    let mut results: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let report = analyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("anomaly failed: {e}"));
        let serialized = format!(
            "numeric_outliers={:?}\nrare_values={:?}\ntype_instabilities={:?}",
            report.numeric_outliers, report.rare_values, report.type_instabilities
        );
        results.push(serialized);
    }
    for i in 1..N {
        assert_eq!(results[0], results[i], "anomaly run 0 vs run {i} differ");
    }
}

// -----------------------------------------------------------------------
// 6. Essence determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_essence_10_runs() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stats = StatsAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("stats failed: {e}"));
    let anomalies = AnomalyAnalyzer::default()
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("anomaly failed: {e}"));
    let fp = FingerprintAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("fingerprint failed: {e}"));
    let profile = EngineerProfile;

    let mut rendered_texts: Vec<String> = Vec::with_capacity(N);
    let mut rendered_jsons: Vec<String> = Vec::with_capacity(N);

    for _ in 0..N {
        let essence = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .with_fingerprint(&fp)
            .build();
        rendered_texts.push(render_text(&essence, "engineer"));
        let json_val = render_json(&essence);
        rendered_jsons.push(serde_json::to_string(&json_val).unwrap_or_default());
    }

    for i in 1..N {
        assert_eq!(
            rendered_texts[0], rendered_texts[i],
            "essence text run 0 vs run {i} differ"
        );
        assert_eq!(
            rendered_jsons[0], rendered_jsons[i],
            "essence JSON run 0 vs run {i} differ"
        );
    }
}

// -----------------------------------------------------------------------
// 7. Essence with different profiles: each produces different but
//    internally deterministic output
// -----------------------------------------------------------------------

#[test]
fn determinism_essence_different_profiles_internally_deterministic() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stats = StatsAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("stats failed: {e}"));
    let anomalies = AnomalyAnalyzer::default()
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("anomaly failed: {e}"));
    let fp = FingerprintAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("fingerprint failed: {e}"));

    let profiles: Vec<(&str, &dyn ConcernProfile)> = vec![
        ("staff", &StaffProfile),
        ("engineer", &EngineerProfile),
        ("fraud", &FraudProfile),
        ("auditor", &AuditorProfile),
        ("ai", &AiProfile),
    ];

    let mut all_outputs: Vec<(String, String)> = Vec::new();

    for (name, profile) in &profiles {
        // Run twice, verify internally identical.
        let mut pair = Vec::new();
        for _ in 0..2 {
            let essence = EssenceBuilder::new(&doc, *profile)
                .with_stats(&stats)
                .with_anomalies(&anomalies)
                .with_fingerprint(&fp)
                .build();
            let text = profile
                .render(&essence, OutputFormat::Text)
                .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));
            pair.push(text);
        }
        assert_eq!(
            pair[0], pair[1],
            "profile '{name}' is not internally deterministic"
        );
        all_outputs.push((
            name.to_string(),
            pair.into_iter().next().unwrap_or_default(),
        ));
    }

    // Verify that different profiles produce different outputs.
    // At least some pairs should differ (staff vs engineer at minimum).
    let mut any_differ = false;
    for i in 0..all_outputs.len() {
        for j in (i + 1)..all_outputs.len() {
            if all_outputs[i].1 != all_outputs[j].1 {
                any_differ = true;
            }
        }
    }
    assert!(
        any_differ,
        "expected at least some profiles to produce different output"
    );
}

// -----------------------------------------------------------------------
// 8. Cross-seed variation for MinHash
// -----------------------------------------------------------------------

#[test]
fn determinism_minhash_same_seed_identical() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let paths = doc.trie().all_paths();
    let k = 128;
    let seed = 42;

    let mut sigs: Vec<String> = Vec::with_capacity(N);
    for _ in 0..N {
        let sig = minhash_signature(&paths, k, seed);
        sigs.push(format!("{sig:?}"));
    }
    for i in 1..N {
        assert_eq!(
            sigs[0], sigs[i],
            "minhash seed={seed} run 0 vs run {i} differ"
        );
    }
}

#[test]
fn determinism_minhash_different_seeds_produce_different_signatures() {
    let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let paths = doc.trie().all_paths();
    let k = 128;

    let sig_a = minhash_signature(&paths, k, 1);
    let sig_b = minhash_signature(&paths, k, 2);

    // Self-similarity should be 1.0 for identical seeds.
    let self_sim = minhash_similarity(&sig_a, &sig_a);
    assert!(
        (self_sim - 1.0).abs() < f64::EPSILON,
        "self-similarity should be 1.0, got {self_sim}"
    );

    // Different seeds should produce different signatures (similarity < 1.0).
    let cross_sim = minhash_similarity(&sig_a, &sig_b);
    assert!(
        cross_sim < 1.0,
        "different seeds should produce different signatures, got similarity {cross_sim}"
    );
}

// -----------------------------------------------------------------------
// Full pipeline end-to-end determinism
// -----------------------------------------------------------------------

#[test]
fn determinism_full_pipeline_10_runs() {
    let mut serialized: Vec<String> = Vec::with_capacity(N);

    for _ in 0..N {
        let doc = parse_str(COMPLEX_JSON).unwrap_or_else(|e| panic!("parse failed: {e}"));

        // Canonicalize
        let canon = canonicalize(doc.value()).unwrap_or_else(|e| panic!("canon failed: {e}"));

        // Stats
        let stats = StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("stats failed: {e}"));

        // Anomalies
        let anomalies = AnomalyAnalyzer::default()
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("anomaly failed: {e}"));

        // Fingerprints
        let fp = FingerprintAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("fingerprint failed: {e}"));

        // Essence (with all analysis results)
        let profile = EngineerProfile;
        let essence = EssenceBuilder::new(&doc, &profile)
            .with_stats(&stats)
            .with_anomalies(&anomalies)
            .with_fingerprint(&fp)
            .build();

        let text = render_text(&essence, "engineer");
        let json = serde_json::to_string(&render_json(&essence)).unwrap_or_default();

        // Combine everything into a single string for comparison.
        let combined = format!(
            "CANON_LEN={}\nTEXT={}\nJSON={}\nPATH_SET={:?}\nSHAPE={:?}",
            canon.len(),
            text,
            json,
            fp.path_set,
            fp.shape,
        );
        serialized.push(combined);
    }

    for i in 1..N {
        assert_eq!(
            serialized[0], serialized[i],
            "full pipeline run 0 vs run {i} differ"
        );
    }
}
