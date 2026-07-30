//! Integration tests for `vajra separation`.

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

/// 200 records: `score` separates the classes perfectly, `noise` does not, and
/// `nope` predicts the *negative* class.
fn fixture(dir: &TempDir) -> Result<std::path::PathBuf> {
    let rows: Vec<String> = (0..200)
        .map(|i| {
            let positive = i % 2 == 0;
            let score = if positive { 60 + i % 40 } else { i % 40 };
            let nope = i32::from(!positive);
            let label = if positive { "aaa_pos" } else { "zzz_neg" };
            format!(
                r#"{{"score": {score}, "noise": {}, "nope": {nope}, "label": "{label}"}}"#,
                i % 7
            )
        })
        .collect();
    let path = dir.path().join("labelled.json");
    let mut f = std::fs::File::create(&path)?;
    write!(f, "[{}]", rows.join(","))?;
    f.flush()?;
    Ok(path)
}

fn run(path: &Path, extra: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("separation")
        .arg(as_str(path)?)
        .args(["--label-field", "label"])
        .args(["--format", "json", "--quiet"]);
    cmd.args(extra);
    let out = cmd.output().context("failed to run vajra separation")?;
    assert!(
        out.status.success(),
        "separation failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("separation did not emit valid JSON")
}

fn feature<'a>(json: &'a serde_json::Value, name: &str) -> Result<&'a serde_json::Value> {
    json["features"]
        .as_array()
        .ok_or_else(|| anyhow!("features missing"))?
        .iter()
        .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with(name)))
        .ok_or_else(|| anyhow!("feature {name} missing"))
}

#[test]
fn reports_class_balance_and_baseline() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &[])?;

    assert_eq!(json["labelled_records"], 200);
    assert_eq!(json["classes"]["aaa_pos"], 100);
    assert_eq!(json["classes"]["zzz_neg"], 100);
    assert_eq!(json["binary"], true);
    let baseline = json["baseline_entropy"]
        .as_f64()
        .ok_or_else(|| anyhow!("baseline missing"))?;
    assert!((baseline - 1.0).abs() < 1e-9, "balanced label is 1 bit");
    Ok(())
}

/// The ranking key must be comparable, so no feature may exceed the baseline.
#[test]
fn mutual_information_is_bounded_by_baseline() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &[])?;

    let baseline = json["baseline_entropy"]
        .as_f64()
        .ok_or_else(|| anyhow!("baseline missing"))?;
    for f in json["features"]
        .as_array()
        .ok_or_else(|| anyhow!("features missing"))?
    {
        let mi = f["mutual_information"]
            .as_f64()
            .ok_or_else(|| anyhow!("mi missing"))?;
        assert!(
            mi <= baseline + 1e-9,
            "{} MI {mi} exceeds baseline {baseline}",
            f["path"]
        );
    }
    Ok(())
}

#[test]
fn separating_feature_ranks_first() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &[])?;

    let score = feature(&json, "score")?;
    let noise = feature(&json, "noise")?;
    assert!(
        score["mutual_information"].as_f64() > noise["mutual_information"].as_f64(),
        "score must beat noise"
    );
    assert!((score["separation"].as_f64().unwrap_or(0.0) - 1.0).abs() < 1e-9);
    assert!((score["auc"].as_f64().unwrap_or(0.0) - 1.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn base_rate_prices_precision() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &["--base-rate", "0.0001"])?;

    assert_eq!(json["base_rate"], 0.0001);
    let score = feature(&json, "score")?;
    let op = &score["operating_point"];
    assert!(!op.is_null(), "operating point expected");
    assert!(op["precision_at_base_rate"].as_f64().is_some());
    assert!(op["rule"].as_str().is_some_and(|r| !r.is_empty()));
    Ok(())
}

#[test]
fn positive_class_can_be_selected() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &["--positive-class", "zzz_neg"])?;
    assert_eq!(json["positive_class"], "zzz_neg");
    Ok(())
}

#[test]
fn unknown_positive_class_is_rejected() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let out = Command::new(vajra_bin())
        .arg("separation")
        .arg(as_str(&path)?)
        .args(["--label-field", "label"])
        .args(["--positive-class", "does-not-exist"])
        .output()?;

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("observed classes"),
        "error should list the real classes, got: {stderr}"
    );
    Ok(())
}

/// A field predicting the negative class must yield a usable rule, not one
/// worse than its own inverse.
#[test]
fn negative_predictor_reports_usable_rule() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let json = run(&path, &["--base-rate", "0.01"])?;

    let nope = feature(&json, "nope")?;
    let op = &nope["operating_point"];
    let j = op["youden_j"].as_f64().ok_or_else(|| anyhow!("no J"))?;
    assert!(
        j > 0.0,
        "emitted rule must be the useful direction, J = {j}"
    );
    Ok(())
}

#[test]
fn top_k_limits_output() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let all = run(&path, &[])?;
    let limited = run(&path, &["--top-k", "2"])?;

    let n_all = all["features"].as_array().map_or(0, Vec::len);
    assert!(n_all > 2);
    assert_eq!(limited["features"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[test]
fn missing_label_field_is_rejected() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let out = Command::new(vajra_bin())
        .arg("separation")
        .arg(as_str(&path)?)
        .args(["--label-field", "not_a_field"])
        .output()?;

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found"),
        "error should say the label is absent, got: {stderr}"
    );
    Ok(())
}

#[test]
fn text_output_states_the_caveats() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let out = Command::new(vajra_bin())
        .arg("separation")
        .arg(as_str(&path)?)
        .args(["--label-field", "label"])
        .args(["--base-rate", "0.0001"])
        .arg("--quiet")
        .output()?;

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "Baseline entropy",
        "Positive class",
        "Ranked by MI",
        "PRECISION",
    ] {
        assert!(stdout.contains(expected), "missing {expected:?}:\n{stdout}");
    }
    assert!(
        stdout.contains("prevalence"),
        "text output must state the base-rate caveat:\n{stdout}"
    );
    Ok(())
}

#[test]
fn is_deterministic() -> Result<()> {
    let dir = TempDir::new()?;
    let path = fixture(&dir)?;
    let a = run(&path, &["--base-rate", "0.01"])?;
    let b = run(&path, &["--base-rate", "0.01"])?;
    assert_eq!(serde_json::to_string(&a)?, serde_json::to_string(&b)?);
    Ok(())
}
