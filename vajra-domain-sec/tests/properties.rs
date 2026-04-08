use proptest::prelude::*;
use vajra_domain_sec::SecurityPlugin;
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = SecurityPlugin;
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
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s); // must not panic
        }
    }

    #[test]
    fn prop_cve_format_always_matches(year in 1999u32..2100, id in 1000u32..100000) {
        let cve = format!("CVE-{year}-{id}");
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let cve_recognizer = recognizers.iter().find(|r| r.type_name() == "cve_id");
        if let Some(r) = cve_recognizer {
            prop_assert!(r.matches(&cve), "valid CVE format should match: {}", cve);
        }
    }

    #[test]
    fn prop_valid_ipv4_always_matches(
        a in 0u8..=255,
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let ip = format!("{a}.{b}.{c}.{d}");
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let ipv4_recognizer = recognizers.iter().find(|r| r.type_name() == "ipv4");
        if let Some(r) = ipv4_recognizer {
            prop_assert!(r.matches(&ip), "valid IPv4 should match: {}", ip);
        }
    }

    #[test]
    fn prop_valid_mitre_technique_always_matches(id in 1000u32..9999) {
        let technique = format!("T{id}");
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let mitre_recognizer = recognizers.iter().find(|r| r.type_name() == "mitre_attack_id");
        if let Some(r) = mitre_recognizer {
            prop_assert!(r.matches(&technique), "valid MITRE technique should match: {}", technique);
        }
    }

    #[test]
    fn prop_valid_mitre_tactic_always_matches(id in 1u32..9999) {
        let tactic = format!("TA{id:04}");
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let tactic_recognizer = recognizers.iter().find(|r| r.type_name() == "mitre_tactic");
        if let Some(r) = tactic_recognizer {
            prop_assert!(r.matches(&tactic), "valid MITRE tactic should match: {}", tactic);
        }
    }

    #[test]
    fn prop_valid_sha256_always_matches(bytes in prop::collection::vec(any::<u8>(), 32..=32)) {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let sha_recognizer = recognizers.iter().find(|r| r.type_name() == "sha256_hash");
        if let Some(r) = sha_recognizer {
            prop_assert!(r.matches(&hex), "valid SHA-256 hex should match: {}", hex);
        }
    }

    #[test]
    fn prop_valid_md5_always_matches(bytes in prop::collection::vec(any::<u8>(), 16..=16)) {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let md5_recognizer = recognizers.iter().find(|r| r.type_name() == "md5_hash");
        if let Some(r) = md5_recognizer {
            prop_assert!(r.matches(&hex), "valid MD5 hex should match: {}", hex);
        }
    }

    #[test]
    fn prop_valid_cidr_always_matches(
        a in 0u8..=255,
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
        prefix in 0u8..=32,
    ) {
        let cidr = format!("{a}.{b}.{c}.{d}/{prefix}");
        let plugin = SecurityPlugin;
        let recognizers = plugin.type_recognizers();
        let cidr_recognizer = recognizers.iter().find(|r| r.type_name() == "cidr");
        if let Some(r) = cidr_recognizer {
            prop_assert!(r.matches(&cidr), "valid CIDR should match: {}", cidr);
        }
    }
}
