//! Golden corpus validation for the GitHub domain plugin.
//!
//! Validates that GitHub recognizers correctly identify known GitHub
//! data types in the corpus fixture files.

use std::path::{Path, PathBuf};

use vajra_domain_github::GitHubPlugin;
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
fn sample_prs_contain_expected_types() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("github").join("sample_prs.json");
    if !path.exists() {
        eprintln!("GitHub corpus not found at {:?}, skipping", path);
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = GitHubPlugin;
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

    // The sample PRs fixture should contain at least these types
    let expected = [
        "github_login",
        "github_bot",
        "pr_state",
        "review_state",
        "semver",
        "git_commit_hash",
        "issue_label",
        "pr_number_ref",
        "iso8601_datetime",
    ];

    for expected_type in &expected {
        assert!(
            found_types.contains(*expected_type),
            "expected to find type '{}' in sample PRs, found: {:?}",
            expected_type,
            found_types
        );
    }

    Ok(())
}

#[test]
fn recognizer_determinism_on_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("github").join("sample_prs.json");
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
        let plugin = GitHubPlugin;
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
