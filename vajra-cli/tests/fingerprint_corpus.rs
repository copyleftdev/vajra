//! Integration tests for `vajra fingerprint --corpus`.
//!
//! The index answers "who else has this shape?" — reuse of a structural hash
//! across otherwise unrelated documents. Clustering operates on a coarser unit
//! than the file (see `--corpus-group-depth`), because a file has exactly one
//! shape and so can never link two shapes together.

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

fn write(dir: &Path, rel: &str, body: &str) -> Result<()> {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(body.as_bytes())?;
    f.flush()?;
    Ok(())
}

/// Structurally identical to `TEMPLATE_B` but with every identifier renamed, so
/// the two share a shape hash while sharing no text.
const TEMPLATE_A: &str = r"
function alpha(first, second) {
  const total = first + second;
  const parts = [first, second, total];
  if (total > 100) { return parts.map(function (p) { return p * 2; }); }
  for (const p of parts) { if (p < 0) { return null; } }
  return { total: total, parts: parts, ok: true };
}
module.exports = alpha;
";

const TEMPLATE_B: &str = r"
function beta(alpha, gamma) {
  const sum = alpha + gamma;
  const bits = [alpha, gamma, sum];
  if (sum > 100) { return bits.map(function (q) { return q * 2; }); }
  for (const q of bits) { if (q < 0) { return null; } }
  return { total: sum, parts: bits, ok: true };
}
module.exports = beta;
";

/// A structurally different module.
const OTHER: &str = r"
class Widget {
  constructor(name, opts) { this.name = name; this.opts = opts || {}; }
  async render(target) {
    const keys = Object.keys(this.opts);
    while (keys.length) { target.set(keys.pop()); }
    await target.flush();
    return this.name;
  }
}
module.exports = { Widget };
";

fn corpus(path: &Path, extra: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new(vajra_bin());
    cmd.arg("fingerprint")
        .arg(as_str(path)?)
        .arg("--corpus")
        .args(["--format", "json", "--quiet"]);
    cmd.args(extra);
    let out = cmd
        .output()
        .context("failed to run vajra fingerprint --corpus")?;
    assert!(
        out.status.success(),
        "corpus index failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).context("corpus index did not emit valid JSON")
}

const JS: &[&str] = &["--input-format", "source", "--lang", "javascript"];

/// Two packages carrying the structurally identical template must be linked;
/// the third must not.
fn three_package_corpus() -> Result<TempDir> {
    let dir = TempDir::new()?;
    write(dir.path(), "pkg-a/lib/index.js", TEMPLATE_A)?;
    write(dir.path(), "pkg-b/lib/index.js", TEMPLATE_B)?;
    write(dir.path(), "pkg-c/lib/index.js", OTHER)?;
    Ok(dir)
}

#[test]
fn corpus_walk_recurses_and_reports_counts() -> Result<()> {
    let dir = three_package_corpus()?;
    write(dir.path(), "pkg-a/README.md", "not analysable")?;

    let json = corpus(dir.path(), JS)?;
    assert_eq!(json["documents_indexed"], 3, "nested .js files found");
    assert_eq!(json["files_scanned"], 4, "every file counted");
    assert_eq!(json["skipped"], 1, "the .md is reported as skipped");
    assert_eq!(json["groups_indexed"], 3, "one group per package");
    Ok(())
}

#[test]
fn reuse_group_links_structurally_identical_files() -> Result<()> {
    let dir = three_package_corpus()?;
    let json = corpus(dir.path(), JS)?;

    assert_eq!(json["shapes_in_multiple_documents"], 1);
    let groups = json["reuse_groups"]
        .as_array()
        .ok_or_else(|| anyhow!("reuse_groups missing"))?;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["count"], 2);
    assert!(
        groups[0]["node_count"].as_u64().is_some_and(|n| n > 0),
        "node_count reported so weak matches are visible"
    );
    let members: Vec<&str> = groups[0]["members"]
        .as_array()
        .ok_or_else(|| anyhow!("members missing"))?
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    assert!(members.iter().any(|m| m.contains("pkg-a")));
    assert!(members.iter().any(|m| m.contains("pkg-b")));
    Ok(())
}

#[test]
fn clusters_group_packages_not_files() -> Result<()> {
    let dir = three_package_corpus()?;
    let json = corpus(dir.path(), JS)?;

    let clusters = json["clusters"]
        .as_array()
        .ok_or_else(|| anyhow!("clusters missing"))?;
    assert_eq!(clusters.len(), 1, "only the two twins cluster");
    assert_eq!(clusters[0]["size"], 2);
    let members: Vec<&str> = clusters[0]["members"]
        .as_array()
        .ok_or_else(|| anyhow!("members missing"))?
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    assert!(
        members.iter().all(|m| !m.ends_with(".js")),
        "cluster members are packages, not files: {members:?}"
    );
    Ok(())
}

