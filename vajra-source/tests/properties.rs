//! Property tests for vajra-source CST→JSON conversion.

use proptest::prelude::*;
use vajra_source::{parse_source, SourceConfig, SourceLanguage};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    /// Same source bytes → identical Document metadata, every time.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_parse_deterministic(src in "fn [a-z_]{1,20}\\(\\) \\{\\}") {
        let bytes = src.as_bytes();
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        let doc1 = parse_source(bytes, &config);
        let doc2 = parse_source(bytes, &config);
        match (doc1, doc2) {
            (Ok(d1), Ok(d2)) => {
                let j1 = serde_json::to_string(&d1.metadata()).unwrap_or_default();
                let j2 = serde_json::to_string(&d2.metadata()).unwrap_or_default();
                prop_assert_eq!(j1, j2, "determinism violation");
            }
            (Err(_), Err(_)) => {} // both fail = consistent
            _ => prop_assert!(false, "one succeeded, one failed = nondeterministic"),
        }
    }

    /// Parsing never panics on arbitrary UTF-8 strings.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_no_panic_on_arbitrary_utf8(src in ".*") {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        let _ = parse_source(src.as_bytes(), &config);
    }

    /// include_anonymous=true always produces >= nodes than include_anonymous=false.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_anonymous_nodes_increase_count(src in "fn [a-z]{1,10}\\(\\) \\{ let [a-z] = [0-9]+; \\}") {
        let config_named = SourceConfig {
            language: Some(SourceLanguage::Rust),
            include_anonymous: false,
            ..SourceConfig::default()
        };
        let config_all = SourceConfig {
            language: Some(SourceLanguage::Rust),
            include_anonymous: true,
            ..SourceConfig::default()
        };
        let doc_named = parse_source(src.as_bytes(), &config_named);
        let doc_all = parse_source(src.as_bytes(), &config_all);
        if let (Ok(dn), Ok(da)) = (doc_named, doc_all) {
            prop_assert!(
                da.metadata().total_nodes >= dn.metadata().total_nodes,
                "anonymous nodes should only add, not remove"
            );
        }
    }

    /// Valid Rust always parses successfully.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_valid_rust_always_parses(
        name in "[a-z][a-z_]{0,10}",
        body in "(let [a-z] = [0-9]+;)?"
    ) {
        let src = format!("fn {name}() {{ {body} }}");
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        let result = parse_source(src.as_bytes(), &config);
        prop_assert!(result.is_ok(), "valid Rust should parse: {}", src);
    }

    /// Document always has positive total_nodes for non-empty input.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_nonempty_source_has_nodes(src in ".{1,500}") {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        if let Ok(doc) = parse_source(src.as_bytes(), &config) {
            prop_assert!(doc.metadata().total_nodes > 0, "non-empty source should produce nodes");
        }
    }

    /// Python parsing never panics on arbitrary input.
    #[test]
    #[cfg(feature = "python")]
    fn prop_python_no_panic(src in ".*") {
        let config = SourceConfig {
            language: Some(SourceLanguage::Python),
            ..SourceConfig::default()
        };
        let _ = parse_source(src.as_bytes(), &config);
    }

    /// Go parsing never panics on arbitrary input.
    #[test]
    #[cfg(feature = "go")]
    fn prop_go_no_panic(src in ".*") {
        let config = SourceConfig {
            language: Some(SourceLanguage::Go),
            ..SourceConfig::default()
        };
        let _ = parse_source(src.as_bytes(), &config);
    }

    /// JavaScript parsing never panics on arbitrary input.
    #[test]
    #[cfg(feature = "javascript")]
    fn prop_javascript_no_panic(src in ".*") {
        let config = SourceConfig {
            language: Some(SourceLanguage::JavaScript),
            ..SourceConfig::default()
        };
        let _ = parse_source(src.as_bytes(), &config);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Parsing never panics on arbitrary byte input.
    #[test]
    #[cfg(feature = "rust")]
    fn prop_no_panic_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let config = SourceConfig {
            language: Some(SourceLanguage::Rust),
            ..SourceConfig::default()
        };
        // Must not panic — errors are fine
        let _ = parse_source(&bytes, &config);
    }
}
