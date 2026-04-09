//! Adversarial tests for encoding detection.
//!
//! Tests layered encoding, padding attacks, depth bombs,
//! and false positive resistance.

use vajra_domain_encoding::{detect_encoding_layers, EncodingPlugin};
use vajra_types::traits::{TypeRecognizer, VajraPlugin};

fn all_recognizers() -> Vec<Box<dyn TypeRecognizer>> {
    EncodingPlugin.type_recognizers()
}

// ---------------------------------------------------------------------------
// False positive gauntlet — common values that should NOT match
// ---------------------------------------------------------------------------

#[test]
fn false_positive_english_words() {
    let words = [
        "hello",
        "world",
        "function",
        "variable",
        "database",
        "configuration",
        "deployment",
        "authentication",
        "authorization",
        "infrastructure",
        "orchestration",
        "kubernetes",
        "terraform",
    ];
    let heuristic_recognizers: Vec<Box<dyn TypeRecognizer>> = all_recognizers()
        .into_iter()
        .filter(|r| matches!(r.type_name(), "base64" | "hex_encoded" | "double_encoded"))
        .collect();
    for word in &words {
        for r in &heuristic_recognizers {
            assert!(
                !r.matches(word),
                "{} false-positive on '{}'",
                r.type_name(),
                word
            );
        }
    }
}

#[test]
fn false_positive_uuids() {
    let uuids = [
        "550e8400-e29b-41d4-a716-446655440000",
        "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    ];
    for uuid in &uuids {
        for r in &all_recognizers() {
            assert!(
                !r.matches(uuid),
                "{} false-positive on UUID '{}'",
                r.type_name(),
                uuid
            );
        }
    }
}

#[test]
fn false_positive_iso_dates() {
    let dates = [
        "2025-04-08T14:30:00Z",
        "2025-04-08",
        "Mon, 07 Apr 2025 14:30:00 GMT",
    ];
    for date in &dates {
        for r in &all_recognizers() {
            assert!(
                !r.matches(date),
                "{} false-positive on date '{}'",
                r.type_name(),
                date
            );
        }
    }
}

#[test]
fn false_positive_urls() {
    let urls = [
        "https://example.com/api/v1/users",
        "http://localhost:8080/health",
        "ftp://files.example.com/data.csv",
    ];
    // URLs should not trigger encoding recognizers (they're URLs, not encoded data)
    let heuristic_recognizers: Vec<Box<dyn TypeRecognizer>> = all_recognizers()
        .into_iter()
        .filter(|r| matches!(r.type_name(), "base64" | "hex_encoded" | "double_encoded"))
        .collect();
    for url in &urls {
        for r in &heuristic_recognizers {
            assert!(
                !r.matches(url),
                "{} false-positive on URL '{}'",
                r.type_name(),
                url
            );
        }
    }
}

#[test]
fn false_positive_json_paths() {
    let paths = [
        "$.claims[*].service_lines[*].procedure_code",
        "$.events[0].source_ip",
        "$.metadata.labels.app",
    ];
    for path in &paths {
        for r in &all_recognizers() {
            assert!(
                !r.matches(path),
                "{} false-positive on path '{}'",
                r.type_name(),
                path
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Double encoding detection
// ---------------------------------------------------------------------------

#[test]
fn double_url_encoding_detected() {
    let recognizers = all_recognizers();
    let double_rec = recognizers
        .iter()
        .find(|r| r.type_name() == "double_encoded");
    if let Some(r) = double_rec {
        // %253C = %25 decodes to %, then %3C decodes to <
        assert!(r.matches("%253Cscript%253Ealert%25281%2529%253C%252Fscript%253E"));
    }
}

#[test]
fn single_url_encoding_not_double() {
    let recognizers = all_recognizers();
    let double_rec = recognizers
        .iter()
        .find(|r| r.type_name() == "double_encoded");
    if let Some(r) = double_rec {
        assert!(!r.matches("hello%20world%21"));
    }
}

// ---------------------------------------------------------------------------
// Mixed encoding detection
// ---------------------------------------------------------------------------

#[test]
fn mixed_url_and_html_detected() {
    let recognizers = all_recognizers();
    let mixed_rec = recognizers
        .iter()
        .find(|r| r.type_name() == "mixed_encoding");
    if let Some(r) = mixed_rec {
        assert!(r.matches("hello%20&amp;%20world&lt;test%3E"));
    }
}

// ---------------------------------------------------------------------------
// Layer peeling
// ---------------------------------------------------------------------------

#[test]
fn depth_bomb_capped() {
    // Even with extreme depth request, hard cap at 5
    let layers = detect_encoding_layers("hello%2520world%2521", 255);
    assert!(layers.len() <= 5, "depth bomb: got {} layers", layers.len());
}

#[test]
fn empty_input_no_layers() {
    let layers = detect_encoding_layers("", 5);
    assert!(layers.is_empty());
}

#[test]
fn normal_string_no_layers() {
    let layers = detect_encoding_layers("just a normal string with spaces", 5);
    assert!(layers.is_empty());
}

#[test]
fn single_layer_url() {
    let layers = detect_encoding_layers("hello%20world%21%20more%20stuff", 5);
    assert!(!layers.is_empty());
    assert_eq!(layers[0].encoding, "url_encoded");
    assert_eq!(layers[0].depth, 0);
}

// ---------------------------------------------------------------------------
// Padding manipulation (Base64)
// ---------------------------------------------------------------------------

#[test]
fn base64_extra_padding_rejected() {
    let recognizers = all_recognizers();
    let b64_rec = recognizers.iter().find(|r| r.type_name() == "base64");
    if let Some(r) = b64_rec {
        // Extra padding should not match (not divisible by 4 after padding)
        assert!(!r.matches("SGVsbG8gV29ybGQ=====")); // too much padding
    }
}

#[test]
fn base64_no_padding_short_rejected() {
    let recognizers = all_recognizers();
    let b64_rec = recognizers.iter().find(|r| r.type_name() == "base64");
    if let Some(r) = b64_rec {
        // Short strings should not match
        assert!(!r.matches("SGVsbG8=")); // 8 chars, below 24 threshold
    }
}

// ---------------------------------------------------------------------------
// Determinism under adversarial input
// ---------------------------------------------------------------------------

#[test]
fn adversarial_input_deterministic() {
    let inputs = [
        "%253Cscript%253Ealert%25281%2529",
        "hello%20&amp;%20world&lt;",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "-----BEGIN CERTIFICATE-----\nMIIBxTCCAW...\n-----END CERTIFICATE-----",
        "",
        "   ",
    ];
    let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
    for _ in 0..10 {
        let recognizers = all_recognizers();
        let run: Vec<Vec<bool>> = inputs
            .iter()
            .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
            .collect();
        results.push(run);
    }
    for (i, run) in results.iter().enumerate().skip(1) {
        assert_eq!(&results[0], run, "adversarial: run 0 and run {} differ", i);
    }
}
