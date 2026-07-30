//! `--provenance` records which build produced a result, and every command's
//! output goes through one exit point.
//!
//! `--version` reported `0.5.0` across 36 commits and eight output-schema
//! changes, and no analysis output carried a build identifier at all, so a
//! stored result could not be traced to what produced it. See #104.

use std::process::Command;

fn vajra(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vajra"))
        .args(args)
        .output()
        .expect("vajra invocation failed")
}

fn stdout_of(args: &[&str]) -> String {
    let out = vajra(args);
    assert!(
        out.status.success(),
        "vajra {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fixture() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("d.json");
    std::fs::write(
        &path,
        r#"[{"label":"spam","reviewer":"alice@example.com","n":1},
            {"label":"ham","reviewer":"bob@example.com","n":2},
            {"label":"spam","reviewer":"alice@example.com","n":3},
            {"label":"ham","reviewer":"carol@example.com","n":4}]"#,
    )
    .expect("write");
    let s = path.to_str().expect("utf-8 path").to_owned();
    (tmp, s)
}

#[test]
fn version_identifies_the_build_not_just_the_release() {
    let out = stdout_of(&["--version"]);
    assert!(out.contains("0.5.0"), "crate version must remain: {out}");
    assert!(
        out.contains('('),
        "version must carry a build identifier: {out}"
    );
    // A commit-shaped token and an ISO date. `unknown` is the documented
    // fallback outside a checkout, and is acceptable here.
    let inside = out
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside.to_owned())
        .unwrap_or_default();
    assert!(
        !inside.trim().is_empty(),
        "build identifier must not be empty: {out}"
    );
}

/// Off by default: attaching provenance unconditionally would change every
/// consumer's output shape, and break the byte-identical guarantee across
/// builds — the property it exists to make checkable.
#[test]
fn provenance_is_off_by_default() {
    let (_tmp, path) = fixture();
    let plain = stdout_of(&["stats", &path, "--format", "json", "--quiet"]);
    assert!(
        !plain.contains("_vajra"),
        "default output must be unchanged: {plain}"
    );

    let v: serde_json::Value = serde_json::from_str(&plain).expect("valid JSON");
    assert!(v.get("paths").is_some(), "root keys must be untouched");
}

/// One envelope shape regardless of whether the command emits an object or an
/// array, so a consumer does not have to branch on the command.
#[test]
fn provenance_wraps_object_and_array_output_identically() {
    let (_tmp, path) = fixture();

    for command in ["stats", "invariants", "inspect", "anomalies"] {
        let out = stdout_of(&[
            command,
            &path,
            "--format",
            "json",
            "--quiet",
            "--provenance",
        ]);
        let v: serde_json::Value =
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("`{command}` invalid JSON: {e}"));

        let record = v
            .get("_vajra")
            .unwrap_or_else(|| panic!("`{command}` missing _vajra: {out}"));
        assert!(v.get("data").is_some(), "`{command}` missing data");
        assert_eq!(
            record["command"], command,
            "`{command}` must name itself in provenance"
        );
        assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
        assert!(
            record["schema"].as_u64().is_some(),
            "`{command}` schema must be an integer to branch on: {out}"
        );
        assert!(
            record["build"].as_str().is_some_and(|s| !s.is_empty()),
            "`{command}` must identify the build: {out}"
        );
    }
}

#[test]
fn provenance_appends_a_line_to_text_and_markdown() {
    let (_tmp, path) = fixture();
    for format in ["text", "markdown"] {
        let out = stdout_of(&[
            "stats",
            &path,
            "--format",
            format,
            "--quiet",
            "--provenance",
        ]);
        assert!(
            out.contains("output schema"),
            "`{format}` must carry a build line: {out}"
        );
        assert!(
            out.trim_end().ends_with(|c: char| c.is_ascii_digit()),
            "the build line belongs at the end: {out}"
        );
    }
}

/// `--redact` was routed through the text branches but eleven commands printed
/// their JSON directly, so `separation --redact --format json` still emitted
/// `alice@example.com`. Both flags now share one exit point, which is what
/// makes this fixable once rather than per command.
#[test]
fn redact_applies_to_json_output_too() {
    let (_tmp, path) = fixture();

    let cases: Vec<Vec<&str>> = vec![
        vec!["separation", &path, "--label-field", "label"],
        vec!["invariants", &path],
        vec!["fingerprint", &path],
        vec!["cluster", &path],
        vec!["stats", &path],
        vec!["anomalies", &path],
        vec!["inspect", &path],
    ];

    for base in cases {
        let label = base[0];
        let mut args = base.clone();
        args.extend_from_slice(&["--redact", "--format", "json", "--quiet"]);
        let out = stdout_of(&args);
        assert!(
            !out.contains("example.com"),
            "`{label}` leaked PII through JSON with --redact:\n{out}"
        );
    }
}

/// Redaction runs before provenance, so the two compose rather than one
/// undoing the other.
#[test]
fn redact_and_provenance_compose() {
    let (_tmp, path) = fixture();
    let out = stdout_of(&[
        "separation",
        &path,
        "--label-field",
        "label",
        "--redact",
        "--provenance",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(!out.contains("example.com"), "PII survived: {out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(v.get("_vajra").is_some(), "provenance missing: {out}");
}
