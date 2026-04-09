//! Integration tests for population-level drift (--group-by flag).

use std::io::Write;
use std::process::Command;

use tempfile::NamedTempFile;

fn write_temp_json(content: &str) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    Ok(file)
}

fn vajra_bin() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.push("target");
    path.push("debug");
    path.push("vajra");
    path
}

#[test]
fn two_group_split_detects_drift() -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::json!([
        {"repo": "fastapi", "score": 95, "status": "pass"},
        {"repo": "fastapi", "score": 88, "status": "pass"},
        {"repo": "fastapi", "score": 92, "status": "pass"},
        {"repo": "fastapi", "score": 91, "status": "pass"},
        {"repo": "gin", "score": 12, "status": "fail"},
        {"repo": "gin", "score": 8, "status": "fail"},
        {"repo": "gin", "score": 15, "status": "fail"},
        {"repo": "gin", "score": 10, "status": "fail"},
    ]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args([
            "drift",
            path_str,
            "--group-by",
            "$.repo",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let groups = json["groups"].as_array().ok_or("missing groups")?;
    assert_eq!(groups.len(), 2);
    let group_names: Vec<&str> = groups.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(group_names, vec!["fastapi", "gin"]);
    let pairs = json["pairwise_drift"]
        .as_array()
        .ok_or("missing pairwise_drift")?;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["group_a"].as_str(), Some("fastapi"));
    assert_eq!(pairs[0]["group_b"].as_str(), Some("gin"));
    let sim = pairs[0]["structural_similarity"]
        .as_f64()
        .ok_or("missing structural_similarity")?;
    assert!((sim - 1.0).abs() < f64::EPSILON, "expected ~1.0, got {sim}");
    assert_eq!(json["group_sizes"]["fastapi"].as_u64(), Some(4));
    assert_eq!(json["group_sizes"]["gin"].as_u64(), Some(4));
    Ok(())
}

#[test]
fn three_group_split_produces_three_pairs() -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::json!([
        {"lang": "rust", "score": 90}, {"lang": "rust", "score": 85},
        {"lang": "go", "score": 80}, {"lang": "go", "score": 75},
        {"lang": "python", "score": 70}, {"lang": "python", "score": 65},
    ]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args(["drift", path_str, "--group-by", "lang", "--format", "json"])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["groups"].as_array().ok_or("missing groups")?.len(), 3);
    let pairs = json["pairwise_drift"]
        .as_array()
        .ok_or("missing pairwise_drift")?;
    assert_eq!(pairs.len(), 3);
    let pair_keys: Vec<(String, String)> = pairs
        .iter()
        .map(|p| {
            (
                p["group_a"].as_str().unwrap_or("").to_string(),
                p["group_b"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    assert!(pair_keys.contains(&("go".to_string(), "python".to_string())));
    assert!(pair_keys.contains(&("go".to_string(), "rust".to_string())));
    assert!(pair_keys.contains(&("python".to_string(), "rust".to_string())));
    Ok(())
}

#[test]
fn single_group_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let data =
        serde_json::json!([{"repo": "fastapi", "score": 95}, {"repo": "fastapi", "score": 88}]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args([
            "drift",
            path_str,
            "--group-by",
            "$.repo",
            "--format",
            "json",
        ])
        .output()?;
    assert!(!output.status.success(), "should fail with single group");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("only one group found"), "got: {stderr}");
    Ok(())
}

#[test]
fn missing_group_by_field_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::json!([{"name": "Alice", "score": 95}, {"name": "Bob", "score": 88}]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args([
            "drift",
            path_str,
            "--group-by",
            "$.nonexistent",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        !output.status.success(),
        "should fail when field doesn't exist"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found in any record"), "got: {stderr}");
    Ok(())
}

#[test]
fn text_output_works() -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::json!([
        {"repo": "fastapi", "score": 95}, {"repo": "fastapi", "score": 88},
        {"repo": "gin", "score": 12}, {"repo": "gin", "score": 8},
    ]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args(["drift", path_str, "--group-by", "repo", "--format", "text"])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Population Drift Report"), "missing header");
    assert!(text.contains("fastapi vs gin"), "missing pair");
    assert!(text.contains("Group-by: repo"), "missing group-by");
    Ok(())
}

#[test]
fn dotted_path_group_by() -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::json!([
        {"meta": {"team": "alpha"}, "score": 90}, {"meta": {"team": "alpha"}, "score": 85},
        {"meta": {"team": "beta"}, "score": 10}, {"meta": {"team": "beta"}, "score": 15},
    ]);
    let tmp = write_temp_json(&data.to_string())?;
    let path_str = tmp.path().to_str().ok_or("invalid temp path")?;
    let output = Command::new(vajra_bin())
        .args([
            "drift",
            path_str,
            "--group-by",
            "$.meta.team",
            "--format",
            "json",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let groups = json["groups"].as_array().ok_or("missing groups")?;
    assert_eq!(groups.len(), 2);
    let names: Vec<&str> = groups.iter().filter_map(|v| v.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    Ok(())
}
