//! Golden corpus validation for the security domain plugin.
//!
//! Validates that security recognizers correctly identify known security
//! data types in the corpus fixture files.

use std::path::{Path, PathBuf};

use vajra_domain_sec::SecurityPlugin;
use vajra_types::traits::VajraPlugin;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("corpus")
}

/// Recursively collect all string values from a JSON value.
fn collect_string_values(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_string_values(v, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_string_values(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn siem_events_contain_expected_types() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("security").join("siem_events.json");
    if !path.exists() {
        eprintln!("Security corpus not found, skipping");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = SecurityPlugin;
    let recognizers = plugin.type_recognizers();

    // Track which types we find
    let mut found_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for val in &all_values {
        for r in &recognizers {
            if r.matches(val) {
                found_types.insert(r.type_name().to_string());
            }
        }
    }

    // The SIEM events fixture should contain at least these types
    let expected = [
        "cve_id",
        "ipv4",
        "mitre_attack_id",
        "mitre_tactic",
        "cvss_vector",
        "sha256_hash",
        "sha1_hash",
        "md5_hash",
        "jwt_token",
        "mac_address",
    ];

    for expected_type in &expected {
        assert!(
            found_types.contains(*expected_type),
            "expected to find type '{}' in SIEM events, found: {:?}",
            expected_type,
            found_types
        );
    }

    Ok(())
}

#[test]
fn vulnerability_report_contains_expected_types() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir()
        .join("security")
        .join("vulnerability_report.json");
    if !path.exists() {
        eprintln!("Vulnerability report corpus not found, skipping");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = SecurityPlugin;
    let recognizers = plugin.type_recognizers();

    let mut found_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for val in &all_values {
        for r in &recognizers {
            if r.matches(val) {
                found_types.insert(r.type_name().to_string());
            }
        }
    }

    // Vulnerability report should contain at least CVEs, IPs, CVSS, and MACs
    assert!(
        found_types.contains("cve_id"),
        "expected CVE IDs in vulnerability report"
    );
    assert!(
        found_types.contains("ipv4"),
        "expected IPv4 addresses in vulnerability report"
    );
    assert!(
        found_types.contains("cvss_vector"),
        "expected CVSS vectors in vulnerability report"
    );
    assert!(
        found_types.contains("mac_address"),
        "expected MAC addresses in vulnerability report"
    );

    Ok(())
}

#[test]
fn recognizer_determinism_on_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("security").join("siem_events.json");
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    // Run 10 times, assert identical match results
    let mut all_results: Vec<Vec<Vec<bool>>> = Vec::new();
    for _ in 0..10 {
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let run: Vec<Vec<bool>> = all_values
            .iter()
            .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
            .collect();
        all_results.push(run);
    }

    for (i, run) in all_results.iter().enumerate().skip(1) {
        assert_eq!(
            &all_results[0], run,
            "recognizer results differed on run {}",
            i
        );
    }

    Ok(())
}