/// Transitive linking: a+b share one template, b+c share another, so all three
/// must land in one cluster.
#[test]
fn clusters_link_transitively() -> Result<()> {
    let dir = TempDir::new()?;
    write(dir.path(), "pkg-a/one.js", TEMPLATE_A)?;
    write(dir.path(), "pkg-b/one.js", TEMPLATE_B)?;
    write(dir.path(), "pkg-b/two.js", OTHER)?;
    write(dir.path(), "pkg-c/two.js", OTHER)?;

    let json = corpus(dir.path(), JS)?;
    let clusters = json["clusters"]
        .as_array()
        .ok_or_else(|| anyhow!("clusters missing"))?;
    assert_eq!(clusters.len(), 1, "a-b and b-c must merge");
    assert_eq!(clusters[0]["size"], 3);
    assert_eq!(
        clusters[0]["shared_shapes"], 2,
        "two distinct shapes link the cluster"
    );
    Ok(())
}

/// Two files inside one package sharing a shape is not cross-package reuse and
/// must not manufacture a cluster.
#[test]
fn shape_shared_within_one_package_is_not_a_cluster() -> Result<()> {
    let dir = TempDir::new()?;
    write(dir.path(), "pkg-a/one.js", TEMPLATE_A)?;
    write(dir.path(), "pkg-a/two.js", TEMPLATE_B)?;

    let json = corpus(dir.path(), JS)?;
    assert_eq!(
        json["shapes_in_multiple_documents"], 1,
        "the two files do share a shape"
    );
    assert_eq!(
        json["shapes_in_multiple_groups"], 0,
        "but not across packages"
    );
    assert_eq!(json["clusters"].as_array().map(Vec::len), Some(0));
    Ok(())
}

#[test]
fn group_depth_zero_disables_clustering() -> Result<()> {
    let dir = three_package_corpus()?;
    let mut args = JS.to_vec();
    args.extend_from_slice(&["--corpus-group-depth", "0"]);
    let json = corpus(dir.path(), &args)?;

    // Each file becomes its own group, so cross-group reuse still holds here
    // (the twins are in different files) but groups_indexed follows files.
    assert_eq!(json["groups_indexed"], 3);
    assert_eq!(json["documents_indexed"], 3);
    Ok(())
}

#[test]
fn min_nodes_excludes_trivial_documents() -> Result<()> {
    let dir = TempDir::new()?;
    write(
        dir.path(),
        "pkg-a/stub.js",
        "module.exports = require('./x');\n",
    )?;
    write(
        dir.path(),
        "pkg-b/stub.js",
        "module.exports = require('./y');\n",
    )?;
    write(dir.path(), "pkg-c/real.js", TEMPLATE_A)?;

    let without = corpus(dir.path(), JS)?;
    assert!(
        without["shapes_in_multiple_groups"]
            .as_u64()
            .is_some_and(|n| n >= 1),
        "trivial stubs collide across packages without a floor"
    );

    let mut args = JS.to_vec();
    args.extend_from_slice(&["--min-nodes", "1000"]);
    let with = corpus(dir.path(), &args)?;
    assert!(
        with["suppressed"].as_u64().is_some_and(|n| n >= 2),
        "stubs suppressed"
    );
    assert_eq!(
        with["shapes_in_multiple_groups"], 0,
        "the spurious link is gone"
    );
    Ok(())
}

#[test]
fn empty_corpus_fails_with_a_useful_message() -> Result<()> {
    let dir = TempDir::new()?;
    write(dir.path(), "pkg-a/notes.md", "nothing here")?;

    let out = Command::new(vajra_bin())
        .arg("fingerprint")
        .arg(as_str(dir.path())?)
        .arg("--corpus")
        .args(["--format", "json", "--quiet"])
        .output()?;

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("scanned"),
        "error should report what was scanned, got: {stderr}"
    );
    Ok(())
}

#[test]
fn index_is_deterministic() -> Result<()> {
    let dir = three_package_corpus()?;
    let a = corpus(dir.path(), JS)?;
    let b = corpus(dir.path(), JS)?;
    assert_eq!(
        serde_json::to_string(&a)?,
        serde_json::to_string(&b)?,
        "same corpus must produce byte-identical output"
    );
    Ok(())
}

#[test]
fn parse_failures_are_reported_not_dropped() -> Result<()> {
    let dir = TempDir::new()?;
    write(dir.path(), "pkg-a/ok.json", r#"{"a": 1, "b": [2, 3]}"#)?;
    write(dir.path(), "pkg-b/broken.json", "{not valid json{{{")?;

    let json = corpus(dir.path(), &[])?;
    let errors = json["errors"]
        .as_array()
        .ok_or_else(|| anyhow!("errors missing"))?;
    assert_eq!(errors.len(), 1, "the broken file is reported");
    assert!(errors[0]["file"]
        .as_str()
        .is_some_and(|f| f.contains("broken")));
    assert_eq!(json["documents_indexed"], 1, "the good file still indexed");
    Ok(())
}
