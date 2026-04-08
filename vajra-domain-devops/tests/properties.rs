use proptest::prelude::*;
use vajra_domain_devops::DevOpsPlugin;
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = DevOpsPlugin;
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
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s); // must not panic
        }
    }

    #[test]
    fn prop_valid_semver_always_matches(major in 0u32..100, minor in 0u32..100, patch in 0u32..100) {
        let version = format!("{major}.{minor}.{patch}");
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let semver_recognizer = recognizers.iter().find(|r| r.type_name() == "semver");
        if let Some(r) = semver_recognizer {
            prop_assert!(r.matches(&version), "valid semver should match: {}", version);
        }
    }

    #[test]
    fn prop_valid_semver_with_v_prefix_always_matches(major in 0u32..100, minor in 0u32..100, patch in 0u32..100) {
        let version = format!("v{major}.{minor}.{patch}");
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let semver_recognizer = recognizers.iter().find(|r| r.type_name() == "semver");
        if let Some(r) = semver_recognizer {
            prop_assert!(r.matches(&version), "valid v-semver should match: {}", version);
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
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let cidr_recognizer = recognizers.iter().find(|r| r.type_name() == "cidr_block");
        if let Some(r) = cidr_recognizer {
            prop_assert!(r.matches(&cidr), "valid CIDR should match: {}", cidr);
        }
    }

    #[test]
    fn prop_valid_container_id_short_always_matches(bytes in prop::collection::vec(any::<u8>(), 6..=6)) {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let cid_recognizer = recognizers.iter().find(|r| r.type_name() == "container_id");
        if let Some(r) = cid_recognizer {
            prop_assert!(r.matches(&hex), "valid short container ID should match: {}", hex);
        }
    }

    #[test]
    fn prop_valid_git_sha_full_always_matches(bytes in prop::collection::vec(any::<u8>(), 20..=20)) {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let sha_recognizer = recognizers.iter().find(|r| r.type_name() == "git_sha");
        if let Some(r) = sha_recognizer {
            prop_assert!(r.matches(&hex), "valid full git SHA should match: {}", hex);
        }
    }

    #[test]
    fn prop_valid_arn_always_matches(
        service in "[a-z]{2,10}",
        region in "(us-east-1|us-west-2|eu-west-1|ap-southeast-1)",
        account in "[0-9]{12}",
        resource in "[a-zA-Z0-9/_-]{1,30}",
    ) {
        let arn = format!("arn:aws:{service}:{region}:{account}:{resource}");
        let plugin = DevOpsPlugin;
        let recognizers = plugin.type_recognizers();
        let arn_recognizer = recognizers.iter().find(|r| r.type_name() == "aws_arn");
        if let Some(r) = arn_recognizer {
            prop_assert!(r.matches(&arn), "valid ARN should match: {}", arn);
        }
    }
}
