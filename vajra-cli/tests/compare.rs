//! Integration tests for `vajra compare` — cross-repo comparison.

use std::io::Write;
use std::process::Command;

use tempfile::NamedTempFile;

fn write_temp_json(content: &str) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let mut file = NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    Ok(file)
}

fn vajra_bin() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.push("target");
    path.push("debug");
    path.push("vajra");
    path
}

/// Build a synthetic dataset: an array of commit-like records.
fn make_dataset(authors: &[&str], messages: &[&str]) -> serde_json::Value {
    let mut records = Vec::new();
    for (i, (author, msg)) in authors.iter().zip(messages.iter()).enumerate() {
        records.push(serde_json::json!({
            "author": author,
            "date": format!("2025-01-{:02}T00:00:00Z", (i % 28) + 1),
            "subject": msg,
        }));
    }
    serde_json::Value::Array(records)
}

// ---------------------------------------------------------------------------
// Basic 2-dataset comparison
// ---------------------------------------------------------------------------

#[test]
fn compare_two_datasets_json() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(
        &["alice", "bob", "alice", "charlie"],
        &["add feature", "fix bug", "update docs", "fix tests"],
    );
    let ds2 = make_dataset(
        &["dave", "dave", "dave", "eve"],
        &["add feature", "add feature", "fix bug", "refactor"],
    );

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args([
            "compare",
            p1,
            p2,
            "--author-field",
            "$.author",
            "--message-field",
            "$.subject",
            "--format",
            "json",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    // Should have 2 datasets
    let datasets = json["datasets"].as_array().ok_or("missing datasets")?;
    assert_eq!(datasets.len(), 2);

    // Each dataset has 4 records
    assert_eq!(datasets[0]["total_records"].as_u64(), Some(4));
    assert_eq!(datasets[1]["total_records"].as_u64(), Some(4));

    // Entropy should be positive for both
    let e1 = datasets[0]["author_entropy"]
        .as_f64()
        .ok_or("missing entropy")?;
    let e2 = datasets[1]["author_entropy"]
        .as_f64()
        .ok_or("missing entropy")?;
    assert!(e1 > 0.0, "dataset 1 entropy should be positive, got {e1}");
    assert!(e2 > 0.0, "dataset 2 entropy should be positive, got {e2}");

    // Dataset 1 has 3 distinct authors, dataset 2 has 2
    assert_eq!(datasets[0]["author_cardinality"].as_u64(), Some(3));
    assert_eq!(datasets[1]["author_cardinality"].as_u64(), Some(2));

    // Fix ratio: ds1 has 2/4 = 0.5, ds2 has 1/4 = 0.25
    let fr1 = datasets[0]["fix_ratio"]
        .as_f64()
        .ok_or("missing fix_ratio")?;
    let fr2 = datasets[1]["fix_ratio"]
        .as_f64()
        .ok_or("missing fix_ratio")?;
    assert!((fr1 - 0.5).abs() < 1e-6, "expected 0.5, got {fr1}");
    assert!((fr2 - 0.25).abs() < 1e-6, "expected 0.25, got {fr2}");

    // Pairwise drift: exactly 1 pair
    let pairs = json["pairwise_drift"].as_array().ok_or("missing pairs")?;
    assert_eq!(pairs.len(), 1);

    // Pair has severity and similarity
    assert!(pairs[0]["severity"].as_str().is_some());
    assert!(pairs[0]["similarity"].as_f64().is_some());

    Ok(())
}

// ---------------------------------------------------------------------------
// 3-dataset comparison produces 3 pairs
// ---------------------------------------------------------------------------

#[test]
fn compare_three_datasets_three_pairs() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(&["a", "b"], &["add x", "fix y"]);
    let ds2 = make_dataset(&["c", "d"], &["add z", "add w"]);
    let ds3 = make_dataset(&["e", "e"], &["fix a", "fix b"]);

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let tmp3 = write_temp_json(&ds3.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;
    let p3 = tmp3.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args(["compare", p1, p2, p3, "--format", "json"])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let datasets = json["datasets"].as_array().ok_or("missing datasets")?;
    assert_eq!(datasets.len(), 3);

    let pairs = json["pairwise_drift"].as_array().ok_or("missing pairs")?;
    // 3 choose 2 = 3 pairs
    assert_eq!(pairs.len(), 3);

    Ok(())
}

// ---------------------------------------------------------------------------
// Single dataset -> error
// ---------------------------------------------------------------------------

#[test]
fn compare_single_dataset_errors() -> Result<(), Box<dyn std::error::Error>> {
    let ds = make_dataset(&["a", "b"], &["add x", "fix y"]);
    let tmp = write_temp_json(&ds.to_string())?;
    let p = tmp.path().to_str().ok_or("bad path")?;

    // clap enforces num_args = 2.., so passing 1 arg should fail
    let output = Command::new(vajra_bin())
        .args(["compare", p, "--format", "json"])
        .output()?;

    assert!(
        !output.status.success(),
        "should fail with a single dataset"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Labels: comma-separated
// ---------------------------------------------------------------------------

#[test]
fn compare_with_labels() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(&["a", "b"], &["add x", "fix y"]);
    let ds2 = make_dataset(&["c", "d"], &["add z", "add w"]);

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args([
            "compare",
            p1,
            p2,
            "--labels",
            "React,Effect-TS",
            "--format",
            "json",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let datasets = json["datasets"].as_array().ok_or("missing datasets")?;
    assert_eq!(datasets[0]["label"].as_str(), Some("React"));
    assert_eq!(datasets[1]["label"].as_str(), Some("Effect-TS"));

    // Drift pairs should use the labels
    let pairs = json["pairwise_drift"].as_array().ok_or("missing pairs")?;
    assert_eq!(pairs[0]["a"].as_str(), Some("React"));
    assert_eq!(pairs[0]["b"].as_str(), Some("Effect-TS"));

    Ok(())
}

// ---------------------------------------------------------------------------
// Filename fallback for labels
// ---------------------------------------------------------------------------

#[test]
fn compare_filename_fallback_labels() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(&["a", "b"], &["add x", "fix y"]);
    let ds2 = make_dataset(&["c", "d"], &["add z", "add w"]);

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args(["compare", p1, p2, "--format", "json"])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let datasets = json["datasets"].as_array().ok_or("missing datasets")?;

    // Labels should be derived from filenames (tempfile names are random but non-empty)
    let label0 = datasets[0]["label"].as_str().ok_or("missing label")?;
    let label1 = datasets[1]["label"].as_str().ok_or("missing label")?;
    assert!(!label0.is_empty(), "label should not be empty");
    assert!(!label1.is_empty(), "label should not be empty");
    assert_ne!(
        label0, label1,
        "labels should be distinct for different files"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Markdown output verification
// ---------------------------------------------------------------------------

#[test]
fn compare_markdown_output() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(
        &["alice", "bob", "alice"],
        &["add feature", "fix bug", "update docs"],
    );
    let ds2 = make_dataset(
        &["charlie", "charlie", "dave"],
        &["fix test", "fix lint", "add CI"],
    );

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args([
            "compare",
            p1,
            p2,
            "--labels",
            "ProjectA,ProjectB",
            "--format",
            "markdown",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8_lossy(&output.stdout);

    // Check markdown structure
    assert!(text.contains("## Project Comparison"), "missing header");
    assert!(text.contains("| Metric |"), "missing table header");
    assert!(text.contains("| Records |"), "missing records row");
    assert!(text.contains("| Author entropy |"), "missing entropy row");
    assert!(
        text.contains("| Author cardinality |"),
        "missing cardinality row"
    );
    assert!(text.contains("| Fix ratio |"), "missing fix ratio row");
    assert!(
        text.contains("| One-commit rate |"),
        "missing one-commit row"
    );
    assert!(text.contains("## Pairwise Drift"), "missing drift section");
    assert!(
        text.contains("| Pair | Severity | Similarity |"),
        "missing drift header"
    );
    assert!(text.contains("ProjectA vs ProjectB"), "missing pair label");

    Ok(())
}

// ---------------------------------------------------------------------------
// Text output verification
// ---------------------------------------------------------------------------

#[test]
fn compare_text_output() -> Result<(), Box<dyn std::error::Error>> {
    let ds1 = make_dataset(&["a", "b"], &["add x", "fix y"]);
    let ds2 = make_dataset(&["c", "d"], &["add z", "add w"]);

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args([
            "compare",
            p1,
            p2,
            "--labels",
            "Alpha,Beta",
            "--format",
            "text",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("=== Project Comparison ==="),
        "missing header"
    );
    assert!(
        text.contains("=== Pairwise Drift ==="),
        "missing drift section"
    );
    assert!(text.contains("Alpha"), "missing label Alpha");
    assert!(text.contains("Beta"), "missing label Beta");

    Ok(())
}

// ---------------------------------------------------------------------------
// One-commit rate calculation
// ---------------------------------------------------------------------------

#[test]
fn compare_one_commit_rate() -> Result<(), Box<dyn std::error::Error>> {
    // ds1: 4 authors, all unique (one-commit rate = 100%)
    let ds1 = make_dataset(&["a", "b", "c", "d"], &["add x", "add y", "add z", "add w"]);
    // ds2: 2 authors, "x" has 3 commits, "y" has 1 => one_commit_rate = 1/2 = 50%
    let ds2 = make_dataset(&["x", "x", "x", "y"], &["add a", "add b", "add c", "add d"]);

    let tmp1 = write_temp_json(&ds1.to_string())?;
    let tmp2 = write_temp_json(&ds2.to_string())?;
    let p1 = tmp1.path().to_str().ok_or("bad path")?;
    let p2 = tmp2.path().to_str().ok_or("bad path")?;

    let output = Command::new(vajra_bin())
        .args(["compare", p1, p2, "--format", "json"])
        .output()?;

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let datasets = json["datasets"].as_array().ok_or("missing datasets")?;

    let ocr1 = datasets[0]["one_commit_rate"]
        .as_f64()
        .ok_or("missing ocr")?;
    let ocr2 = datasets[1]["one_commit_rate"]
        .as_f64()
        .ok_or("missing ocr")?;

    assert!((ocr1 - 1.0).abs() < 1e-6, "expected 1.0, got {ocr1}");
    assert!((ocr2 - 0.5).abs() < 1e-6, "expected 0.5, got {ocr2}");

    Ok(())
}
