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

/// Markdown is now implemented for every command, so the fallback notice only
/// fires for `compact-ai`. This keeps that path covered.
#[test]
fn compact_ai_fallback_is_announced() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (_, stderr) = run(&f, &["stats", "--format", "compact-ai"])?;
    assert!(
        stderr.contains("no compact-ai renderer"),
        "`stats` has no compact-ai view, so must announce the fallback: {stderr:?}"
    );
    let (rendered, _) = run(&f, &["stats", "--format", "compact-ai", "--quiet"])?;
    let (text, _) = run(&f, &["stats", "--format", "text", "--quiet"])?;
    assert_eq!(rendered, text, "the fallback really is the text output");
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

/// `--redact` must apply to text output, not only JSON.
///
/// Six commands ignored it entirely — `fingerprint`, `invariants`, `separation`,
/// `cluster`, `batch`, and `inspect` for its text branch. `separation` is the
/// clearest case: it prints field *values* in the rule column of its
/// operating-point table, so `== "alice@example.com"` reached stdout with
/// `--redact` set. Migrating onto the renderer funnels each command through one
/// output site, which is what makes this a one-line fix per command.
#[test]
fn redact_applies_to_text_output() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("pii.json");
    std::fs::write(
        &f,
        r#"[{"who":"alice@example.com","label":"a"},{"who":"bob@example.com","label":"b"},
            {"who":"alice@example.com","label":"a"},{"who":"bob@example.com","label":"b"}]"#,
    )?;

    let base = [
        "separation",
        "--label-field",
        "label",
        "--base-rate",
        "0.1",
        "--quiet",
    ];

    // Without the flag the value reaches stdout — that is what makes the flag
    // meaningful, and pins the test against a fixture that actually leaks.
    let (plain, _) = run(&f, &base)?;
    assert!(
        plain.contains("example.com"),
        "fixture must actually leak, or the test proves nothing:\n{plain}"
    );

    let mut redacted_args = base.to_vec();
    redacted_args.push("--redact");
    let (redacted, _) = run(&f, &redacted_args)?;
    assert!(
        !redacted.contains("example.com"),
        "--redact must suppress values in text output:\n{redacted}"
    );
    Ok(())
}

/// Sub-modes are a separate rendering path and drifted unnoticed.
///
/// `all_migrated_commands_emit_real_markdown` runs each command against a
/// single JSON file, so it never reached `fingerprint --corpus`,
/// `drift --group-by`, `drift --tree` or `profiles`. All four fell through to
/// the text branch while `fingerprint` and `drift` were listed as rendering
/// Markdown — the claim held for the path under test and not for the others.
#[test]
fn command_sub_modes_emit_real_markdown() -> Result<()> {
    let dir = tempfile::tempdir()?;

    // A corpus: two structurally distinct JSON documents in a tree.
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("a"))?;
    std::fs::create_dir_all(corpus.join("b"))?;
    std::fs::write(corpus.join("a/one.json"), r#"{"x":1,"y":[1,2,3]}"#)?;
    std::fs::write(corpus.join("b/two.json"), r#"{"p":{"q":"z"},"r":false}"#)?;

    // A second tree, for --tree.
    let other = dir.path().join("other");
    std::fs::create_dir_all(other.join("a"))?;
    std::fs::write(other.join("a/one.json"), r#"{"x":1,"y":[1,2,3],"z":9}"#)?;

    // Grouped records, for --group-by.
    let grouped = dir.path().join("grouped.json");
    std::fs::write(
        &grouped,
        r#"[{"team":"red","score":1},{"team":"red","score":2},
            {"team":"blue","score":80},{"team":"blue","extra":true}]"#,
    )?;

    let corpus_s = corpus.to_str().ok_or_else(|| anyhow!("bad path"))?;
    let other_s = other.to_str().ok_or_else(|| anyhow!("bad path"))?;
    let grouped_s = grouped.to_str().ok_or_else(|| anyhow!("bad path"))?;

    let modes: Vec<(&str, Vec<&str>)> = vec![
        (
            "fingerprint --corpus",
            vec!["fingerprint", corpus_s, "--corpus"],
        ),
        ("drift --tree", vec!["drift", corpus_s, other_s, "--tree"]),
        (
            "drift --group-by",
            vec!["drift", grouped_s, "--group-by", "$.team"],
        ),
        ("profiles", vec!["profiles"]),
    ];

    for (label, base) in modes {
        let mut md_args = base.clone();
        md_args.extend_from_slice(&["--format", "markdown", "--quiet"]);
        let mut text_args = base.clone();
        text_args.extend_from_slice(&["--format", "text", "--quiet"]);

        let md = Command::new(vajra_bin()).args(&md_args).output()?;
        let text = Command::new(vajra_bin()).args(&text_args).output()?;
        assert!(
            md.status.success(),
            "`{label}` markdown failed: {}",
            String::from_utf8_lossy(&md.stderr)
        );
        assert!(text.status.success(), "`{label}` text failed");

        let md_out = String::from_utf8_lossy(&md.stdout).into_owned();
        let text_out = String::from_utf8_lossy(&text.stdout).into_owned();

        assert!(
            md.stderr.is_empty(),
            "`{label}` must not warn about markdown: {:?}",
            String::from_utf8_lossy(&md.stderr)
        );
        assert_ne!(
            md_out, text_out,
            "`{label}` markdown is byte-identical to text — it fell through"
        );
        assert!(
            md_out.contains("## "),
            "`{label}` should emit Markdown headings:\n{md_out}"
        );
        assert!(
            !text_out.contains("## "),
            "`{label}` text must not contain Markdown headings:\n{text_out}"
        );
    }
    Ok(())
}

/// `--streaming` advertised "bounded memory". It selects the sketch-based
/// accumulators, but reaching them parses the whole document and then
/// materialises a `Vec<JsonEvent>` beside it — measured at 402 MB against the
/// DOM path's 233 MB on a 15 MB input. A flag that is accepted and does the
/// opposite of what it says is the same defect as `--redact` doing nothing.
/// See #102.
#[test]
fn streaming_says_it_is_not_yet_bounded() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let f = dir.path().join("d.json");
    std::fs::write(&f, FIXTURE)?;

    let (_, stderr) = run(&f, &["stats", "--streaming", "--format", "json"])?;
    assert!(
        stderr.contains("does not yet bound memory"),
        "--streaming must not claim bounded memory silently: {stderr:?}"
    );

    let (_, quiet) = run(&f, &["stats", "--streaming", "--format", "json", "--quiet"])?;
    assert!(quiet.is_empty(), "--quiet must silence it: {quiet:?}");

    let (_, without) = run(&f, &["stats", "--format", "json"])?;
    assert!(
        !without.contains("bound memory"),
        "the warning must not fire without the flag: {without:?}"
    );
    Ok(())
}
