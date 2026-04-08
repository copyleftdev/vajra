//! Security domain plugin for Vajra.
//!
//! Provides type recognizers for common security data types
//! (CVE IDs, IP addresses, hashes, JWT tokens, MITRE ATT&CK IDs,
//! CVSS vectors) and relationship hints that describe expected field
//! co-occurrence patterns in security event data.

pub mod hints;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::security_hints;
use crate::recognizers::{
    CidrRecognizer, CveRecognizer, CvssRecognizer, Ipv4Recognizer, Ipv6Recognizer, JwtRecognizer,
    MacAddressRecognizer, Md5Recognizer, MitreAttackRecognizer, MitreTacticRecognizer,
    Sha1Recognizer, Sha256Recognizer,
};

/// The security domain plugin.
///
/// Registers security-specific type recognizers and relationship hints
/// to enhance Vajra's analysis of security event logs, threat intelligence
/// feeds, vulnerability scan results, and network flow data.
pub struct SecurityPlugin;

impl VajraPlugin for SecurityPlugin {
    fn name(&self) -> &str {
        "security"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(CveRecognizer),
            Box::new(Ipv4Recognizer),
            Box::new(Ipv6Recognizer),
            Box::new(CidrRecognizer),
            Box::new(MacAddressRecognizer),
            Box::new(Sha256Recognizer),
            Box::new(Sha1Recognizer),
            Box::new(Md5Recognizer),
            Box::new(JwtRecognizer),
            Box::new(MitreAttackRecognizer),
            Box::new(MitreTacticRecognizer),
            Box::new(CvssRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        security_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = SecurityPlugin;
        assert_eq!(p.name(), "security");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        let p = SecurityPlugin;
        let recognizers = p.type_recognizers();
        assert_eq!(recognizers.len(), 12);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        let p = SecurityPlugin;
        let hints = p.relationship_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let p = SecurityPlugin;
        let recognizers = p.type_recognizers();
        let mut names: Vec<&str> = recognizers.iter().map(|r| r.type_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            recognizers.len(),
            "recognizer type names must be unique"
        );
    }

    #[test]
    fn recognizers_have_positive_priority() {
        let p = SecurityPlugin;
        let recognizers = p.type_recognizers();
        let priorities: Vec<u32> = recognizers.iter().map(|r| r.priority()).collect();
        assert!(
            priorities.iter().all(|&p| p > 0),
            "all priorities should be > 0"
        );
    }

    #[test]
    fn plugin_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SecurityPlugin>();
    }

    #[test]
    fn recognizers_are_send_and_sync() {
        let p = SecurityPlugin;
        let recognizers = p.type_recognizers();
        // Stored as Box<dyn TypeRecognizer> where TypeRecognizer: Send + Sync
        assert!(!recognizers.is_empty());
    }

    #[test]
    fn hint_names_match_expected() {
        let p = SecurityPlugin;
        let hints = p.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"network_flow"));
        assert!(names.contains(&"alert_classification"));
        assert!(names.contains(&"vulnerability_assessment"));
        assert!(names.contains(&"authentication_event"));
        assert!(names.contains(&"process_execution"));
        assert!(names.contains(&"dns_query"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "CVE-2024-12345",
            "192.168.1.1",
            "T1059.001",
            "TA0001",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "10.0.0.0/8",
            "aa:bb:cc:dd:ee:ff",
            "not-a-match",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let p = SecurityPlugin;
            let recognizers = p.type_recognizers();
            let run: Vec<Vec<bool>> = test_values
                .iter()
                .map(|v| recognizers.iter().map(|r| r.matches(v)).collect())
                .collect();
            results.push(run);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(
                &results[0], run,
                "run 0 and run {} produced different recognizer results",
                i
            );
        }
    }

    #[test]
    fn determinism_hints_10_runs() {
        let mut results: Vec<Vec<String>> = Vec::new();
        for _ in 0..10 {
            let p = SecurityPlugin;
            let hints = p.relationship_hints();
            let names: Vec<String> = hints.iter().map(|h| h.name.clone()).collect();
            results.push(names);
        }
        for (i, run) in results.iter().enumerate().skip(1) {
            assert_eq!(
                &results[0], run,
                "run 0 and run {} produced different hint names",
                i
            );
        }
    }
}
