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

    // `batch` is not yet migrated onto the renderer, so it must still announce
    // both unimplemented formats. Repoint when it is migrated.
    let batch_dir = dir.path().join("corpus");
    std::fs::create_dir(&batch_dir)?;
    std::fs::write(batch_dir.join("one.json"), FIXTURE)?;
    for format in ["markdown", "compact-ai"] {
        let (_, stderr) = run(&batch_dir, &["batch", "--format", format])?;
        assert!(
            stderr.contains("no ") && stderr.contains("renderer"),
            "`batch --format {format}` must say it has no renderer, got: {stderr:?}"
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

/// Every command claimed as renderer-backed must genuinely produce output that
/// differs from its text form — measured, not assumed.
///
/// This exists because the claim lists were wrong in both directions when first
/// written: they were built by reading match arms, which missed that `cascade`
/// hand-rolls all three formats and `score` hand-rolls compact-ai. `cascade`
/// was therefore warning about a renderer it has always had. Measuring the
/// output is the only way to keep the lists honest.
#[test]
fn claimed_renderers_produce_distinct_output() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let plain = dir.path().join("d.json");
    std::fs::write(&plain, FIXTURE)?;
    let commits = dir.path().join("commits.json");
    std::fs::write(
        &commits,
        r#"[{"author":"a","date":"2025-01-01T00:00:00Z","subject":"feat: x"},
            {"author":"b","date":"2025-01-02T00:00:00Z","subject":"fix: y"},
            {"author":"a","date":"2025-01-03T00:00:00Z","subject":"feat: z"}]"#,
    )?;
    let events = dir.path().join("events.json");
    std::fs::write(
        &events,
        r#"[{"file":"a.rs","t":"2025-01-01","msg":"feat: add"},
            {"file":"a.rs","t":"2025-01-02","msg":"fix: repair"}]"#,
    )?;

    let cascade_args = [
        "cascade",
        "--entity-field",
        "$.file",
        "--time-field",
        "$.t",
        "--event-field",
        "$.msg",
        "--response-values",
        "fix",
    ];

    // (input, base args, format, must-differ-from-text)
    let cases: &[(&std::path::PathBuf, &[&str], &str)] = &[
        (&plain, &["anomalies"], "markdown"),
        (&plain, &["stats"], "markdown"),
        (&plain, &["invariants"], "markdown"),
        (&plain, &["fingerprint"], "markdown"),
        (&plain, &["essence"], "markdown"),
        (&plain, &["essence"], "compact-ai"),
        (&events, &cascade_args, "markdown"),
        (&events, &cascade_args, "compact-ai"),
        (&commits, &["score"], "compact-ai"),
        (&commits, &["governance"], "markdown"),
        // `compare` takes two inputs so it cannot use the single-input helper;
        // it gets its own assertion below.
    ];

    for (input, base, format) in cases {
        let mut a = base.to_vec();
        a.extend_from_slice(&["--format", format, "--quiet"]);
        let mut t = base.to_vec();
        t.extend_from_slice(&["--format", "text", "--quiet"]);
        let (rendered, err) = run(input, &a)?;
        let (text, _) = run(input, &t)?;
        assert!(
            err.is_empty(),
            "`{} --format {format}` is claimed, so must not warn: {err:?}",
            base[0]
        );
        assert_ne!(
            rendered, text,
            "`{} --format {format}` is claimed but matches its text output",
            base[0]
        );
    }
    Ok(())
}

