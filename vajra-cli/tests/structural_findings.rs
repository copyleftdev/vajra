//! Integration tests for package-manifest structural findings.
//!
//! `TypeRecognizer` classifies values; an install-time hook is instead a fact
//! about a *key* existing, which is what `StructuralDetector` exists for.

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

fn write(dir: &TempDir, name: &str, body: &str) -> Result<std::path::PathBuf> {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path)?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(path)
}

fn inspect(path: &Path) -> Result<serde_json::Value> {
    let out = Command::new(vajra_bin())
        .arg("inspect")
        .arg(as_str(path)?)
        .args(["--format", "json", "--quiet"])
        .output()
        .context("failed to run vajra inspect")?;
    assert!(
        out.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("inspect did not emit valid JSON")
}

fn signals(json: &serde_json::Value) -> Vec<String> {
    json["structural_findings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["signal"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn npm_install_hook_is_reported_as_a_concern() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "package.json",
        r#"{"name":"x","version":"1.0.0","scripts":{"preinstall":"node i.js"}}"#,
    )?;
    let json = inspect(&p)?;

    let findings = json["structural_findings"]
        .as_array()
        .ok_or_else(|| anyhow!("structural_findings missing"))?;
    let hook = findings
        .iter()
        .find(|f| f["signal"] == "npm_install_hook")
        .ok_or_else(|| anyhow!("install hook not detected"))?;
    assert_eq!(hook["severity"], "concern");
    assert_eq!(hook["path"], "$.scripts.preinstall");
    Ok(())
}

/// Build and test scripts do not run at install time and must not be flagged.
#[test]
fn ordinary_scripts_are_not_flagged() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "package.json",
        r#"{"name":"x","version":"1.0.0","repository":"git+https://e.com/x",
            "homepage":"https://e.com","bugs":"https://e.com/i","author":"a","license":"MIT",
            "scripts":{"build":"tsc","test":"jest"}}"#,
    )?;
    let json = inspect(&p)?;
    let s = signals(&json);
    assert!(!s.contains(&"npm_install_hook".to_owned()));
    assert!(!s.contains(&"npm_missing_provenance".to_owned()));
    assert!(s.contains(&"npm_script_count".to_owned()));
    Ok(())
}

#[test]
fn cargo_build_script_is_reported() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "manifest.json",
        r#"{"package":{"name":"x","version":"0.1.0","build":"build.rs"}}"#,
    )?;
    assert!(signals(&inspect(&p)?).contains(&"cargo_build_script".to_owned()));
    Ok(())
}

#[test]
fn python_in_tree_backend_is_reported() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "pyproject.json",
        r#"{"build-system":{"requires":["setuptools"],"backend-path":["."]}}"#,
    )?;
    assert!(signals(&inspect(&p)?).contains(&"python_in_tree_backend".to_owned()));
    Ok(())
}

/// The field must be absent, not an empty array, for data that is not a
/// manifest — and no detector may claim arbitrary records.
#[test]
fn unrelated_data_yields_no_findings() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "people.json",
        r#"[{"name":"Alice","age":30},{"name":"Bob","age":25}]"#,
    )?;
    let json = inspect(&p)?;
    assert!(
        json["structural_findings"].is_null(),
        "expected no findings, got {:?}",
        json["structural_findings"]
    );
    Ok(())
}

/// A manifest wrapped in an array (a batch of records) must still be found.
#[test]
fn manifest_inside_an_array_is_found() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "batch.json",
        r#"[{"name":"a","version":"1.0.0","scripts":{"postinstall":"x"}},
            {"name":"b","version":"2.0.0"}]"#,
    )?;
    assert!(signals(&inspect(&p)?).contains(&"npm_install_hook".to_owned()));
    Ok(())
}

#[test]
fn findings_are_ordered_concern_first() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "package.json",
        r#"{"name":"x","version":"1.0.0","dependencies":{"a":"^1"},
            "scripts":{"preinstall":"a"},"bin":{"x":"c.js"}}"#,
    )?;
    let json = inspect(&p)?;
    let severities: Vec<&str> = json["structural_findings"]
        .as_array()
        .ok_or_else(|| anyhow!("missing"))?
        .iter()
        .filter_map(|f| f["severity"].as_str())
        .collect();
    let rank = |s: &str| match s {
        "concern" => 0,
        "notable" => 1,
        _ => 2,
    };
    assert!(
        severities.windows(2).all(|w| rank(w[0]) <= rank(w[1])),
        "expected concern-first ordering, got {severities:?}"
    );
    Ok(())
}

#[test]
fn text_output_explains_concern() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "package.json",
        r#"{"name":"x","version":"1.0.0","scripts":{"preinstall":"a"}}"#,
    )?;
    let out = Command::new(vajra_bin())
        .arg("inspect")
        .arg(as_str(&p)?)
        .arg("--quiet")
        .output()?;
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Structural Findings"), "{stdout}");
    assert!(
        stdout.contains("not a verdict"),
        "must not overclaim:\n{stdout}"
    );
    Ok(())
}

#[test]
fn is_deterministic() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "package.json",
        r#"{"name":"x","version":"1.0.0","scripts":{"postinstall":"a","preinstall":"b"},
            "dependencies":{"a":"^1","b":"^2"}}"#,
    )?;
    let a = inspect(&p)?;
    let b = inspect(&p)?;
    assert_eq!(
        serde_json::to_string(&a["structural_findings"])?,
        serde_json::to_string(&b["structural_findings"])?
    );
    Ok(())
}
