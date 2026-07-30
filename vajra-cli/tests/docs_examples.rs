//! Guards the documented JSON examples against the real output schema.
//!
//! Every command page in `docs/src/` previously documented output the command
//! had never emitted — invented keys like `distribution_shifts` where the real
//! key is `distributional_drifts`. A fabricated schema is worse than no docs:
//! someone writes a `jq` filter against it and gets silence rather than an
//! error.
//!
//! This asserts that the top-level keys in each page's first JSON example are
//! keys the command actually produces. It does not check nested structure or
//! values — the aim is to catch wholesale drift, which is the failure that
//! actually happened.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

fn vajra_bin() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("target");
    p.push("debug");
    p.push("vajra");
    p
}

fn docs_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("docs");
    p.push("src");
    p
}

/// Top-level keys of the first parseable JSON object in a page.
fn documented_keys(page: &Path) -> Result<Option<BTreeSet<String>>> {
    let text =
        std::fs::read_to_string(page).with_context(|| format!("cannot read {}", page.display()))?;
    let mut rest = text.as_str();
    while let Some(start) = rest.find("```json") {
        let after = &rest[start + 7..];
        let Some(end) = after.find("```") else { break };
        let body = &after[..end];
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(body)
        {
            return Ok(Some(map.keys().cloned().collect()));
        }
        rest = &after[end..];
    }
    Ok(None)
}

fn real_keys(args: &[&str], input: &Path) -> Result<BTreeSet<String>> {
    let mut cmd = Command::new(vajra_bin());
    cmd.args(args)
        .arg(input.to_str().ok_or_else(|| anyhow!("bad path"))?)
        .args(["--format", "json", "--quiet"]);
    let out = cmd.output().context("failed to run vajra")?;
    assert!(
        out.status.success(),
        "vajra {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("command did not emit JSON")?;
    match v {
        serde_json::Value::Object(map) => Ok(map.keys().cloned().collect()),
        other => Err(anyhow!("expected an object, got {other:?}")),
    }
}

const FIXTURE: &str = r#"[
  {"id":1,"name":"a","score":10.5,"tags":["x","y"],"nested":{"deep":true}},
  {"id":2,"name":"b","score":22.1,"tags":["z"],"nested":{"deep":false}},
  {"id":3,"name":"c","score":9.9,"tags":[],"nested":{"deep":true}}
]"#;

/// Documented keys must be a subset of what the command emits.
///
/// Subset rather than equality: a page may reasonably show a trimmed example,
/// and some fields are omitted when empty. What must never happen is a
/// documented key the command cannot produce.
#[test]
fn documented_examples_use_real_keys() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let fixture = dir.path().join("d.json");
    std::fs::write(&fixture, FIXTURE)?;

    // Optional keys the command omits when empty, so a page may document them
    // even though this fixture does not trigger them.
    let optional: BTreeSet<&str> = ["structural_findings", "windows", "trends"]
        .into_iter()
        .collect();

    let cases: &[(&str, &[&str])] = &[
        ("cmd-inspect.md", &["inspect"]),
        ("cmd-stats.md", &["stats"]),
        ("cmd-anomalies.md", &["anomalies"]),
        ("cmd-essence.md", &["essence"]),
    ];

    for (page, args) in cases {
        let path = docs_dir().join(page);
        let Some(documented) = documented_keys(&path)? else {
            panic!("{page} has no parseable JSON example");
        };
        let real = real_keys(args, &fixture)?;
        let invented: Vec<&String> = documented
            .iter()
            .filter(|k| !real.contains(*k) && !optional.contains(k.as_str()))
            .collect();
        assert!(
            invented.is_empty(),
            "{page} documents keys `{args:?}` never emits: {invented:?}\n  real keys: {real:?}"
        );
    }
    Ok(())
}

/// `cascade` needs field flags, so it gets its own case.
#[test]
fn cascade_example_uses_real_keys() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let fixture = dir.path().join("c.json");
    std::fs::write(
        &fixture,
        r#"[{"file":"a.rs","t":"2025-01-01","msg":"feat: add"},
            {"file":"a.rs","t":"2025-01-02","msg":"fix: repair"}]"#,
    )?;

    let out = Command::new(vajra_bin())
        .arg("cascade")
        .arg(fixture.to_str().ok_or_else(|| anyhow!("bad path"))?)
        .args(["--entity-field", "$.file"])
        .args(["--time-field", "$.t"])
        .args(["--event-field", "$.msg"])
        .args(["--response-values", "fix"])
        .args(["--format", "json", "--quiet"])
        .output()?;
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let real: BTreeSet<String> = v
        .as_object()
        .ok_or_else(|| anyhow!("expected object"))?
        .keys()
        .cloned()
        .collect();

    let documented = documented_keys(&docs_dir().join("cmd-cascade.md"))?
        .ok_or_else(|| anyhow!("no JSON example in cmd-cascade.md"))?;
    let invented: Vec<&String> = documented.iter().filter(|k| !real.contains(*k)).collect();
    assert!(
        invented.is_empty(),
        "cmd-cascade.md documents keys cascade never emits: {invented:?}\n  real: {real:?}"
    );
    Ok(())
}
