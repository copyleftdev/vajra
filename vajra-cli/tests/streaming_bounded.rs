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

/// Every streamable top-level shape must produce the same path names as the
/// DOM path. A lone object rooted at `$[*]` reported `$[*].a` where the DOM
/// path reports `$.a` — the same input answered two ways depending on a flag.
#[test]
fn path_names_match_the_dom_path_for_every_top_level_shape() {
    let dir = tempfile::tempdir().expect("tempdir");

    let cases = [
        ("object.json", r#"{"a":1,"b":{"c":"x"},"d":[1,2]}"#),
        ("array.json", r#"[{"a":1},{"a":2}]"#),
        ("one_element.json", r#"[{"a":1}]"#),
        ("stream.ndjson", "{\"a\":1}\n{\"a\":2}\n"),
    ];

    for (name, body) in cases {
        let path = dir.path().join(name);
        std::fs::write(&path, body).expect("write");
        let path = path.to_str().expect("utf-8 path");

        let dom = paths_of(&json(&["stats", path, "--format", "json", "--quiet"]));
        let streamed = paths_of(&json(&[
            "stats",
            path,
            "--streaming",
            "--format",
            "json",
            "--quiet",
        ]));
        assert_eq!(
            dom.keys().collect::<Vec<_>>(),
            streamed.keys().collect::<Vec<_>>(),
            "`{name}` must yield the same paths under both modes"
        );
    }
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

    // Cardinality comes from HyperLogLog, which is a two-sided estimator: it
    // may land either side of the truth. An earlier version of this test
    // asserted `reported <= true_card`, which passed only because this fixture
    // happened to underestimate — a specification that a correct
    // implementation is free to violate. The contract is the error bound.
    let dom = paths_of(&json(&["stats", &path, "--format", "json", "--quiet"]));
    let true_card = dom["$[*].v"]["cardinality"].as_u64().expect("cardinality");
    let reported = v["cardinality"].as_u64().expect("cardinality");
    assert!(true_card >= 10_000, "the fixture must cross the threshold");

    #[allow(clippy::cast_precision_loss)]
    let error = (reported as f64 - true_card as f64).abs() / true_card as f64;
    assert!(
        error < 0.10,
        "estimated {reported} against {true_card}, relative error {error:.4}"
    );
    assert!(
        reported > true_card / 2,
        "must not collapse to the Space-Saving budget: {reported} against {true_card}"
    );
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

/// Entropy past the threshold was `log2(top_k)` — 6.64 against a true 16.87,
/// an error of 61% invisible in the number, and not comparable with an exact
/// figure from another path. It is withheld, and the provable ceiling
/// `log2(cardinality)` reported in its place. See #108.
#[test]
fn entropy_is_withheld_past_the_threshold_and_bounded_instead() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Every value distinct, so true entropy is log2(20_000) = 14.29 and the
    // old top-k figure would have been log2(100) = 6.64.
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

    assert!(
        v["entropy"].is_null(),
        "a figure off by 61% must not be reported as entropy: {v}"
    );
    assert!(
        v["normalized_entropy"].is_null(),
        "normalized entropy is derived from it and must go too: {v}"
    );

    let bound = v["entropy_upper_bound"]
        .as_f64()
        .expect("the ceiling must be reported in entropy's place");

    let dom = paths_of(&json(&["stats", &path, "--format", "json", "--quiet"]));
    let truth = dom["$[*].v"]["entropy"].as_f64().expect("true entropy");

    assert!(
        bound >= truth - 0.5,
        "the ceiling must not sit below the truth: {bound} < {truth}"
    );
    assert!(
        (bound - truth).abs() < 1.0,
        "the ceiling should be tight for a near-uniform field: {bound} against {truth}"
    );
    // The point being that this is far better than what it replaced.
    assert!(
        bound > 10.0,
        "log2(top_k) was 6.64; the bound must not resemble it: {bound}"
    );
}

/// The exact path is untouched: entropy present, no ceiling alongside it.
#[test]
fn an_exact_path_reports_entropy_and_no_ceiling() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = corpus(dir.path(), "narrow.json", 4000, 50, false);

    for args in [
        vec!["stats", &path, "--format", "json", "--quiet"],
        vec!["stats", &path, "--streaming", "--format", "json", "--quiet"],
    ] {
        let v = &paths_of(&json(&args))["$[*].v"];
        assert!(v["entropy"].as_f64().is_some(), "entropy must be present");
        assert!(
            v["entropy_upper_bound"].is_null(),
            "an exact figure needs no ceiling: {v}"
        );
    }
}
