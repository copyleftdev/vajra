//! `governance` and `score` must work on a git repository with no field flags.
//!
//! The git reader emits `author_name` / `subject` while the governance commands
//! defaulted to `$.author` / `$.message`, so the first thing a user tries —
//! point `governance` at a checkout — failed outright. These are the smoke tests
//! that would have caught it.

use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
        .output()
        .expect("git invocation failed");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build a small repository with two authors and a fix commit, so every
/// governance dimension has something to measure.
fn fixture_repo(dir: &Path) {
    git(dir, &["init", "--quiet", "--initial-branch", "main"]);
    git(dir, &["config", "user.name", "Alice"]);
    git(dir, &["config", "user.email", "alice@example.com"]);
    for (i, (name, email, subject)) in [
        ("Alice", "alice@example.com", "feat: add the thing"),
        ("Alice", "alice@example.com", "feat: add another thing"),
        ("Bob", "bob@example.com", "fix: repair the thing"),
    ]
    .iter()
    .enumerate()
    {
        std::fs::write(dir.join(format!("f{i}.txt")), format!("{i}\n")).expect("write");
        git(dir, &["add", "-A"]);
        git(dir, &["config", "user.name", name]);
        git(dir, &["config", "user.email", email]);
        git(dir, &["commit", "--quiet", "-m", subject]);
    }
}

fn vajra(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vajra"))
        .args(args)
        .output()
        .expect("vajra invocation failed")
}

