use proptest::prelude::*;
use vajra_domain_med::MedicalPlugin;
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = MedicalPlugin;
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
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s); // must not panic
        }
    }

    #[test]
    fn prop_valid_icd10_always_matches(
        letter in "[A-Z]",
        d1 in 0u8..=9,
        d2 in 0u8..=9,
    ) {
        let code = format!("{}{}{}", letter, d1, d2);
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let icd10 = recognizers.iter().find(|r| r.type_name() == "icd10");
        if let Some(r) = icd10 {
            prop_assert!(r.matches(&code), "valid ICD-10 stem should match: {}", code);
        }
    }

    #[test]
    fn prop_valid_icd10_with_decimal_always_matches(
        letter in "[A-Z]",
        d1 in 0u8..=9,
        d2 in 0u8..=9,
        suffix in "[A-Za-z0-9]{1,4}",
    ) {
        let code = format!("{}{}{}.{}", letter, d1, d2, suffix);
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let icd10 = recognizers.iter().find(|r| r.type_name() == "icd10");
        if let Some(r) = icd10 {
            prop_assert!(r.matches(&code), "valid ICD-10 with decimal should match: {}", code);
        }
    }

    #[test]
    fn prop_valid_cpt_always_matches(code in "[0-9]{5}") {
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let cpt = recognizers.iter().find(|r| r.type_name() == "cpt");
        if let Some(r) = cpt {
            prop_assert!(r.matches(&code), "valid CPT code should match: {}", code);
        }
    }

    #[test]
    fn prop_valid_npi_always_matches(
        start in 1u8..=2,
        rest in "[0-9]{9}",
    ) {
        let npi = format!("{}{}", start, rest);
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let npi_rec = recognizers.iter().find(|r| r.type_name() == "npi");
        if let Some(r) = npi_rec {
            prop_assert!(r.matches(&npi), "valid NPI should match: {}", npi);
        }
    }

    #[test]
    fn prop_valid_hcpcs_always_matches(
        letter in "[A-Z]",
        digits in "[0-9]{4}",
    ) {
        let code = format!("{}{}", letter, digits);
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let hcpcs = recognizers.iter().find(|r| r.type_name() == "hcpcs");
        if let Some(r) = hcpcs {
            prop_assert!(r.matches(&code), "valid HCPCS code should match: {}", code);
        }
    }

    #[test]
    fn prop_valid_ndc_11digit_always_matches(digits in "[0-9]{11}") {
        let plugin = MedicalPlugin;
        let recognizers = plugin.type_recognizers();
        let ndc = recognizers.iter().find(|r| r.type_name() == "ndc");
        if let Some(r) = ndc {
            prop_assert!(r.matches(&digits), "valid 11-digit NDC should match: {}", digits);
        }
    }
}
