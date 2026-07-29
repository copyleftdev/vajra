//! Integration tests for `vajra invariants --bin`.
//!
//! Continuous numeric fields are near-unique, so without discretisation each
//! distinct value maps to exactly one target value and every relationship looks
//! perfect. These tests pin the flag's behaviour and the reported binned flags.

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

/// 400 records where `score` is unique per record and noisily drives `outcome`.
fn fixture(dir: &TempDir) -> Result<std::path::PathBuf> {
    let rows: Vec<String> = (0..400)
        .map(|i| {
            let score = f64::from(i) / 400.0 - 0.5;
            let jitter = f64::from((i * 37) % 11) / 11.0 - 0.5;
            let outcome = if score + jitter * 0.8 > 0.0 {
                "hit"
            } else {
                "miss"
            };
            format!(r#"{{"score": {score:.6}, "outcome": "{outcome}"}}"#)
        })
        .collect();
    let path = dir.path().join("scores.json");
    let mut f = std::fs::File::create(&path)?;
    write!(f, "[{}]", rows.join(","))?;
    f.flush()?;
    Ok(path)
}

fn run(path: &Path, extra: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("invariants")
        .arg(as_str(path)?)
        .args(["--format", "json", "--quiet"]);
    cmd.args(extra);
    let out = cmd.output().context("failed to run vajra invariants")?;
    assert!(
        out.status.success(),
        "invariants failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("invariants did not emit valid JSON")
}

/// Find the `score -> outcome` direction.
fn score_row(json: &serde_json::Value) -> Result<serde_json::Value> {
    json.as_array()
        .ok_or_else(|| anyhow!("expected an array"))?
        .iter()
        .find(|r| {
            r["field_x"].as_str().is_some_and(|s| s.contains("score"))
                && r["field_y"].as_str().is_some_and(|s| s.contains("outcome"))
        })
        .cloned()
        .ok_or_else(|| anyhow!("missing score -> outcome row"))
}

#[test]
fn default_bins_numeric_fields() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let row = score_row(&run(&path, &[])?)?;

    assert_eq!(row["field_x_binned"], true, "score should be binned");
    assert_eq!(row["field_y_binned"], false, "outcome is a string");
    let strength = row["relationship_strength"]
        .as_f64()
        .ok_or_else(|| anyhow!("strength not a number"))?;
    assert!(
        strength > 0.0 && strength < 0.95,
        "expected a real association, got {strength}"
    );
    Ok(())
}

#[test]
fn bin_none_reproduces_degenerate_result() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let row = score_row(&run(&path, &["--bin", "none"])?)?;

    assert_eq!(row["field_x_binned"], false);
    let strength = row["relationship_strength"]
        .as_f64()
        .ok_or_else(|| anyhow!("strength not a number"))?;
    assert!(
        (strength - 1.0).abs() < 1e-9,
        "unbinned continuous field should look perfect, got {strength}"
    );
    Ok(())
}

#[test]
fn equal_width_strategy_is_accepted() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let row = score_row(&run(&path, &["--bin", "equal-width:4"])?)?;
    assert_eq!(row["field_x_binned"], true);
    Ok(())
}

#[test]
fn explicit_quantile_count_is_accepted() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let row = score_row(&run(&path, &["--bin", "quantile:10"])?)?;
    assert_eq!(row["field_x_binned"], true);
    Ok(())
}

#[test]
fn invalid_bin_specs_are_rejected() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;

    for (spec, expect) in [
        ("quantile", "Expected"),
        ("quantile:abc", "expected an integer"),
        ("quantile:1", "at least 2"),
        ("octiles:4", "unknown binning strategy"),
    ] {
        let out = Command::new(vajra_bin())
            .arg("invariants")
            .arg(as_str(&path)?)
            .args(["--bin", spec])
            .output()?;
        assert!(!out.status.success(), "--bin '{spec}' should be rejected");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expect),
            "--bin '{spec}': expected error mentioning {expect:?}, got: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn text_output_marks_binned_fields() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let out = Command::new(vajra_bin())
        .arg("invariants")
        .arg(as_str(&path)?)
        .arg("--quiet")
        .output()?;

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[b]"),
        "text output should mark binned fields, got:\n{stdout}"
    );
    assert!(
        stdout.contains("discretised"),
        "text output should explain the marker, got:\n{stdout}"
    );
    Ok(())
}

/// Booleans and small integer enums must not be silently coarsened.
#[test]
fn low_cardinality_numeric_left_alone() -> Result<()> {
    let dir = TempDir::new()?;
    let rows: Vec<String> = (0..120)
        .map(|i| format!(r#"{{"flag": {}, "tag": "t{}"}}"#, i % 2, i % 3))
        .collect();
    let path = dir.path().join("flags.json");
    let mut f = std::fs::File::create(&path)?;
    write!(f, "[{}]", rows.join(","))?;
    f.flush()?;

    let json = run(&path, &[])?;
    let any_binned = json
        .as_array()
        .ok_or_else(|| anyhow!("expected array"))?
        .iter()
        .any(|r| r["field_x_binned"] == true || r["field_y_binned"] == true);
    assert!(!any_binned, "2 distinct values must not be binned");
    Ok(())
}
