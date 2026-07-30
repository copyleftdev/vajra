//! `--streaming` holds one record at a time, and says when it is approximating.
//!
//! It used to read the whole file, parse a full DOM, then materialise a
//! `Vec<JsonEvent>` beside it — 402 MB against the DOM path's 233 MB on a 15 MB
//! input. The sketch accumulators were already bounded; what was missing was a
//! way to feed them incrementally. See #102.

use std::io::Write;
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

/// `records` rows with `distinct` distinct values in `v`, so the exact/sketch
/// boundary can be crossed deliberately.
fn corpus(
    dir: &std::path::Path,
    name: &str,
    records: usize,
    distinct: usize,
    ndjson: bool,
) -> String {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).expect("create");
    if ndjson {
        for i in 0..records {
            writeln!(f, r#"{{"v":"item{}","n":{}}}"#, i % distinct, i).expect("write");
        }
    } else {
        write!(f, "[").expect("write");
        for i in 0..records {
            if i > 0 {
                write!(f, ",").expect("write");
            }
            write!(f, r#"{{"v":"item{}","n":{}}}"#, i % distinct, i).expect("write");
        }
        write!(f, "]").expect("write");
    }
    path.to_str().expect("utf-8 path").to_owned()
}

fn paths_of(v: &serde_json::Value) -> std::collections::BTreeMap<String, serde_json::Value> {
    v["paths"]
        .as_array()
        .expect("paths array")
        .iter()
        .filter_map(|p| p["path"].as_str().map(|s| (s.to_owned(), p.clone())))
        .collect()
}

/// Below the exact-tracking threshold the streaming path is not an
/// approximation at all — it must agree with the DOM path exactly. If it did
/// not, `--streaming` would silently answer a different question.
#[test]
fn streaming_matches_the_dom_path_below_the_sketch_threshold() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = corpus(dir.path(), "d.json", 4000, 50, false);

    let dom = paths_of(&json(&["stats", &path, "--format", "json", "--quiet"]));
    let streamed = paths_of(&json(&[
        "stats",
        &path,
        "--streaming",
        "--format",
        "json",
        "--quiet",
    ]));

    assert_eq!(
        dom.keys().collect::<Vec<_>>(),
        streamed.keys().collect::<Vec<_>>(),
        "both paths must discover the same fields"
    );

    let v_dom = &dom["$[*].v"];
    let v_str = &streamed["$[*].v"];
    assert_eq!(v_dom["cardinality"], v_str["cardinality"]);
    assert_eq!(v_dom["total_count"], v_str["total_count"]);
    let (a, b) = (
        v_dom["entropy"].as_f64().expect("entropy"),
        v_str["entropy"].as_f64().expect("entropy"),
    );
    assert!(
        (a - b).abs() < 1e-12,
        "entropy must agree exactly below the threshold: {a} vs {b}"
    );
    assert!(
        v_str["exact"].is_null(),
        "an exact result must not be flagged inexact: {v_str}"
    );
}

/// NDJSON is the other streamable shape and must behave identically.
#[test]
fn ndjson_streams_and_agrees_with_the_dom_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = corpus(dir.path(), "d.ndjson", 3000, 40, true);

    let dom = paths_of(&json(&["stats", &path, "--format", "json", "--quiet"]));
    let streamed = paths_of(&json(&[
        "stats",
        &path,
        "--streaming",
        "--format",
        "json",
        "--quiet",
    ]));
    assert_eq!(
        dom["$[*].v"]["cardinality"],
        streamed["$[*].v"]["cardinality"]
    );
    assert_eq!(
        dom["$[*].v"]["total_count"],
        streamed["$[*].v"]["total_count"]
    );
}

/// Past the threshold the figures are sketch output, and must say so rather
/// than sitting in the same fields the DOM path uses for true values.
#[test]
fn sketch_results_are_marked_inexact() {
    let dir = tempfile::tempdir().expect("tempdir");
    // 20,000 distinct values, over the 10,000 exact-tracking threshold.
    let path = corpus(dir.path(), "wide.json", 20_000, 20_000, false);

    let streamed = paths_of(&json(&[
        "stats",
        &path,
        "--streaming",
        "--format",
        "json",
        "--quiet",
    ]));
    let v = &streamed["$[*].v"];
    assert_eq!(
        v["exact"], false,
        "a sketch figure must be flagged, not reported as a measurement: {v}"
    );

    // The lower-bound property that is actually claimed: at least this many
    // distinct values were seen.
    let dom = paths_of(&json(&["stats", &path, "--format", "json", "--quiet"]));
    let true_card = dom["$[*].v"]["cardinality"].as_u64().expect("cardinality");
    let reported = v["cardinality"].as_u64().expect("cardinality");
    assert!(
        reported <= true_card,
        "cardinality must not exceed the truth: {reported} > {true_card}"
    );
    assert!(true_card >= 10_000, "the fixture must cross the threshold");
}

/// The default path is unaffected, so nothing carries the flag by accident.
#[test]
fn the_dom_path_is_always_exact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = corpus(dir.path(), "wide.json", 20_000, 20_000, false);
    let dom = json(&["stats", &path, "--format", "json", "--quiet"]);
    assert!(
        !dom.to_string().contains("\"exact\""),
        "exact is omitted when true, so DOM output is unchanged"
    );
}

/// A caller who passes `--streaming` to survive a large input needs to know
/// when it did not apply.
#[test]
fn a_non_streamable_input_says_it_was_loaded_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("d.csv");
    std::fs::write(&path, "a,b\n1,2\n3,4\n").expect("write");

    let out = vajra(&[
        "stats",
        path.to_str().expect("utf-8 path"),
        "--input-format",
        "csv",
        "--streaming",
        "--format",
        "json",
    ]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("loaded whole"),
        "the fallback must be reported: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
