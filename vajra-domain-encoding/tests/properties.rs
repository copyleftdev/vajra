use proptest::prelude::*;
use vajra_domain_encoding::{detect_encoding_layers, EncodingPlugin};
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = EncodingPlugin;
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
        let plugin = EncodingPlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s);
        }
    }

    #[test]
    fn prop_layers_bounded_by_max_depth(s in ".*", depth in 0u8..=10) {
        let layers = detect_encoding_layers(&s, depth);
        let effective = depth.min(5); // hard cap
        prop_assert!(
            layers.len() as u8 <= effective,
            "layers {} exceeded max_depth {}",
            layers.len(),
            effective
        );
    }

    #[test]
    fn prop_layers_deterministic(s in ".*") {
        let a = detect_encoding_layers(&s, 5);
        let b = detect_encoding_layers(&s, 5);
        let names_a: Vec<&str> = a.iter().map(|l| l.encoding).collect();
        let names_b: Vec<&str> = b.iter().map(|l| l.encoding).collect();
        prop_assert_eq!(names_a, names_b, "layer detection must be deterministic");
    }

    #[test]
    fn prop_url_encoded_detected(
        a in "[a-z]{3,8}",
        b in "[a-z]{3,8}",
    ) {
        let encoded = format!("{a}%20{b}%21%3F");
        let plugin = EncodingPlugin;
        let recognizers = plugin.type_recognizers();
        let url_rec = recognizers.iter().find(|r| r.type_name() == "url_encoded");
        if let Some(r) = url_rec {
            prop_assert!(r.matches(&encoded), "URL-encoded should match: {}", encoded);
        }
    }

    #[test]
    fn prop_pem_detected(label in "[A-Z ]{3,20}") {
        let pem = format!("-----BEGIN {label}-----\ndata\n-----END {label}-----");
        let plugin = EncodingPlugin;
        let recognizers = plugin.type_recognizers();
        let pem_rec = recognizers.iter().find(|r| r.type_name() == "pem_block");
        if let Some(r) = pem_rec {
            prop_assert!(r.matches(&pem), "PEM should match: {}", pem);
        }
    }

    #[test]
    fn prop_html_entities_detected(
        entity1 in prop::sample::select(vec!["&amp;", "&lt;", "&gt;", "&quot;", "&#x41;", "&#65;"]),
        entity2 in prop::sample::select(vec!["&amp;", "&lt;", "&gt;", "&quot;", "&#x42;", "&#66;"]),
    ) {
        let value = format!("text {entity1} more {entity2} end");
        let plugin = EncodingPlugin;
        let recognizers = plugin.type_recognizers();
        let html_rec = recognizers.iter().find(|r| r.type_name() == "html_entity");
        if let Some(r) = html_rec {
            prop_assert!(r.matches(&value), "HTML entities should match: {}", value);
        }
    }
}
