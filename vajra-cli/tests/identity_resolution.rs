//! `--resolve-identities` merges contributor aliases before analysis.
//!
//! Git records a `(name, email)` pair chosen per commit, so one person appears
//! under several. Keying on a single field counts them separately and moves
//! every concentration metric. See #88.

use std::process::Command;

fn vajra(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vajra"))
        .args(args)
        .output()
        .expect("vajra invocation failed")
}

fn json(args: &[&str]) -> serde_json::Value {
    let out = vajra(args);
    assert!(
        out.status.success(),
        "vajra {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("invalid JSON")
}

/// The alias structure from the issue: one maintainer under three names across
/// three addresses, linked by shared names and shared emails, plus two others.
fn aliased_commits(path: &std::path::Path) {
    let mut records = Vec::new();
    let maintainer = [
        ("Dietrich", "137048761+Dietrich@users.noreply.github.com"),
        ("Dietrich", "dietrich@gmail.com"),
        ("Dietrich", "dietrich@work.com"),
        ("dgebert", "dietrich@work.com"),
        ("Emeriko", "dietrich@gmail.com"),
    ];
    // 20 maintainer commits spread over the five pairs.
    for i in 0..20 {
        let (name, email) = maintainer[i % maintainer.len()];
        records.push(serde_json::json!({
            "author_name": name,
            "author_email": email,
            "date": format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1),
            "subject": "feat: work",
        }));
    }
    for (i, name) in ["Alice", "Bob"].iter().enumerate() {
        for j in 0..5 {
            records.push(serde_json::json!({
                "author_name": name,
                "author_email": format!("{}@example.com", name.to_lowercase()),
                "date": format!("2026-02-{:02}T00:00:00Z", (i * 5) + j + 1),
                "subject": "feat: work",
            }));
        }
    }
    std::fs::write(
        path,
        serde_json::to_string(&records).expect("serialise fixture"),
    )
    .expect("write fixture");
}

fn fixture() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("commits.json");
    aliased_commits(&path);
    let s = path.to_str().expect("utf-8 path").to_owned();
    (tmp, s)
}

#[test]
fn governance_concentration_changes_when_aliases_merge() {
    let (_tmp, path) = fixture();

    let raw = json(&[
        "governance",
        &path,
        "--author-field",
        "$.author_name",
        "--format",
        "json",
        "--quiet",
    ]);
    let resolved = json(&[
        "governance",
        &path,
        "--author-field",
        "$.author_name",
        "--resolve-identities",
        "--format",
        "json",
        "--quiet",
    ]);

    // Five names before (Dietrich, dgebert, Emeriko, Alice, Bob) -> three after.
    assert_eq!(raw["unique_authors"], 5);
    assert_eq!(resolved["unique_authors"], 3);

    // The maintainer is 20 of 30 commits. Split across three names, the top
    // one holds only a fraction of that.
    let raw_top1 = raw["top1_share"].as_f64().expect("top1_share");
    let resolved_top1 = resolved["top1_share"].as_f64().expect("top1_share");
    assert!(
        (resolved_top1 - 20.0 / 30.0).abs() < 1e-9,
        "expected 20/30, got {resolved_top1}"
    );
    assert!(
        resolved_top1 > raw_top1,
        "merging aliases must raise the measured concentration: {raw_top1} -> {resolved_top1}"
    );

    // The metric the issue is about.
    assert_eq!(raw["bus_factor_50"], 2);
    assert_eq!(
        resolved["bus_factor_50"], 1,
        "one person holds half the commits once the aliases are merged"
    );
}

/// `core-team` keys a contributor on `(name, email)`. Unifying only the name
/// leaves one person counted once per address — which is what my first
/// implementation did.
#[test]
fn core_team_lists_a_merged_identity_exactly_once() {
    let (_tmp, path) = fixture();
    let resolved = json(&[
        "core-team",
        &path,
        "--resolve-identities",
        "--format",
        "json",
        "--quiet",
    ]);

    let names: Vec<&str> = resolved["core"]
        .as_array()
        .into_iter()
        .chain(resolved["community"].as_array())
        .flatten()
        .filter_map(|c| c["name"].as_str())
        .collect();

    let maintainer_entries = names.iter().filter(|n| **n == "Dietrich").count();
    assert_eq!(
        maintainer_entries, 1,
        "the maintainer must appear once, got {names:?}"
    );
    assert!(
        !names.contains(&"Emeriko") && !names.contains(&"dgebert"),
        "no alias may survive as its own contributor: {names:?}"
    );
}

#[test]
fn score_bus_factor_reflects_the_merged_concentration() {
    let (_tmp, path) = fixture();
    let raw = json(&[
        "score",
        &path,
        "--author-field",
        "$.author_name",
        "--message-field",
        "$.subject",
        "--format",
        "json",
        "--quiet",
    ]);
    let resolved = json(&[
        "score",
        &path,
        "--author-field",
        "$.author_name",
        "--message-field",
        "$.subject",
        "--resolve-identities",
        "--format",
        "json",
        "--quiet",
    ]);

    let raw_share = raw["dimensions"]["bus_factor"]["value"]
        .as_f64()
        .expect("top1_share");
    let resolved_share = resolved["dimensions"]["bus_factor"]["value"]
        .as_f64()
        .expect("top1_share");
    assert!(
        resolved_share > raw_share,
        "{raw_share} -> {resolved_share}"
    );
}

/// The merge is aggressive enough that hiding it would be worse than not doing
/// it, so every fold is named on stderr.
#[test]
fn every_merge_is_reported() {
    let (_tmp, path) = fixture();
    let out = vajra(&[
        "governance",
        &path,
        "--author-field",
        "$.author_name",
        "--resolve-identities",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);

    assert!(
        err.contains("Dietrich <- Dietrich, Emeriko, dgebert"),
        "the merge must name every alias it absorbed: {err}"
    );
    assert!(
        err.contains("merged 2 name(s) into 3 contributor(s), from 5"),
        "the summary must state the before and after counts: {err}"
    );
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .expect("stdout must stay parseable JSON");
}

#[test]
fn resolution_is_off_by_default_and_silent_under_quiet() {
    let (_tmp, path) = fixture();

    let default = vajra(&[
        "governance",
        &path,
        "--author-field",
        "$.author_name",
        "--format",
        "json",
    ]);
    assert!(
        !String::from_utf8_lossy(&default.stderr).contains("identity resolution"),
        "resolution must not run unless asked"
    );

    let quiet = vajra(&[
        "governance",
        &path,
        "--author-field",
        "$.author_name",
        "--resolve-identities",
        "--format",
        "json",
        "--quiet",
    ]);
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains("identity resolution"),
        "--quiet must silence the merge report"
    );
}
