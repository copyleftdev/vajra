//! Integration tests for `vajra cluster` with source-code input.
//!
//! Regression coverage for the bug where `cluster` ignored
//! `--input-format source` and tried to parse every input as JSON.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use tempfile::TempDir;

fn vajra_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.push("target");
    path.push("debug");
    path.push("vajra");
    path
}

fn as_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

fn write(dir: &TempDir, name: &str, body: &str) -> Result<PathBuf> {
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

fn cluster(inputs: &[&Path], extra: &[&str]) -> Result<std::process::Output> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("cluster");
    for input in inputs {
        cmd.arg(as_str(input)?);
    }
    cmd.args(extra)
        .output()
        .context("failed to run vajra cluster")
}

fn cluster_json(inputs: &[&Path], extra: &[&str]) -> Result<serde_json::Value> {
    let mut args = vec!["--format", "json", "--quiet"];
    args.extend_from_slice(extra);
    let out = cluster(inputs, &args)?;
    assert!(
        out.status.success(),
        "cluster failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("cluster did not emit valid JSON")
}

#[test]
fn cluster_accepts_source_files() -> Result<()> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.js", TWIN_A)?;
    let b = write(&dir, "b.js", TWIN_B)?;

    let json = cluster_json(
        &[&a, &b],
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(json["total_documents"], 2, "both source files parsed");
    Ok(())
}

#[test]
fn cluster_source_groups_structural_twins() -> Result<()> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.js", TWIN_A)?;
    let b = write(&dir, "b.js", TWIN_B)?;
    let c = write(&dir, "c.js", OUTLIER)?;

    let json = cluster_json(
        &[&a, &b, &c],
        &[
            "--input-format",
            "source",
            "--lang",
            "javascript",
            "--similarity-threshold",
            "0.9",
        ],
    )?;

    let clusters = json["clusters"]
        .as_array()
        .ok_or_else(|| anyhow!("clusters should be an array"))?;
    let twins = clusters
        .iter()
        .find(|c| c["size"].as_u64() == Some(2))
        .ok_or_else(|| anyhow!("the two structural twins should share a cluster"))?;
    let members: Vec<&str> = twins["members"]
        .as_array()
        .ok_or_else(|| anyhow!("members should be an array"))?
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    assert!(members.iter().any(|m| m.ends_with("a.js")));
    assert!(members.iter().any(|m| m.ends_with("b.js")));
    Ok(())
}

#[test]
fn cluster_source_directory_picks_up_source_files() -> Result<()> {
    let dir = TempDir::new()?;
    write(&dir, "a.js", TWIN_A)?;
    write(&dir, "b.js", TWIN_B)?;
    write(&dir, "c.js", OUTLIER)?;
    // A stray JSON file must be ignored when source mode is requested.
    write(&dir, "meta.json", r#"{"unrelated": true}"#)?;

    let json = cluster_json(
        &[dir.path()],
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(
        json["total_documents"], 3,
        "should cluster the 3 .js files and skip meta.json"
    );
    Ok(())
}

#[test]
fn cluster_json_directory_still_ignores_source() -> Result<()> {
    let dir = TempDir::new()?;
    write(&dir, "one.json", r#"{"a": 1, "b": 2}"#)?;
    write(&dir, "two.json", r#"{"a": 3, "b": 4}"#)?;
    write(&dir, "ignored.js", TWIN_A)?;

    let json = cluster_json(&[dir.path()], &[])?;
    assert_eq!(
        json["total_documents"], 2,
        "default mode still clusters only .json"
    );
    Ok(())
}

#[test]
fn cluster_rejects_out_of_range_threshold() -> Result<()> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.json", r#"{"a": 1}"#)?;

    let out = cluster(&[&a], &["--similarity-threshold", "1.5"])?;

    assert!(!out.status.success(), "should reject threshold > 1.0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("similarity-threshold"),
        "error should name the flag, got: {stderr}"
    );
    Ok(())
}
