#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::json;
use vajra_core::parse::parse_str;
use vajra_fingerprint::merkle::{merkle_fingerprint, merkle_with_motifs};
use vajra_fingerprint::ncd::ncd;
use vajra_fingerprint::path_set::path_set_fingerprint;
use vajra_fingerprint::typed_path::typed_path_fingerprint;

/// Generate arbitrary JSON values with bounded depth and size.
fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-1000000i64..1000000i64).prop_map(|n| json!(n)),
        (0.001f64..1000000.0f64).prop_map(|f| json!(f)),
        "[a-zA-Z0-9_ ]{0,50}".prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(6, 128, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(serde_json::Value::Array),
            prop::collection::btree_map("[a-z]{1,6}", inner, 0..6)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Generate an arbitrary JSON object (not scalar).
fn arb_json_object() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-1000i64..1000i64).prop_map(|n| json!(n)),
        "[a-z]{0,10}".prop_map(serde_json::Value::String),
    ];
    prop::collection::btree_map("[a-z]{1,6}", leaf, 1..8)
        .prop_map(|m| serde_json::Value::Object(m.into_iter().collect()))
}

// =========================================================================
// 1. Path set fingerprint determinism: same document -> same hash every time
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_path_set_fingerprint_determinism(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = parse_str(&json_str).unwrap();
        let h1 = path_set_fingerprint(&doc);
        let h2 = path_set_fingerprint(&doc);
        prop_assert_eq!(h1, h2, "same document should always produce the same hash");
    }
}

// =========================================================================
// 2. Path set key-order invariance: shuffling keys -> same fingerprint
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_path_set_key_order_invariance(
        keys in prop::collection::vec("[a-z]{1,6}", 2..8),
        values in prop::collection::vec(
            prop_oneof![
                Just(serde_json::Value::Null),
                any::<bool>().prop_map(serde_json::Value::Bool),
                (-100i64..100i64).prop_map(|n| json!(n)),
            ],
            2..8,
        ),
    ) {
        let len = keys.len().min(values.len());
        if len < 2 {
            return Ok(());
        }

        // Build JSON string with keys in forward order
        let mut pairs_fwd = Vec::new();
        for i in 0..len {
            pairs_fwd.push(format!(
                "\"{}\":{}",
                keys[i],
                serde_json::to_string(&values[i]).unwrap()
            ));
        }
        let json_a = format!("{{{}}}", pairs_fwd.join(","));

        // Build JSON string with keys in reverse order
        pairs_fwd.reverse();
        let json_b = format!("{{{}}}", pairs_fwd.join(","));

        let doc_a = parse_str(&json_a).unwrap();
        let doc_b = parse_str(&json_b).unwrap();
        prop_assert_eq!(
            path_set_fingerprint(&doc_a),
            path_set_fingerprint(&doc_b),
            "key order should not affect path set fingerprint"
        );
    }
}

// =========================================================================
// 3. Typed path changes with type: changing a value's type changes the
//    typed path fingerprint.
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_typed_path_changes_with_type(key in "[a-z]{1,6}") {
        // Same key, integer value vs. string value
        let doc_int = parse_str(
            &format!("{{\"{key}\": 42}}")
        ).unwrap();
        let doc_str = parse_str(
            &format!("{{\"{key}\": \"hello\"}}")
        ).unwrap();
        prop_assert_ne!(
            typed_path_fingerprint(&doc_int),
            typed_path_fingerprint(&doc_str),
            "changing value type should change typed path fingerprint"
        );
    }
}

// =========================================================================
// 4. Merkle structural identity: two structurally identical subtrees
//    (same types, same keys, different values) -> same merkle hash.
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_merkle_structural_identity(
        keys in prop::collection::vec("[a-z]{1,6}", 1..6),
    ) {
        // Build two objects with the same keys and same types but different int values.
        let mut obj_a = serde_json::Map::new();
        let mut obj_b = serde_json::Map::new();
        for (i, key) in keys.iter().enumerate() {
            obj_a.insert(key.clone(), json!(i as i64));
            obj_b.insert(key.clone(), json!((i as i64) + 1000));
        }
        let val_a = serde_json::Value::Object(obj_a);
        let val_b = serde_json::Value::Object(obj_b);
        prop_assert_eq!(
            merkle_fingerprint(&val_a),
            merkle_fingerprint(&val_b),
            "same structure (types+keys) with different values should produce same merkle hash"
        );
    }
}

