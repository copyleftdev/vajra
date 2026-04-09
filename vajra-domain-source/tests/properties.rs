use proptest::prelude::*;
use vajra_domain_source::SourcePlugin;
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let a = r.matches(&s);
            let b = r.matches(&s);
            prop_assert_eq!(a, b, "{} must be deterministic", r.type_name());
        }
    }

    #[test]
    fn prop_no_recognizer_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..500)) {
        let s = String::from_utf8_lossy(&bytes);
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s);
        }
    }

    #[test]
    fn prop_valid_snake_case_always_matches(
        a in "[a-z][a-z0-9]{0,8}",
        b in "[a-z][a-z0-9]{0,8}",
    ) {
        let ident = format!("{a}_{b}");
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        let rec = recognizers.iter().find(|r| r.type_name() == "snake_case_identifier");
        if let Some(r) = rec {
            prop_assert!(r.matches(&ident), "valid snake_case should match: {}", ident);
        }
    }

    #[test]
    fn prop_valid_screaming_snake_always_matches(
        a in "[A-Z][A-Z0-9]{0,8}",
        b in "[A-Z][A-Z0-9]{0,8}",
    ) {
        let ident = format!("{a}_{b}");
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        let rec = recognizers.iter().find(|r| r.type_name() == "screaming_snake_case");
        if let Some(r) = rec {
            prop_assert!(r.matches(&ident), "valid SCREAMING_SNAKE should match: {}", ident);
        }
    }

    #[test]
    fn prop_valid_pascal_case_always_matches(name in "[A-Z][a-zA-Z0-9]{1,15}") {
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        let rec = recognizers.iter().find(|r| r.type_name() == "pascal_case_identifier");
        if let Some(r) = rec {
            prop_assert!(r.matches(&name), "valid PascalCase should match: {}", name);
        }
    }

    #[test]
    fn prop_source_file_path_always_matches(
        dir in "[a-z]{1,5}",
        file in "[a-z]{1,10}",
        ext in prop::sample::select(vec!["rs", "py", "js", "go", "java", "c", "cpp", "rb"]),
    ) {
        let path = format!("{dir}/{file}.{ext}");
        let plugin = SourcePlugin;
        let recognizers = plugin.type_recognizers();
        let rec = recognizers.iter().find(|r| r.type_name() == "source_file_path");
        if let Some(r) = rec {
            prop_assert!(r.matches(&path), "valid source file path should match: {}", path);
        }
    }
}
