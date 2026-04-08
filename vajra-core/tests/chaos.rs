//! Chaos and adversarial input tests.
//!
//! Feed pathological inputs to every stage of the pipeline and verify
//! NO PANICS and graceful error handling.

use vajra_anomaly::AnomalyAnalyzer;
use vajra_core::canonical::canonicalize;
use vajra_core::parse::parse_str;
use vajra_fingerprint::FingerprintAnalyzer;
use vajra_stats::StatsAnalyzer;
use vajra_types::traits::Analyzer;

// -----------------------------------------------------------------------
// Deep nesting
// -----------------------------------------------------------------------

#[test]
fn chaos_deeply_nested_127_levels() {
    // serde_json's default recursion limit is 128, so 127 nested objects
    // (depth 127 after walk) should succeed.
    let depth = 127;
    let mut json = String::new();
    for _ in 0..depth {
        json.push_str(r#"{"a":"#);
    }
    json.push('1');
    for _ in 0..depth {
        json.push('}');
    }
    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed at depth {depth}: {e}"));
    assert_eq!(doc.metadata().max_depth, depth as u32);
}

#[test]
fn chaos_deeply_nested_beyond_serde_limit() {
    // 200 levels deep exceeds serde_json's default recursion limit of 128.
    // Should return a parse error, NOT panic.
    let depth = 200;
    let mut json = String::new();
    for _ in 0..depth {
        json.push_str(r#"{"a":"#);
    }
    json.push('1');
    for _ in 0..depth {
        json.push('}');
    }
    let result = parse_str(&json);
    assert!(result.is_err(), "expected error for depth {depth}, got Ok");
}

// -----------------------------------------------------------------------
// Extremely wide objects
// -----------------------------------------------------------------------

#[test]
fn chaos_extremely_wide_object() {
    let mut json = String::from("{");
    for i in 0..10_000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(r#""key_{i}": {i}"#));
    }
    json.push('}');

    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(doc.metadata().max_depth, 1);

    // Run stats and fingerprint -- should not panic.
    let _ = StatsAnalyzer.analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Empty everything
// -----------------------------------------------------------------------

#[test]
fn chaos_empty_everything() {
    let inputs = vec!["{}", "[]", r#""""#, "0", "null", "false", "true"];

    for input in inputs {
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed for '{input}': {e}"));

        // Full pipeline: no panics.
        let _ = canonicalize(doc.value());
        let _ = StatsAnalyzer.analyze(&doc);
        let _ = AnomalyAnalyzer::default().analyze(&doc);
        let _ = FingerprintAnalyzer.analyze(&doc);
    }
}

// -----------------------------------------------------------------------
// Null everywhere
// -----------------------------------------------------------------------

#[test]
fn chaos_null_everywhere() {
    let json = r#"{"a": null, "b": [null, null], "c": {"d": null}}"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let stats = StatsAnalyzer.analyze(&doc);
    assert!(stats.is_ok(), "stats should handle nulls");

    let anomalies = AnomalyAnalyzer::default().analyze(&doc);
    assert!(anomalies.is_ok(), "anomalies should handle nulls");

    let fp = FingerprintAnalyzer.analyze(&doc);
    assert!(fp.is_ok(), "fingerprint should handle nulls");
}

// -----------------------------------------------------------------------
// Type chaos
// -----------------------------------------------------------------------

#[test]
fn chaos_type_chaos() {
    let json = r#"[1, "two", true, null, {"x": 1}, [1, 2], 3.14]"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let stats = StatsAnalyzer.analyze(&doc);
    assert!(stats.is_ok(), "stats should handle mixed types");

    let anomalies = AnomalyAnalyzer::default().analyze(&doc);
    assert!(anomalies.is_ok(), "anomalies should handle mixed types");

    // Check that the trie records type instability for $[*].
    let root_array_path = vajra_types::path::WildcardPath::root().push_array_wildcard();
    if let Some(node) = doc.trie().get(&root_array_path) {
        assert!(
            node.metadata.type_instability() > 0.0,
            "expected type instability for mixed-type array, got {}",
            node.metadata.type_instability()
        );
    }
}

// -----------------------------------------------------------------------
// Unicode stress
// -----------------------------------------------------------------------

#[test]
fn chaos_unicode_stress() {
    // Emoji, RTL markers, combining characters, surrogate-safe multi-byte.
    let json = r#"{
        "emoji\ud83c\udf89": "party",
        "rtl\u200f": "test",
        "combining\u0301": "accent",
        "normal": "hello",
        "four_byte": "\ud83d\ude00",
        "empty_val": ""
    }"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    // Canonicalize should handle all these without panic.
    let canon = canonicalize(doc.value());
    assert!(canon.is_ok(), "canonicalize should handle unicode stress");

    // Stats and fingerprint should work.
    let _ = StatsAnalyzer.analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Huge array of identical elements
// -----------------------------------------------------------------------

#[test]
fn chaos_huge_array_identical_elements() {
    // 10,000 identical objects.
    let mut json = String::from("[");
    for i in 0..10_000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(r#"{"id": 1, "name": "same"}"#);
    }
    json.push(']');

    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let stats = StatsAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("stats failed: {e}"));

    // $[*].id should have cardinality 1 and entropy 0.
    let id_path = vajra_types::path::WildcardPath::root()
        .push_array_wildcard()
        .push_key("id");
    if let Some(ps) = stats.paths.get(&id_path) {
        assert_eq!(
            ps.cardinality, 1,
            "expected cardinality 1 for identical values"
        );
        assert!(
            ps.entropy.abs() < 1e-10,
            "expected entropy ~0 for identical values, got {}",
            ps.entropy
        );
    }
}

// -----------------------------------------------------------------------
// Huge array of unique elements
// -----------------------------------------------------------------------

#[test]
fn chaos_huge_array_unique_elements() {
    let mut json = String::from("[");
    for i in 0..10_000 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&i.to_string());
    }
    json.push(']');

    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let stats = StatsAnalyzer
        .analyze(&doc)
        .unwrap_or_else(|e| panic!("stats failed: {e}"));

    let path = vajra_types::path::WildcardPath::root().push_array_wildcard();
    if let Some(ps) = stats.paths.get(&path) {
        assert_eq!(ps.cardinality, 10_000, "expected cardinality 10000");
        assert!(
            ps.entropy > 0.0,
            "expected positive entropy for unique values"
        );
    }
}

// -----------------------------------------------------------------------
// Single enormous string
// -----------------------------------------------------------------------

#[test]
fn chaos_single_enormous_string() {
    // 1 MB string value.
    let big_str: String = "x".repeat(1_000_000);
    let json = format!(r#"{{"big": "{}"}}"#, big_str);
    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert!(doc.metadata().raw_size_bytes > 1_000_000);

    let canon = canonicalize(doc.value());
    assert!(canon.is_ok(), "canonicalize should handle 1MB string");
}

// -----------------------------------------------------------------------
// Numbers at extremes
// -----------------------------------------------------------------------

#[test]
fn chaos_numbers_at_extremes() {
    // serde_json may not parse f64::MAX because the text representation
    // can exceed f64 range on round-trip. Use values known to be safe.
    let json_safe = r#"[1.7976931348623157e308, 5e-324, 0, 1e-308, 1e308, 0.0000000001]"#;
    let result = parse_str(json_safe);
    // If it parses, canonicalization should handle it.
    if let Ok(doc) = result {
        let canon = canonicalize(doc.value());
        assert!(canon.is_ok(), "canonicalize should handle extreme numbers");
    }
    // Also test -0 specifically.
    let neg_zero_json = r#"{"val": -0.0}"#;
    let doc = parse_str(neg_zero_json).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let canon = canonicalize(doc.value()).unwrap_or_else(|e| panic!("canon failed: {e}"));
    let canon_str = String::from_utf8(canon).unwrap_or_default();
    // JCS: -0 should be serialized as 0.
    assert!(
        canon_str.contains(r#""val":0"#),
        "expected -0 to be serialized as 0, got: {canon_str}"
    );
}

// -----------------------------------------------------------------------
// Empty and whitespace-only keys
// -----------------------------------------------------------------------

#[test]
fn chaos_empty_keys() {
    let json = r#"{"": 1, " ": 2, "  ": 3}"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    // Should have 4 paths: $, $."", $." ", $."  " (key-based paths).
    assert!(doc.metadata().distinct_paths >= 3);

    let canon = canonicalize(doc.value());
    assert!(canon.is_ok(), "canonicalize should handle empty keys");

    let _ = StatsAnalyzer.analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Duplicate keys in input
// -----------------------------------------------------------------------

#[test]
fn chaos_duplicate_keys_in_input() {
    // serde_json takes the last value for duplicate keys.
    let json = r#"{"a": 1, "a": 2}"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    // serde_json should resolve to {"a": 2}.
    let val = doc.value().get("a");
    assert_eq!(val, Some(&serde_json::json!(2)));

    let canon = canonicalize(doc.value());
    assert!(canon.is_ok());

    let _ = StatsAnalyzer.analyze(&doc);
    let _ = AnomalyAnalyzer::default().analyze(&doc);
}

// -----------------------------------------------------------------------
// Query on adversarial input
// -----------------------------------------------------------------------

#[test]
fn chaos_query_on_adversarial_input() {
    let adversarial_inputs = vec![
        r#"{"": null}"#,
        r#"[[[[[1]]]]]"#,
        r#"{"a": {"a": {"a": {"a": 1}}}}"#,
        r#"[1, "two", true, null, {"x": 1}]"#,
    ];

    for input in adversarial_inputs {
        let doc = parse_str(input).unwrap_or_else(|e| panic!("parse failed for '{input}': {e}"));
        let stats = StatsAnalyzer
            .analyze(&doc)
            .unwrap_or_else(|e| panic!("stats failed: {e}"));

        let ctx = vajra_query::QueryContext {
            doc: &doc,
            stats: Some(&stats),
        };

        // Run a path query -- should not panic.
        let expr =
            vajra_query::parse_expr("$").unwrap_or_else(|e| panic!("parse expr failed: {e}"));
        let _ = vajra_query::evaluate(&expr, &ctx);

        // Try counting the root.
        let expr = vajra_query::parse_expr("count($)")
            .unwrap_or_else(|e| panic!("parse expr failed: {e}"));
        let _ = vajra_query::evaluate(&expr, &ctx);
    }
}

// -----------------------------------------------------------------------
// Boolean array
// -----------------------------------------------------------------------

#[test]
fn chaos_boolean_array() {
    let json = r#"[true, false, true, false, true]"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let _ = StatsAnalyzer.analyze(&doc);
    let _ = AnomalyAnalyzer::default().analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Nested empty containers
// -----------------------------------------------------------------------

#[test]
fn chaos_nested_empty_containers() {
    let json = r#"{"a": {}, "b": [], "c": {"d": {}, "e": []}}"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let _ = canonicalize(doc.value());
    let _ = StatsAnalyzer.analyze(&doc);
    let _ = AnomalyAnalyzer::default().analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Very long key names
// -----------------------------------------------------------------------

#[test]
fn chaos_very_long_key_names() {
    let long_key = "k".repeat(10_000);
    let json = format!(r#"{{"{long_key}": 42}}"#);
    let doc = parse_str(&json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let canon = canonicalize(doc.value());
    assert!(canon.is_ok(), "canonicalize should handle 10k-char keys");

    let _ = StatsAnalyzer.analyze(&doc);
    let _ = FingerprintAnalyzer.analyze(&doc);
}

// -----------------------------------------------------------------------
// Numeric strings that look like numbers
// -----------------------------------------------------------------------

#[test]
fn chaos_numeric_strings() {
    let json = r#"["123", "45.67", "-89", "1e10", "0x1F", "NaN", "Infinity"]"#;
    let doc = parse_str(json).unwrap_or_else(|e| panic!("parse failed: {e}"));

    let stats = StatsAnalyzer.analyze(&doc);
    assert!(stats.is_ok());

    // These are strings, not numbers, so no numeric stats expected.
    let path = vajra_types::path::WildcardPath::root().push_array_wildcard();
    if let Ok(ref s) = stats {
        if let Some(ps) = s.paths.get(&path) {
            assert!(
                ps.numeric_stats.is_none(),
                "string values should not have numeric stats"
            );
        }
    }
}
