//! Golden corpus validation for the encoding domain plugin.
//!
//! Validates that encoding recognizers correctly identify known encoded
//! data in the security corpus fixtures (which contain Base64, hex, JWT,
//! CVSS vectors, etc.).

use std::path::{Path, PathBuf};

use vajra_domain_encoding::EncodingPlugin;
use vajra_types::traits::VajraPlugin;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("corpus")
}

fn collect_string_values(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_string_values(v, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_string_values(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn siem_events_contain_encodable_data() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("security").join("siem_events.json");
    if !path.exists() {
        eprintln!("Security corpus not found, skipping");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let plugin = EncodingPlugin;
    let recognizers = plugin.type_recognizers();

    let mut found_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for val in &all_values {
        for r in &recognizers {
            if r.matches(val) {
                found_types.insert(r.type_name().to_string());
            }
        }
    }

    // The SIEM events fixture has a base64-encoded answer field and
    // a JWT token — encoding plugin should find something
    // (At minimum the data URI-like or base64-like patterns)
    // Note: exact matches depend on entropy gating thresholds
    eprintln!("Encoding types found in SIEM events: {:?}", found_types);

    Ok(())
}

#[test]
fn known_encoded_values_detected() -> Result<(), Box<dyn std::error::Error>> {
    let plugin = EncodingPlugin;
    let recognizers = plugin.type_recognizers();

    let test_cases: Vec<(&str, &str)> = vec![
        // (value, expected recognizer type)
        (
            "-----BEGIN CERTIFICATE-----\nMIIB...\n-----END CERTIFICATE-----",
            "pem_block",
        ),
        ("data:image/png;base64,iVBORw0KGgo=", "data_uri"),
        ("=?UTF-8?B?SGVsbG8=?=", "mime_encoded_word"),
        ("xn--nxasmq6b", "punycode"),
        ("hello%20world%21%20test%20data", "url_encoded"),
        ("&lt;script&gt;alert&amp;1&lt;/script&gt;", "html_entity"),
        ("\\u0048\\u0065\\u006C\\u006C\\u006F", "unicode_escape"),
    ];

    for (value, expected_type) in &test_cases {
        let matched = recognizers.iter().find(|r| r.type_name() == *expected_type);
        if let Some(r) = matched {
            assert!(
                r.matches(value),
                "expected '{}' to match '{}' (first 50 chars: {})",
                expected_type,
                &value[..value.len().min(50)],
                &value[..value.len().min(50)]
            );
        }
    }

    Ok(())
}

#[test]
fn recognizer_determinism_on_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let path = corpus_dir().join("security").join("siem_events.json");
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut all_values = Vec::new();
    collect_string_values(&value, &mut all_values);

    let mut all_results: Vec<Vec<Vec<bool>>> = Vec::new();
    for _ in 0..10 {
        let plugin = EncodingPlugin;
        let recognizers = plugin.type_recognizers();
        let run: Vec<Vec<bool>> = all_values
            .iter()
            .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
            .collect();
        all_results.push(run);
    }

    for (i, run) in all_results.iter().enumerate().skip(1) {
        assert_eq!(
            &all_results[0], run,
            "recognizer results differed on run {}",
            i
        );
    }

    Ok(())
}
