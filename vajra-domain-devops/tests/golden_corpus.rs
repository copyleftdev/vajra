//! Golden corpus validation for the DevOps domain plugin.
//!
//! Validates that DevOps recognizers correctly identify known infrastructure
//! data types in the corpus fixture files.

use std::path::{Path, PathBuf};

use vajra_domain_devops::DevOpsPlugin;
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
fn terraform_state_contains_expected_types() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("config").join("terraform_state.json");
    if !path.exists() {
        eprintln!("Terraform state corpus not found, skipping");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = DevOpsPlugin;
    let recognizers = plugin.type_recognizers();

    let mut found_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for val in &all_values {
        for r in &recognizers {
            if r.matches(val) {
                found_types.insert(r.type_name().to_string());
            }
        }
    }

    // Terraform state should contain at least CIDR blocks
    assert!(
        found_types.contains("cidr_block"),
        "expected CIDR blocks in Terraform state, found: {:?}",
        found_types
    );

    Ok(())
}

#[test]
fn medical_claim_contains_recognizable_values() -> Result<(), Box<dyn std::error::Error>> {
    // The medical corpus has NPI numbers which are 10-digit strings starting with 1 or 2.
    // Our DevOps plugin shouldn't false-positive on medical data (except maybe git_sha
    // overlapping with hex strings).
    let path = corpus_dir().join("medical").join("single_claim.json");
    if !path.exists() {
        eprintln!("Medical corpus not found, skipping");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = DevOpsPlugin;
    let recognizers = plugin.type_recognizers();

    // Semver recognizer should match version-like strings if any
    // But medical data shouldn't trigger K8s/Terraform/ARN recognizers
    for val in &all_values {
        for r in &recognizers {
            if r.matches(val) {
                // ARN, GCP, and Terraform recognizers should NOT match medical data
                assert!(
                    r.type_name() != "aws_arn"
                        && r.type_name() != "gcp_resource"
                        && r.type_name() != "terraform_resource",
                    "DevOps recognizer '{}' should not match medical value '{}'",
                    r.type_name(),
                    val
                );
            }
        }
    }

    Ok(())
}

#[test]
fn recognizer_determinism_on_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("config").join("terraform_state.json");
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let mut all_results: Vec<Vec<Vec<bool>>> = Vec::new();
    for _ in 0..10 {
        let plugin = DevOpsPlugin;
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
