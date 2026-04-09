use proptest::prelude::*;
use vajra_domain_github::GitHubPlugin;
use vajra_types::traits::VajraPlugin;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn prop_all_recognizers_deterministic(s in ".*") {
        let plugin = GitHubPlugin;
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
        let plugin = GitHubPlugin;
        let recognizers = plugin.type_recognizers();
        for r in &recognizers {
            let _ = r.matches(&s); // must not panic
        }
    }

    #[test]
    fn prop_valid_semver_always_matches(major in 0u32..100, minor in 0u32..100, patch in 0u32..100) {
        let ver = format!("{major}.{minor}.{patch}");
        let plugin = GitHubPlugin;
        let recognizers = plugin.type_recognizers();
        let semver_recognizer = recognizers.iter().find(|r| r.type_name() == "semver");
        if let Some(r) = semver_recognizer {
            prop_assert!(r.matches(&ver), "valid semver should match: {}", ver);
        }
    }

    #[test]
    fn prop_valid_semver_with_v_prefix_always_matches(major in 0u32..100, minor in 0u32..100, patch in 0u32..100) {
        let ver = format!("v{major}.{minor}.{patch}");
        let plugin = GitHubPlugin;
        let recognizers = plugin.type_recognizers();
        let semver_recognizer = recognizers.iter().find(|r| r.type_name() == "semver");
        if let Some(r) = semver_recognizer {
            prop_assert!(r.matches(&ver), "valid v-prefixed semver should match: {}", ver);
        }
    }

    #[test]
    fn prop_valid_pr_number_ref_always_matches(num in 1u32..100000) {
        let pr_ref = format!("#{num}");
        let plugin = GitHubPlugin;
        let recognizers = plugin.type_recognizers();
        let pr_recognizer = recognizers.iter().find(|r| r.type_name() == "pr_number_ref");
        if let Some(r) = pr_recognizer {
            prop_assert!(r.matches(&pr_ref), "valid PR number ref should match: {}", pr_ref);
        }
    }

    #[test]
    fn prop_valid_commit_hash_always_matches(bytes in prop::collection::vec(any::<u8>(), 4..=20)) {
        // Generate 7-40 hex chars from random bytes
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if hex.len() >= 7 && hex.len() <= 40 {
            let plugin = GitHubPlugin;
            let recognizers = plugin.type_recognizers();
            let hash_recognizer = recognizers.iter().find(|r| r.type_name() == "git_commit_hash");
            if let Some(r) = hash_recognizer {
                prop_assert!(r.matches(&hex), "valid commit hash should match: {}", hex);
            }
        }
    }

    #[test]
    fn prop_valid_iso8601_always_matches(
        year in 2000u32..2030,
        month in 1u32..=12,
        day in 1u32..=28,
        hour in 0u32..24,
        minute in 0u32..60,
        second in 0u32..60,
    ) {
        let dt = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z");
        let plugin = GitHubPlugin;
        let recognizers = plugin.type_recognizers();
        let dt_recognizer = recognizers.iter().find(|r| r.type_name() == "iso8601_datetime");
        if let Some(r) = dt_recognizer {
            prop_assert!(r.matches(&dt), "valid ISO 8601 datetime should match: {}", dt);
        }
    }

    #[test]
    fn prop_valid_github_login_always_matches(
        prefix in "[a-zA-Z0-9]",
        middle in "[a-zA-Z0-9-]{0,20}",
        suffix in "[a-zA-Z0-9]",
    ) {
        let login = format!("{prefix}{middle}{suffix}");
        if login.len() <= 39 {
            let plugin = GitHubPlugin;
            let recognizers = plugin.type_recognizers();
            let login_recognizer = recognizers.iter().find(|r| r.type_name() == "github_login");
            if let Some(r) = login_recognizer {
                prop_assert!(r.matches(&login), "valid GitHub login should match: {}", login);
            }
        }
    }
}
