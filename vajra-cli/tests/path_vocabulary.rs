//! Every command reports field paths in one vocabulary.
//!
//! `stats` emits `$[*].author`; `stats --window` used to emit `$.author` for
//! the same field. A consumer that read one form off the other's output got
//! `null` — no error, no warning, and `// 0` flows through arithmetic looking
//! like a real measurement. See #91.

use std::process::Command;

fn vajra(args: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_vajra"))
        .args(args)
        .output()
        .expect("vajra invocation failed");
    assert!(
        out.status.success(),
        "vajra {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("vajra emitted invalid JSON")
}

fn fixture() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("commits.json");
    std::fs::write(
        &path,
        r#"[{"author":"alice","subject":"feat: a","date":"2026-01-05T00:00:00Z"},
            {"author":"bob","subject":"fix: b","date":"2026-01-06T00:00:00Z"},
            {"author":"alice","subject":"feat: c","date":"2026-02-05T00:00:00Z"},
            {"author":"carol","subject":"fix: d","date":"2026-02-06T00:00:00Z"},
            {"author":"bob","subject":"feat: e","date":"2026-03-05T00:00:00Z"}]"#,
    )
    .expect("write");
    let s = path.to_str().expect("utf-8 path").to_owned();
    (tmp, s)
}

#[test]
fn windowed_and_unwindowed_stats_agree_on_path_names() {
    let (_tmp, path) = fixture();

    let plain = vajra(&["stats", &path, "--format", "json", "--quiet"]);
    let mut plain_paths: Vec<String> = plain["paths"]
        .as_array()
        .expect("paths array")
        .iter()
        .filter_map(|p| p["path"].as_str())
        // The root and the bare-array path have no windowed counterpart.
        .filter(|p| *p != "$" && *p != "$[*]")
        .map(str::to_owned)
        .collect();
    plain_paths.sort();

    let windowed = vajra(&[
        "stats",
        &path,
        "--window",
        "month",
        "--time-field",
        "$.date",
        "--format",
        "json",
        "--quiet",
    ]);
    let mut windowed_paths: Vec<String> = windowed["windows"][0]["field_stats"]
        .as_object()
        .expect("field_stats object")
        .keys()
        .cloned()
        .collect();
    // The time field is excluded from windowed per-field stats by design.
    plain_paths.retain(|p| p != "$[*].date");
    windowed_paths.sort();

    assert_eq!(
        plain_paths, windowed_paths,
        "stats and stats --window must name fields identically"
    );
}

#[test]
fn windowed_field_stats_use_the_document_relative_form() {
    let (_tmp, path) = fixture();
    let windowed = vajra(&[
        "stats",
        &path,
        "--window",
        "month",
        "--time-field",
        "$.date",
        "--format",
        "json",
        "--quiet",
    ]);

    let first = &windowed["windows"][0]["field_stats"];
    assert!(
        !first["$[*].author"].is_null(),
        "expected $[*].author, got keys {:?}",
        first.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    assert!(
        first["$.author"].is_null(),
        "the record-relative form must no longer be emitted"
    );
    // The values must still be real, not merely renamed onto nothing.
    assert_eq!(first["$[*].author"]["cardinality"], 2);
}

#[test]
fn trend_keys_use_the_same_vocabulary() {
    let (_tmp, path) = fixture();
    let windowed = vajra(&[
        "stats",
        &path,
        "--window",
        "month",
        "--time-field",
        "$.date",
        "--format",
        "json",
        "--quiet",
    ]);

    let trends = windowed["trends"].as_object().expect("trends object");
    assert!(
        trends.keys().any(|k| k.starts_with("$[*].")),
        "trend keys must match the field_stats vocabulary, got {:?}",
        trends.keys().collect::<Vec<_>>()
    );
    assert!(
        !trends.keys().any(|k| k.starts_with("$.")),
        "no trend key may use the record-relative form, got {:?}",
        trends.keys().collect::<Vec<_>>()
    );
}

/// Normalising the document-relative prefix must not truncate a nested path:
/// `$.commit.date` names a real field and reducing it to `$.date` would break it.
#[test]
fn a_nested_time_field_still_resolves() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("nested.json");
    std::fs::write(
        &path,
        r#"[{"author":"alice","commit":{"date":"2026-01-05T00:00:00Z"}},
            {"author":"bob","commit":{"date":"2026-02-06T00:00:00Z"}}]"#,
    )
    .expect("write");
    let path = path.to_str().expect("utf-8 path");

    for time_field in ["$.commit.date", "$[*].commit.date"] {
        let windowed = vajra(&[
            "stats",
            path,
            "--window",
            "month",
            "--time-field",
            time_field,
            "--format",
            "json",
            "--quiet",
        ]);
        assert_eq!(
            windowed["windows"].as_array().map(Vec::len),
            Some(2),
            "--time-field {time_field} must bucket into two months"
        );
    }
}

/// The time field is excluded however it is spelled, so a caller passing the
/// document-relative form does not get it double-counted as an ordinary field.
#[test]
fn the_time_field_is_excluded_in_either_vocabulary() {
    let (_tmp, path) = fixture();
    for time_field in ["$.date", "$[*].date"] {
        let windowed = vajra(&[
            "stats",
            &path,
            "--window",
            "month",
            "--time-field",
            time_field,
            "--format",
            "json",
            "--quiet",
        ]);
        let keys: Vec<String> = windowed["windows"][0]["field_stats"]
            .as_object()
            .expect("field_stats object")
            .keys()
            .cloned()
            .collect();
        assert!(
            !keys.iter().any(|k| k.ends_with(".date")),
            "--time-field {time_field} must exclude the date field, got {keys:?}"
        );
    }
}