/// `compare` takes two inputs, so it needs a bespoke invocation rather than the
/// single-input helper — but it is claimed for both Markdown and compact-AI, so
/// it still needs measuring.
#[test]
fn compare_renders_both_claimed_formats() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    std::fs::write(
        &a,
        r#"[{"author":"a","date":"2025-01-01T00:00:00Z","subject":"feat: x"}]"#,
    )?;
    std::fs::write(
        &b,
        r#"[{"author":"b","date":"2025-01-02T00:00:00Z","subject":"fix: y"}]"#,
    )?;

    let invoke = |format: &str| -> Result<(String, String)> {
        let out = Command::new(vajra_bin())
            .arg("compare")
            .arg(a.to_str().ok_or_else(|| anyhow!("bad path"))?)
            .arg(b.to_str().ok_or_else(|| anyhow!("bad path"))?)
            .args(["--format", format, "--quiet"])
            .output()
            .context("failed to run vajra compare")?;
        assert!(
            out.status.success(),
            "compare failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    };

    let (text, _) = invoke("text")?;
    for format in ["markdown", "compact-ai"] {
        let (rendered, _) = invoke(format)?;
        assert_ne!(
            rendered, text,
            "compare is claimed for {format} but matches its text output"
        );
    }
    Ok(())
}

/// The other half of the claim check: a command *not* in a list must genuinely
/// fall through to text, so the warning it emits is accurate.
///
/// Without this, the lists could under-claim indefinitely — which they did.
/// `governance` and `compare` were emitting real Markdown while being told they
/// had no renderer, and only measuring every command surfaced it.
#[test]
fn unclaimed_commands_really_fall_through_to_text() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let plain = dir.path().join("d.json");
    std::fs::write(&plain, FIXTURE)?;
    let commits = dir.path().join("commits.json");
    std::fs::write(
        &commits,
        r#"[{"author":"a","date":"2025-01-01T00:00:00Z","subject":"feat: x"},
            {"author":"b","date":"2025-01-02T00:00:00Z","subject":"fix: y"}]"#,
    )?;

    // `core-team` needs author_name / author_email / date, not the subject-based
    // shape `score` and `governance` accept.
    let team = dir.path().join("team.json");
    std::fs::write(
        &team,
        r#"[{"author_name":"a","author_email":"a@e.com","date":"2025-01-01T00:00:00Z"},
            {"author_name":"b","author_email":"b@e.com","date":"2025-01-02T00:00:00Z"}]"#,
    )?;

    let cases: &[(&std::path::PathBuf, &[&str])] =
        &[(&team, &["core-team"]), (&commits, &["score"])];

    for (input, base) in cases {
        let mut md = base.to_vec();
        md.extend_from_slice(&["--format", "markdown", "--quiet"]);
        let mut tx = base.to_vec();
        tx.extend_from_slice(&["--format", "text", "--quiet"]);
        let (rendered, _) = run(input, &md)?;
        let (text, _) = run(input, &tx)?;
        assert_eq!(
            rendered, text,
            "`{}` is not claimed for markdown, so it must fall through to text",
            base[0]
        );
        // And it must say so.
        let mut noisy = base.to_vec();
        noisy.extend_from_slice(&["--format", "markdown"]);
        let (_, err) = run(input, &noisy)?;
        assert!(
            err.contains("no markdown renderer"),
            "`{}` must announce the fallback: {err:?}",
            base[0]
        );
    }
    Ok(())
}

/// `separation` needs a labelled fixture and its own flags, so it cannot join the
/// simple loop above — but it is claimed as renderer-backed, so it needs the
/// same assertion. Without this, adding a command to `RENDERS_MARKDOWN` without
/// migrating it would go unnoticed, which is the exact rot the loop prevents.
#[test]
fn separation_emits_real_markdown() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("labelled.json");
    let rows: Vec<String> = (0..40)
        .map(|i| {
            let positive = i % 2 == 0;
            let score = if positive { 60 + i } else { i };
            let label = if positive { "aaa_pos" } else { "zzz_neg" };
            format!(r#"{{"score": {score}, "label": "{label}"}}"#)
        })
        .collect();
    std::fs::write(&f, format!("[{}]", rows.join(",")))?;

    let args = [
        "separation",
        "--label-field",
        "label",
        "--base-rate",
        "0.01",
    ];
    let mut md_args = args.to_vec();
    md_args.extend_from_slice(&["--format", "markdown", "--quiet"]);
    let mut text_args = args.to_vec();
    text_args.extend_from_slice(&["--format", "text", "--quiet"]);

    let (md, err) = run(&f, &md_args)?;
    let (text, _) = run(&f, &text_args)?;

    assert!(
        err.is_empty(),
        "separation is migrated, must not warn: {err:?}"
    );
    assert_ne!(md, text, "markdown must differ from text");
    assert!(md.contains("## "), "expected Markdown headings:\n{md}");
    assert!(md.contains("|---|"), "expected a Markdown table:\n{md}");
    // The caveats are the product — they must survive the format.
    assert!(
        md.contains("> ") && md.contains("Ranked by MI"),
        "notes must render as blockquotes:\n{md}"
    );
    Ok(())
}

/// Detailed assertions for one migrated command; the loop above covers the set.
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
/// This is a deliberate tripwire: it will fail when `core-team` is migrated
/// onto the renderer, which is the intended signal rather than a regression.
/// Point it at another unmigrated command then, or delete it once every command
/// renders every format.
#[test]
fn unmigrated_command_stdout_is_unchanged() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("team.json");
    std::fs::write(
        &f,
        r#"[{"author_name":"a","author_email":"a@e.com","date":"2025-01-01T00:00:00Z"}]"#,
    )?;

    let (markdown, _) = run(&f, &["core-team", "--format", "markdown", "--quiet"])?;
    let (text, _) = run(&f, &["core-team", "--format", "text", "--quiet"])?;
    assert_eq!(
        markdown, text,
        "behaviour is unchanged; only the diagnostic is new"
    );
    Ok(())
}

/// Every migrated command must emit genuine Markdown, not the text output.
#[test]
fn all_migrated_commands_emit_real_markdown() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    for command in [
        "anomalies",
        "stats",
        "invariants",
        "fingerprint",
        "inspect",
        "cluster",
    ] {
        let (md, err) = run(&f, &[command, "--format", "markdown", "--quiet"])?;
        let (text, _) = run(&f, &[command, "--format", "text", "--quiet"])?;
        assert!(
            err.is_empty(),
            "`{command}` is migrated, so must not warn: {err:?}"
        );
        assert_ne!(md, text, "`{command}` markdown must differ from text");
        assert!(
            md.contains("## "),
            "`{command}` should emit Markdown headings:\n{md}"
        );
    }
    Ok(())
}
