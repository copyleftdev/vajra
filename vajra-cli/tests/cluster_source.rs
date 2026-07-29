//! Integration tests for `vajra cluster` with source-code input.
//!
//! Regression coverage for the bug where `cluster` ignored
//! `--input-format source` and tried to parse every input as JSON.

use std::io::Write;
use std::process::Command;

use tempfile::TempDir;

fn vajra_bin() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.push("target");
    path.push("debug");
    path.push("vajra");
    path
}

fn write(dir: &TempDir, name: &str, body: &str) -> Result<std::path::PathBuf, std::io::Error> {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path)?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(path)
}

/// Two structurally identical modules differing only in identifiers, plus one
/// with a clearly different shape.
const TWIN_A: &str = r"
function alpha(a, b) {
  const total = a + b;
  if (total > 10) { return total * 2; }
  return total;
}
module.exports = alpha;
";

const TWIN_B: &str = r"
function beta(x, y) {
  const sum = x + y;
  if (sum > 10) { return sum * 2; }
  return sum;
}
module.exports = beta;
";

const OUTLIER: &str = r"
class Widget {
  constructor(name) { this.name = name; }
  async render(target, options) {
    for (const key of Object.keys(options)) { target.set(key, options[key]); }
    await target.flush();
    return this.name;
  }
}
module.exports = { Widget };
";

#[test]
fn cluster_accepts_source_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.js", TWIN_A)?;
    let b = write(&dir, "b.js", TWIN_B)?;

    let out = Command::new(vajra_bin())
        .arg("cluster")
        .args([a.to_str().unwrap(), b.to_str().unwrap()])
        .args(["--input-format", "source", "--lang", "javascript"])
        .args(["--format", "json", "--quiet"])
        .output()?;

    assert!(
        out.status.success(),
        "cluster failed on source input: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(json["total_documents"], 2, "both source files parsed");
    Ok(())
}

#[test]
fn cluster_source_groups_structural_twins() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.js", TWIN_A)?;
    let b = write(&dir, "b.js", TWIN_B)?;
    let c = write(&dir, "c.js", OUTLIER)?;

    let out = Command::new(vajra_bin())
        .arg("cluster")
        .args([
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            c.to_str().unwrap(),
        ])
        .args(["--input-format", "source", "--lang", "javascript"])
        .args(["--similarity-threshold", "0.9"])
        .args(["--format", "json", "--quiet"])
        .output()?;

    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let clusters = json["clusters"].as_array().expect("clusters array");

    let twins = clusters
        .iter()
        .find(|c| c["size"].as_u64() == Some(2))
        .expect("the two structural twins should share a cluster");
    let members: Vec<&str> = twins["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    assert!(members.iter().any(|m| m.ends_with("a.js")));
    assert!(members.iter().any(|m| m.ends_with("b.js")));
    Ok(())
}

#[test]
fn cluster_source_directory_picks_up_source_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write(&dir, "a.js", TWIN_A)?;
    write(&dir, "b.js", TWIN_B)?;
    write(&dir, "c.js", OUTLIER)?;
    // A stray JSON file must be ignored when source mode is requested.
    write(&dir, "meta.json", r#"{"unrelated": true}"#)?;

    let out = Command::new(vajra_bin())
        .arg("cluster")
        .arg(dir.path().to_str().unwrap())
        .args(["--input-format", "source", "--lang", "javascript"])
        .args(["--format", "json", "--quiet"])
        .output()?;

    assert!(
        out.status.success(),
        "cluster failed on source directory: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(
        json["total_documents"], 3,
        "should cluster the 3 .js files and skip meta.json"
    );
    Ok(())
}

#[test]
fn cluster_json_directory_still_ignores_source() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write(&dir, "one.json", r#"{"a": 1, "b": 2}"#)?;
    write(&dir, "two.json", r#"{"a": 3, "b": 4}"#)?;
    write(&dir, "ignored.js", TWIN_A)?;

    let out = Command::new(vajra_bin())
        .arg("cluster")
        .arg(dir.path().to_str().unwrap())
        .args(["--format", "json", "--quiet"])
        .output()?;

    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(
        json["total_documents"], 2,
        "default mode still clusters only .json"
    );
    Ok(())
}

#[test]
fn cluster_rejects_out_of_range_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.json", r#"{"a": 1}"#)?;

    let out = Command::new(vajra_bin())
        .arg("cluster")
        .arg(a.to_str().unwrap())
        .args(["--similarity-threshold", "1.5"])
        .output()?;

    assert!(!out.status.success(), "should reject threshold > 1.0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("similarity-threshold"),
        "error should name the flag, got: {stderr}"
    );
    Ok(())
}
