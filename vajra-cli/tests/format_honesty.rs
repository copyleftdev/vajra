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

/// The warning is diagnostics only — stdout must be byte-identical to before.
#[test]
fn stdout_is_unchanged() -> Result<()> {
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