// =========================================================================
// 5. Merkle different structures differ: adding a key changes the merkle
//    hash (with overwhelming probability).
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_merkle_different_structures_differ(obj in arb_json_object()) {
        // Add an extra key that definitely doesn't exist.
        if let serde_json::Value::Object(map) = &obj {
            let extra_key = "zzzzzz_extra_unique_key";
            if !map.contains_key(extra_key) {
                let mut extended = map.clone();
                extended.insert(extra_key.to_string(), json!(1));
                let val_extended = serde_json::Value::Object(extended);
                prop_assert_ne!(
                    merkle_fingerprint(&obj),
                    merkle_fingerprint(&val_extended),
                    "adding a key should change the merkle hash"
                );
            }
        }
    }
}

// =========================================================================
// 6. Motif counting: repeated array elements produce motif count >= 2.
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn prop_motif_counting(
        keys in prop::collection::vec("[a-z]{1,4}", 1..5),
        n in 2usize..10,
    ) {
        // Build an array of n identical-structure objects.
        let mut elements = Vec::new();
        for i in 0..n {
            let mut obj = serde_json::Map::new();
            for key in &keys {
                obj.insert(key.clone(), json!(i as i64));
            }
            elements.push(serde_json::Value::Object(obj));
        }
        let array_val = serde_json::Value::Array(elements);
        let (_root_hash, freqs) = merkle_with_motifs(&array_val);

        // The element objects all have the same structural hash.
        // Build a reference element to get the expected hash.
        let mut ref_obj = serde_json::Map::new();
        for key in &keys {
            ref_obj.insert(key.clone(), json!(0));
        }
        let ref_val = serde_json::Value::Object(ref_obj);
        let element_hash = merkle_fingerprint(&ref_val);

        let count = freqs.get(&element_hash).copied().unwrap_or(0);
        prop_assert!(count >= n as u64,
            "identical-structure array elements should have motif count >= {n}, got {count}");
    }
}

// =========================================================================
// Extended CI tests
// =========================================================================
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100_000))]

    #[test]
    #[ignore]
    fn prop_path_set_fingerprint_determinism_extended(value in arb_json_value()) {
        let json_str = serde_json::to_string(&value).unwrap();
        let doc = parse_str(&json_str).unwrap();
        let h1 = path_set_fingerprint(&doc);
        let h2 = path_set_fingerprint(&doc);
        prop_assert_eq!(h1, h2);
    }
}

// =========================================================================
// NCD properties
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // N1. NCD symmetric: NCD(a,b) ≈ NCD(b,a)
    #[test]
    fn prop_ncd_symmetric(
        a in prop::collection::vec(0u8..255, 20..200),
        b in prop::collection::vec(0u8..255, 20..200),
    ) {
        let d_ab = ncd(&a, &b);
        let d_ba = ncd(&b, &a);
        prop_assert!((d_ab - d_ba).abs() < 1e-10,
            "NCD not symmetric: NCD(a,b)={d_ab}, NCD(b,a)={d_ba}");
    }

    // N2. NCD non-negative
    #[test]
    fn prop_ncd_non_negative(
        a in prop::collection::vec(0u8..255, 1..200),
        b in prop::collection::vec(0u8..255, 1..200),
    ) {
        let d = ncd(&a, &b);
        prop_assert!(d >= 0.0, "NCD must be >= 0, got {d}");
    }

    // N3. NCD self-similarity low: NCD(x,x) should be near 0
    #[test]
    fn prop_ncd_self_low(data in prop::collection::vec(0u8..255, 50..500)) {
        let d = ncd(&data, &data);
        prop_assert!(d < 0.3,
            "NCD(x,x) should be near 0 for sufficient data, got {d}");
    }

    // N4. NCD bounded: NCD ∈ [0, ~1.1] (can slightly exceed 1 due to compressor overhead)
    #[test]
    fn prop_ncd_bounded(
        a in prop::collection::vec(0u8..255, 10..200),
        b in prop::collection::vec(0u8..255, 10..200),
    ) {
        let d = ncd(&a, &b);
        prop_assert!(d < 2.0,
            "NCD should be bounded, got {d}");
    }

    // N5. NCD finite
    #[test]
    fn prop_ncd_finite(
        a in prop::collection::vec(0u8..255, 0..200),
        b in prop::collection::vec(0u8..255, 0..200),
    ) {
        let d = ncd(&a, &b);
        prop_assert!(d.is_finite(), "NCD must be finite, got {d}");
    }
}
