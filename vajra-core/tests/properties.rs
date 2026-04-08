#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::Value;

/// Generate arbitrary JSON values with bounded depth and size.
///
/// Floats are restricted to integers cast to f64 to avoid floating-point
/// precision issues during round-trip canonicalization (the JCS float
/// formatter and serde_json may disagree on the shortest representation
/// of a float after re-parsing).
fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-1000000i64..1000000i64).prop_map(|n| serde_json::json!(n)),
        "[a-zA-Z0-9_ ]{0,50}".prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(6, 128, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8)
                .prop_map(serde_json::Value::Array),
            prop::collection::btree_map("[a-z]{1,6}", inner, 0..6)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

// ---------------------------------------------------------------------------
// 1. Canonicalization idempotence: canon(parse(canon(x))) == canon(x)
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_canonicalization_idempotence(value in arb_json_value()) {
        let first = vajra_core::canonical::canonicalize(&value);
        // Skip values that can't be canonicalized (e.g., NaN/Inf floats
        // which shouldn't be generated but guard anyway).
        if let Ok(first_bytes) = first {
            let first_str = String::from_utf8(first_bytes.clone()).unwrap();
            let reparsed: serde_json::Value = serde_json::from_str(&first_str).unwrap();
            let second = vajra_core::canonical::canonicalize(&reparsed).unwrap();
            prop_assert_eq!(first_bytes, second,
                "canon(parse(canon(x))) != canon(x) for input: {}",
                serde_json::to_string(&value).unwrap_or_default()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Canonicalization key-order invariance: shuffling object keys doesn't
//    change canonical output.
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_canonicalization_key_order_invariance(
        pairs in prop::collection::btree_map(
            "[a-z]{1,6}",
            prop_oneof![
                Just(serde_json::Value::Null),
                any::<bool>().prop_map(serde_json::Value::Bool),
                (-1000i64..1000i64).prop_map(|n| serde_json::json!(n)),
                "[a-z]{0,10}".prop_map(serde_json::Value::String),
            ],
            2..8
        )
    ) {
        // Build two objects with the same key-value pairs in different insertion orders.
        // BTreeMap gives us unique keys with deterministic values.
        let items: Vec<(String, serde_json::Value)> = pairs.into_iter().collect();

        let mut map_forward = serde_json::Map::new();
        for (k, v) in items.iter() {
            map_forward.insert(k.clone(), v.clone());
        }

        let mut map_reverse = serde_json::Map::new();
        for (k, v) in items.iter().rev() {
            map_reverse.insert(k.clone(), v.clone());
        }

        let val_a = serde_json::Value::Object(map_forward);
        let val_b = serde_json::Value::Object(map_reverse);
        let canon_a = vajra_core::canonical::canonicalize(&val_a).unwrap();
        let canon_b = vajra_core::canonical::canonicalize(&val_b).unwrap();
        prop_assert_eq!(canon_a, canon_b,
            "canonical output should not depend on key insertion order"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Parse round-trip: parse_str(json) succeeds for any valid JSON, and the
//    path trie is non-empty for non-scalar inputs.
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_parse_roundtrip(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = vajra_core::parse::parse_str(&json_str).unwrap();
        // The trie always has at least the root path.
        prop_assert!(doc.trie().path_count() >= 1,
            "trie should have at least the root path");
        // For non-scalar inputs (objects/arrays with children), trie should have more paths.
        let is_container = matches!(&value, Value::Array(a) if !a.is_empty())
            || matches!(&value, Value::Object(m) if !m.is_empty());
        if is_container {
            prop_assert!(doc.trie().path_count() > 1,
                "non-empty container should produce more than 1 path in trie");
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Path trie completeness: every node in the JSON tree has a corresponding
//    path in the trie.
// ---------------------------------------------------------------------------

/// Count total nodes in a JSON value tree.
fn count_nodes(value: &Value) -> u64 {
    match value {
        Value::Object(map) => 1 + map.values().map(count_nodes).sum::<u64>(),
        Value::Array(arr) => 1 + arr.iter().map(count_nodes).sum::<u64>(),
        _ => 1,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_path_trie_completeness(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = vajra_core::parse::parse_str(&json_str).unwrap();
        let expected_total = count_nodes(&value);
        prop_assert_eq!(doc.metadata().total_nodes, expected_total,
            "total_nodes should match the count of JSON tree nodes");
    }
}

// ---------------------------------------------------------------------------
// 5. Metadata consistency: total_nodes >= distinct_paths, max_depth >= 0
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_metadata_consistency(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = vajra_core::parse::parse_str(&json_str).unwrap();
        let meta = doc.metadata();
        prop_assert!(meta.total_nodes >= u64::from(meta.distinct_paths),
            "total_nodes ({}) should be >= distinct_paths ({})",
            meta.total_nodes, meta.distinct_paths);
        // max_depth is u32, always >= 0, but let's also verify it's sensible
        // (it should be at most total_nodes - 1 for a linear chain).
        prop_assert!(u64::from(meta.max_depth) < meta.total_nodes || meta.total_nodes == 1,
            "max_depth ({}) should be < total_nodes ({}) unless single node",
            meta.max_depth, meta.total_nodes);
    }
}

// ---------------------------------------------------------------------------
// 6. Depth invariant: no path in the trie has depth > max_depth.
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_depth_invariant(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = vajra_core::parse::parse_str(&json_str).unwrap();
        let max_depth = doc.metadata().max_depth;
        let all_paths = doc.trie().all_paths();
        for path in &all_paths {
            prop_assert!(path.depth() <= max_depth,
                "path {} has depth {} but max_depth is {}",
                path, path.depth(), max_depth);
        }
    }
}

// ---------------------------------------------------------------------------
// Extended CI: idempotence with many more cases.
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    #[ignore]
    fn prop_canonicalization_idempotence_extended(value in arb_json_value()) {
        let first = vajra_core::canonical::canonicalize(&value);
        if let Ok(first_bytes) = first {
            let first_str = String::from_utf8(first_bytes.clone()).unwrap();
            let reparsed: serde_json::Value = serde_json::from_str(&first_str).unwrap();
            let second = vajra_core::canonical::canonicalize(&reparsed).unwrap();
            prop_assert_eq!(first_bytes, second);
        }
    }
}