#[test]
fn governance_reads_a_git_repo_with_no_field_flags() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fixture_repo(tmp.path());
    let repo = tmp.path().to_str().expect("utf-8 path");

    let out = vajra(&[
        "governance",
        repo,
        "--input-format",
        "git",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        out.status.success(),
        "governance failed on a git repo with no field flags: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("governance emitted invalid JSON");
    assert_eq!(
        v["unique_authors"], 2,
        "the git reader's author_name must be resolved, not ignored"
    );
    assert_eq!(v["total_items"], 3);
}

#[test]
fn score_reads_a_git_repo_with_no_field_flags() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fixture_repo(tmp.path());
    let repo = tmp.path().to_str().expect("utf-8 path");

    let out = vajra(&[
        "score",
        repo,
        "--input-format",
        "git",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        out.status.success(),
        "score failed on a git repo with no field flags: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("score emitted invalid JSON");
    // One of three commits has a `fix:` subject. Reaching this value at all
    // proves `$.subject` was resolved: with the old `$.message` default the
    // dimension would be absent entirely.
    let fix_ratio = v["dimensions"]["code_stability"]["value"]
        .as_f64()
        .expect("code_stability requires a resolved message field");
    assert!(
        (fix_ratio - 1.0 / 3.0).abs() < 1e-9,
        "expected fix_ratio 1/3, got {fix_ratio}"
    );
}

#[test]
fn an_explicit_field_flag_is_never_overridden() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fixture_repo(tmp.path());
    let repo = tmp.path().to_str().expect("utf-8 path");

    // author_email is a real field on git records but not a resolution
    // candidate, so seeing its cardinality proves the explicit flag won.
    let out = vajra(&[
        "governance",
        repo,
        "--input-format",
        "git",
        "--author-field",
        "$.author_email",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    assert_eq!(v["unique_authors"], 2);
}

#[test]
fn a_missing_field_names_the_field_and_lists_alternatives() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("records.json");
    std::fs::write(
        &path,
        r#"[{"who":"alice","when":"2026-01-01","nested":{"a":1}}]"#,
    )
    .expect("write");

    let out = vajra(&[
        "governance",
        path.to_str().expect("utf-8 path"),
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(!out.status.success(), "expected a failure");
    let err = String::from_utf8_lossy(&out.stderr);

    assert!(
        err.contains("'$.author'"),
        "the message must name the field it looked for: {err}"
    );
    assert!(
        err.contains("$.who") && err.contains("$.when"),
        "the message must list the fields that are present: {err}"
    );
    assert!(
        !err.contains("$.nested"),
        "nested objects are not usable selectors and must not be offered: {err}"
    );
    assert!(
        err.contains("--author-field"),
        "the message must say what to pass: {err}"
    );
}

/// `compare` resolves once per input, because a git checkout and a GitHub
/// ingest carry different field names and one selector cannot read both.
#[test]
fn compare_resolves_each_dataset_against_its_own_schema() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Git vocabulary: author_name / subject.
    let git_shaped = tmp.path().join("git-shaped.json");
    std::fs::write(
        &git_shaped,
        r#"[{"author_name":"Alice","subject":"feat: a","date":"2026-01-01"},
            {"author_name":"Bob","subject":"fix: b","date":"2026-01-02"}]"#,
    )
    .expect("write");

    // GitHub vocabulary: author / message.
    let github_shaped = tmp.path().join("github-shaped.json");
    std::fs::write(
        &github_shaped,
        r#"[{"author":"Carol","message":"feat: c","date":"2026-01-01"},
            {"author":"Dave","message":"fix: d","date":"2026-01-02"},
            {"author":"Carol","message":"fix: e","date":"2026-01-03"}]"#,
    )
    .expect("write");

    let out = vajra(&[
        "compare",
        git_shaped.to_str().expect("utf-8 path"),
        github_shaped.to_str().expect("utf-8 path"),
        "--labels",
        "git,github",
        "--format",
        "json",
    ]);
    assert!(
        out.status.success(),
        "compare failed across mixed schemas: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    let sets = v["datasets"].as_array().expect("datasets array");
    assert_eq!(sets.len(), 2);

    // Both datasets must be measured. A single shared selector would leave one
    // of them with zero authors, which is the bug this resolution prevents.
    assert_eq!(
        sets[0]["author_cardinality"], 2,
        "the git-shaped dataset must resolve $.author_name"
    );
    assert_eq!(
        sets[1]["author_cardinality"], 2,
        "the github-shaped dataset must resolve $.author"
    );

    // fix_ratio proves the message field resolved too: 1/2 and 2/3.
    let git_fix = sets[0]["fix_ratio"].as_f64().expect("git fix_ratio");
    let github_fix = sets[1]["fix_ratio"].as_f64().expect("github fix_ratio");
    assert!(
        (git_fix - 0.5).abs() < 1e-9,
        "expected 1/2 from $.subject, got {git_fix}"
    );
    assert!(
        (github_fix - 2.0 / 3.0).abs() < 1e-9,
        "expected 2/3 from $.message, got {github_fix}"
    );

    // The note must name the dataset it applies to, or two fallbacks are
    // indistinguishable.
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("[git]"),
        "the resolution note must be attributed to its dataset: {err}"
    );
    assert!(
        !err.contains("[github]"),
        "the github-shaped dataset resolves to the primary candidate and needs no note: {err}"
    );
}

/// The resolution note is a diagnostic; it must not contaminate the JSON on
/// stdout, and `--quiet` must silence it.
#[test]
fn the_resolution_note_goes_to_stderr_and_respects_quiet() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fixture_repo(tmp.path());
    let repo = tmp.path().to_str().expect("utf-8 path");

    let loud = vajra(&[
        "governance",
        repo,
        "--input-format",
        "git",
        "--format",
        "json",
    ]);
    assert!(loud.status.success());
    assert!(
        String::from_utf8_lossy(&loud.stderr).contains("$.author_name"),
        "resolving to a non-primary candidate must be reported"
    );
    serde_json::from_slice::<serde_json::Value>(&loud.stdout)
        .expect("stdout must stay parseable JSON when the note is emitted");

    let quiet = vajra(&[
        "governance",
        repo,
        "--input-format",
        "git",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains("$.author_name"),
        "--quiet must silence the resolution note"
    );
}
