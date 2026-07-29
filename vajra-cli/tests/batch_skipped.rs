//! Integration tests for `vajra batch` file selection and skip reporting.
//!
//! Regression coverage for the bug where `batch` analyzed only the `.json`
//! files in a directory, ignored `--input-format source`, and reported
//! `errors: []` — making a partial run indistinguishable from a complete one.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use tempfile::TempDir;

fn vajra_bin() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

fn write(dir: &TempDir, name: &str, body: &str) -> Result<()> {
    let mut f = std::fs::File::create(dir.path().join(name))?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(())
}

const JS: &str = r"
function handler(req, res) {
  const payload = { ok: true, id: req.id };
  if (!req.id) { return res.status(400).end(); }
  return res.json(payload);
}
module.exports = handler;
";

/// A directory of 3 JSON and 4 JS files.
fn mixed_dir() -> Result<TempDir> {
    let dir = TempDir::new()?;
    write(&dir, "one.json", r#"{"a": 1, "b": 2}"#)?;
    write(&dir, "two.json", r#"{"a": 3, "b": 4}"#)?;
    write(&dir, "three.json", r#"{"a": 5, "b": 6}"#)?;
    for name in ["p.js", "q.js", "r.js", "s.js"] {
        write(&dir, name, JS)?;
    }
    Ok(dir)
}

fn batch_output(dir: &TempDir, extra: &[&str]) -> Result<std::process::Output> {
    Command::new(vajra_bin())
        .arg("batch")
        .arg(as_str(dir.path())?)
        .args(extra)
        .output()
        .context("failed to run vajra batch")
}

fn run_batch(dir: &TempDir, extra: &[&str]) -> Result<serde_json::Value> {
    let mut args = vec!["--format", "json", "--quiet"];
    args.extend_from_slice(extra);
    let out = batch_output(dir, &args)?;
    assert!(
        out.status.success(),
        "batch failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("batch did not emit valid JSON")
}

#[test]
fn batch_reports_skipped_files_in_json_mode() -> Result<()> {
    let dir = mixed_dir()?;
    let json = run_batch(&dir, &[])?;

    assert_eq!(json["total_documents"], 3, "the 3 .json files are analyzed");
    assert_eq!(json["skipped_count"], 4, "the 4 .js files must be reported");

    let skipped = json["skipped"]
        .as_array()
        .ok_or_else(|| anyhow!("skipped should be an array"))?;
    assert_eq!(skipped.len(), 4);
    assert!(skipped
        .iter()
        .all(|s| s.as_str().is_some_and(|n| n.ends_with(".js"))));
    Ok(())
}

#[test]
fn batch_honours_input_format_source() -> Result<()> {
    let dir = mixed_dir()?;
    let json = run_batch(&dir, &["--input-format", "source", "--lang", "javascript"])?;

    assert_eq!(json["total_documents"], 4, "the 4 .js files are analyzed");
    assert_eq!(
        json["skipped_count"], 3,
        "the 3 .json files are reported as skipped"
    );
    assert_eq!(json["errors"].as_array().map(Vec::len), Some(0));
    Ok(())
}

/// A homogeneous directory must report nothing skipped — the field should not
/// become noise for the common case.
#[test]
fn batch_reports_nothing_skipped_when_all_selected() -> Result<()> {
    let dir = TempDir::new()?;
    write(&dir, "a.json", r#"{"x": 1}"#)?;
    write(&dir, "b.json", r#"{"x": 2}"#)?;

    let json = run_batch(&dir, &[])?;
    assert_eq!(json["total_documents"], 2);
    assert_eq!(json["skipped_count"], 0);
    assert_eq!(json["skipped"].as_array().map(Vec::len), Some(0));
    Ok(())
}

/// When nothing matches, the error must say how many files were present so the
/// user can tell "empty directory" from "wrong format".
#[test]
fn batch_error_mentions_unselected_files() -> Result<()> {
    let dir = TempDir::new()?;
    write(&dir, "a.js", JS)?;
    write(&dir, "b.js", JS)?;

    let out = batch_output(&dir, &["--format", "json", "--quiet"])?;

    assert!(!out.status.success(), "no JSON files means failure");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains('2'),
        "error should report 2 unselected files, got: {stderr}"
    );
    Ok(())
}

#[test]
fn batch_skipped_shown_in_text_mode() -> Result<()> {
    let dir = mixed_dir()?;
    let out = batch_output(&dir, &["--quiet"])?;

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("=== Skipped (4) ==="),
        "text output should list skipped files, got:\n{stdout}"
    );
    Ok(())
}

/// Subdirectories are not analyzable inputs and must not be counted as skipped
/// files.
#[test]
fn batch_ignores_subdirectories() -> Result<()> {
    let dir = TempDir::new()?;
    write(&dir, "a.json", r#"{"x": 1}"#)?;
    std::fs::create_dir(dir.path().join("nested"))?;
    std::fs::write(dir.path().join("nested").join("b.json"), r#"{"y": 2}"#)?;

    let json = run_batch(&dir, &[])?;
    assert_eq!(json["total_documents"], 1);
    assert_eq!(
        json["skipped_count"], 0,
        "subdirectory is not a skipped file"
    );
    Ok(())
}
