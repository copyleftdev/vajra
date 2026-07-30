//! `--lang` is a fallback in a corpus walk, not a tree-wide override.
//!
//! A corpus crosses languages. Forcing one grammar onto every file mis-parses
//! the rest: `--corpus --lang javascript` over a mixed tree parsed `.py` files
//! as JavaScript, failed, and dropped them from the index. See #90.

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

/// A tree with one Python and two JavaScript files, each structurally distinct.
fn polyglot_tree() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("script.py"),
        "class Handler:\n    def run(self, items):\n        return [i * 2 for i in items if i]\n",
    )
    .expect("write py");
    std::fs::write(
        tmp.path().join("app.js"),
        "export function run(items) { return items.filter(Boolean).map((i) => i * 2); }\n",
    )
    .expect("write js");
    std::fs::write(
        tmp.path().join("util.js"),
        "const NAMES = ['a', 'b'];\nexport const pick = (n) => NAMES[n % NAMES.length];\n",
    )
    .expect("write js2");
    tmp
}

#[test]
fn an_explicit_lang_does_not_override_a_recognised_extension() {
    let tmp = polyglot_tree();
    let dir = tmp.path().to_str().expect("utf-8 path");

    let forced = json(&[
        "fingerprint",
        dir,
        "--corpus",
        "--input-format",
        "source",
        "--lang",
        "javascript",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(
        forced["errors"].as_array().map(Vec::len),
        Some(0),
        "no file should fail to parse: {:?}",
        forced["errors"]
    );
    assert_eq!(
        forced["documents_indexed"], 3,
        "the Python file must be indexed with its own grammar"
    );

    // Indexing with no --lang at all must produce the identical result, which
    // is the whole point: the flag only fills in for unrecognised extensions.
    let auto = json(&[
        "fingerprint",
        dir,
        "--corpus",
        "--input-format",
        "source",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(
        forced["distinct_shapes"], auto["distinct_shapes"],
        "--lang must not change how a recognised extension is parsed"
    );
    assert_eq!(forced["documents_indexed"], auto["documents_indexed"]);
}

/// A single file named explicitly is a different matter: there the user is
/// pointing at one input and `--lang` is an instruction, not a default.
#[test]
fn an_explicit_lang_still_applies_to_a_single_named_file() {
    let tmp = polyglot_tree();
    let py = tmp.path().join("script.py");
    let py = py.to_str().expect("utf-8 path");

    let as_python = json(&[
        "fingerprint",
        py,
        "--input-format",
        "source",
        "--lang",
        "python",
        "--format",
        "json",
        "--quiet",
    ]);
    let as_javascript = json(&[
        "fingerprint",
        py,
        "--input-format",
        "source",
        "--lang",
        "javascript",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_ne!(
        as_python["path_set"], as_javascript["path_set"],
        "--lang must still choose the grammar for a single named file"
    );
}

/// `raw_size_bytes` is documented as the size of the raw input. For source
/// input it reported the size of the CST's JSON serialisation instead — about
/// 4x the file, and the origin of the impossible byte offsets in #90.
#[test]
fn raw_size_bytes_is_the_source_file_size() {
    let tmp = polyglot_tree();
    let py = tmp.path().join("script.py");
    let on_disk = std::fs::metadata(&py).expect("metadata").len();

    let doc = json(&[
        "inspect",
        py.to_str().expect("utf-8 path"),
        "--input-format",
        "source",
        "--lang",
        "python",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(
        doc["metadata"]["raw_size_bytes"].as_u64(),
        Some(on_disk),
        "raw_size_bytes must be the file, not an intermediate serialisation"
    );
}
