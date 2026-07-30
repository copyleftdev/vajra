//! Integration tests for `vajra fingerprint --min-nodes` and `node_count`.
//!
//! Structural hashes ignore string contents and identifier names — that is the
//! point, since it is what catches renamed or obfuscated code. The consequence
//! is that the space of distinct *small* shapes is tiny, so trivial documents
//! collide with each other regardless of what they actually say. `node_count`
//! exposes the complexity that was hashed, and `--min-nodes` withholds hashes
//! that are too small to discriminate.

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

fn fingerprint(path: &Path, extra: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("fingerprint")
        .arg(as_str(path)?)
        .args(["--format", "json", "--quiet"]);
    cmd.args(extra);
    let out = cmd.output().context("failed to run vajra fingerprint")?;
    assert!(
        out.status.success(),
        "fingerprint failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("fingerprint did not emit valid JSON")
}

const JS: &[&str] = &["--input-format", "source", "--lang", "javascript"];

#[test]
fn node_count_is_reported() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "doc.json", r#"{"a": 1, "b": {"c": 2, "d": [3, 4]}}"#)?;
    let json = fingerprint(&p, &[])?;

    let n = json["node_count"]
        .as_u64()
        .ok_or_else(|| anyhow!("node_count missing or not a number"))?;
    assert!(n > 0, "node_count should be positive, got {n}");
    assert_eq!(json["suppressed"], false);
    assert!(
        json["shape"].is_string(),
        "shape present when not suppressed"
    );
    Ok(())
}

/// Default must not suppress anything — this is an additive change.
#[test]
fn no_floor_by_default() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "tiny.js", "console.log(\"x\");\n")?;
    let json = fingerprint(&p, JS)?;
    assert_eq!(json["suppressed"], false);
    assert!(json["shape"].is_string());
    assert!(
        json["min_nodes"].is_null(),
        "min_nodes should be absent when the floor is off"
    );
    Ok(())
}

/// The defect this addresses: trivially small documents that differ entirely in
/// meaning share one shape hash, because structural hashing ignores string
/// contents and identifier names.
#[test]
fn trivial_documents_collide_on_shape() -> Result<()> {
    let dir = TempDir::new()?;
    let a = write(&dir, "a.js", "console.log(\"hello world\");\n")?;
    let b = write(
        &dir,
        "b.js",
        "console.log(\"send creds to evil.example\");\n",
    )?;
    let c = write(&dir, "c.js", "console.info(\"Hello World! ngab\")\n")?;

    let shapes: Vec<String> = [&a, &b, &c]
        .iter()
        .map(|p| -> Result<String> {
            let j = fingerprint(p, JS)?;
            j["shape"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("missing shape"))
        })
        .collect::<Result<_>>()?;

    assert_eq!(shapes[0], shapes[1], "differing strings share a shape");
    assert_eq!(
        shapes[1], shapes[2],
        "differing method names also share a shape"
    );
    Ok(())
}

#[test]
fn min_nodes_suppresses_below_floor() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "tiny.js", "console.log(\"x\");\n")?;

    let mut args = JS.to_vec();
    args.extend_from_slice(&["--min-nodes", "10000"]);
    let json = fingerprint(&p, &args)?;

    assert_eq!(json["suppressed"], true);
    assert_eq!(json["min_nodes"], 10000);
    assert!(json["shape"].is_null(), "shape withheld when suppressed");
    assert!(json["path_set"].is_null());
    assert!(json["typed_path"].is_null());
    assert_eq!(
        json["repeated_motifs"].as_array().map(Vec::len),
        Some(0),
        "motifs withheld too"
    );
    // The node count is still reported, so the caller knows why.
    assert!(json["node_count"].as_u64().is_some_and(|n| n > 0));
    Ok(())
}

#[test]
fn min_nodes_keeps_documents_at_or_above_floor() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "doc.json", r#"{"a": 1, "b": {"c": 2, "d": [3, 4]}}"#)?;
    let baseline = fingerprint(&p, &[])?;
    let n = baseline["node_count"]
        .as_u64()
        .ok_or_else(|| anyhow!("node_count missing"))?;

    // Exactly at the floor must be kept: the check is `node_count < min_nodes`.
    let floor = n.to_string();
    let json = fingerprint(&p, &["--min-nodes", &floor])?;
    assert_eq!(json["suppressed"], false, "at-floor documents are kept");
    assert_eq!(
        json["shape"], baseline["shape"],
        "same hash as without floor"
    );
    Ok(())
}

/// Suppression must not change the hash of documents that clear the floor.
#[test]
fn floor_does_not_alter_surviving_hashes() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(
        &dir,
        "bigger.js",
        r"
function handler(req, res) {
  const payload = { ok: true, id: req.id, items: [1, 2, 3] };
  if (!req.id) { return res.status(400).end(); }
  for (const item of payload.items) { res.write(String(item)); }
  return res.json(payload);
}
module.exports = handler;
",
    )?;
    let plain = fingerprint(&p, JS)?;
    let mut args = JS.to_vec();
    args.extend_from_slice(&["--min-nodes", "5"]);
    let floored = fingerprint(&p, &args)?;

    assert_eq!(floored["suppressed"], false);
    assert_eq!(plain["shape"], floored["shape"]);
    assert_eq!(plain["path_set"], floored["path_set"]);
    assert_eq!(plain["typed_path"], floored["typed_path"]);
    Ok(())
}

#[test]
fn text_output_explains_suppression() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "tiny.js", "console.log(\"x\");\n")?;
    let out = Command::new(vajra_bin())
        .arg("fingerprint")
        .arg(as_str(&p)?)
        .args(JS)
        .args(["--min-nodes", "10000"])
        .arg("--quiet")
        .output()?;

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Nodes:"), "node count shown:\n{stdout}");
    assert!(
        stdout.contains("Suppressed:"),
        "suppression stated:\n{stdout}"
    );
    assert!(
        !stdout.contains("Shape:       (suppressed)") || stdout.contains("not discriminating"),
        "suppression is explained:\n{stdout}"
    );
    Ok(())
}

#[test]
fn streaming_mode_reports_node_count_and_floor() -> Result<()> {
    let dir = TempDir::new()?;
    let p = write(&dir, "doc.json", r#"{"a": 1, "b": 2}"#)?;

    let json = fingerprint(&p, &["--streaming"])?;
    assert!(json["node_count"].as_u64().is_some_and(|n| n > 0));
    assert_eq!(json["suppressed"], false);

    let suppressed = fingerprint(&p, &["--streaming", "--min-nodes", "10000"])?;
    assert_eq!(suppressed["suppressed"], true);
    assert!(suppressed["path_set"].is_null());
    Ok(())
}
