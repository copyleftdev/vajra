//! Guards that the CLI does not silently accept a format it cannot render.
//!
//! `--format markdown` and `--format compact-ai` are byte-identical to `text`
//! for every command except `essence`. Accepting the flag and ignoring it is
//! the same failure mode as reporting `errors: []` over a partial batch: the
//! caller cannot tell "rendered as Markdown" from "fell back to text".

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

const FIXTURE: &str = r#"[{"id":1,"name":"a","score":10.5},{"id":2,"name":"b","score":22.1},
                          {"id":3,"name":"c","score":9.9}]"#;

fn run(input: &Path, args: &[&str]) -> Result<(String, String)> {
    let out = Command::new(vajra_bin())
        .args(args)
        .arg(input.to_str().ok_or_else(|| anyhow!("bad path"))?)
        .output()
        .context("failed to run vajra")?;
    assert!(
        out.status.success(),
        "vajra {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

#[test]
fn unimplemented_format_is_announced() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    for format in ["markdown", "compact-ai"] {
        let (_, stderr) = run(&f, &["stats", "--format", format])?;
        assert!(
            stderr.contains("no ") && stderr.contains("renderer"),
            "`stats --format {format}` must say it has no renderer, got: {stderr:?}"
        );
        assert!(
            stderr.contains("--format json"),
            "should point at a format that works, got: {stderr:?}"
        );
    }
    Ok(())
}

/// `essence` has a real renderer for every format, so it must stay silent.
#[test]
fn implemented_format_is_silent() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    for format in ["markdown", "compact-ai", "text", "json"] {
        let (_, stderr) = run(&f, &["essence", "--format", format])?;
        assert!(
            stderr.is_empty(),
            "`essence --format {format}` should not warn, got: {stderr:?}"
        );
    }
    Ok(())
}

/// Tracking is per-format, not per-command: `anomalies` renders real Markdown
/// but has no compact-AI view, and the notice must reflect exactly that.
#[test]
fn per_format_tracking_is_exact() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (_, md_err) = run(&f, &["anomalies", "--format", "markdown"])?;
    assert!(
        md_err.is_empty(),
        "anomalies renders markdown, so must not warn: {md_err:?}"
    );

    let (_, ai_err) = run(&f, &["anomalies", "--format", "compact-ai"])?;
    assert!(
        ai_err.contains("compact-ai"),
        "anomalies has no compact-ai view, so must warn: {ai_err:?}"
    );
    Ok(())
}

/// A migrated command must emit genuine Markdown, not the text output.
#[test]
fn migrated_command_emits_real_markdown() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (md, _) = run(&f, &["anomalies", "--format", "markdown", "--quiet"])?;
    let (text, _) = run(&f, &["anomalies", "--format", "text", "--quiet"])?;

    assert_ne!(md, text, "markdown must not be the text output");
    assert!(md.contains("## "), "expected Markdown headings:\n{md}");
    assert!(md.contains("|---|"), "expected a Markdown table:\n{md}");
    assert!(
        text.contains("=== "),
        "text keeps its own heading style:\n{text}"
    );
    Ok(())
}

#[test]
fn text_and_json_never_warn() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    for format in ["text", "json"] {
        let (_, stderr) = run(&f, &["stats", "--format", format])?;
        assert!(stderr.is_empty(), "{format} should not warn: {stderr:?}");
    }
    Ok(())
}

/// The warning must not contaminate a pipeline that asked for silence.
#[test]
fn quiet_suppresses_the_warning() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (_, stderr) = run(&f, &["stats", "--format", "markdown", "--quiet"])?;
    assert!(
        stderr.is_empty(),
        "--quiet must silence it, got: {stderr:?}"
    );
    Ok(())
}

/// For an *unmigrated* command the notice is diagnostics only, so stdout must
/// stay byte-identical to the text output.
///
/// This test will start failing when `stats` is migrated onto the renderer —
/// that is the intended signal, not a regression. Point it at another
/// unmigrated command, or delete it once every command renders every format.
#[test]
fn unmigrated_command_stdout_is_unchanged() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (markdown, _) = run(&f, &["stats", "--format", "markdown", "--quiet"])?;
    let (text, _) = run(&f, &["stats", "--format", "text", "--quiet"])?;
    assert_eq!(
        markdown, text,
        "behaviour is unchanged; only the diagnostic is new"
    );
    Ok(())
}
