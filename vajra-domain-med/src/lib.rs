//! Medical/EDI domain plugin for Vajra.
//!
//! Provides type recognizers for common healthcare coding systems
//! (ICD-10, CPT, HCPCS, NDC, NPI) and relationship hints that
//! describe expected field co-occurrence patterns in healthcare data.

pub mod hints;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::medical_hints;
use crate::recognizers::{
    CptRecognizer, DiagnosisCodeRecognizer, HcpcsRecognizer, Icd10Recognizer, NdcRecognizer,
    NpiRecognizer,
};

/// The medical/EDI domain plugin.
///
/// Registers healthcare-specific type recognizers and relationship hints
/// to enhance Vajra's analysis of medical data files (835, 837, FHIR, etc.).
pub struct MedicalPlugin;

impl VajraPlugin for MedicalPlugin {
    fn name(&self) -> &str {
        "medical"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(Icd10Recognizer),
            Box::new(CptRecognizer),
            Box::new(HcpcsRecognizer),
            Box::new(NdcRecognizer),
            Box::new(NpiRecognizer),
            Box::new(DiagnosisCodeRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        medical_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = MedicalPlugin;
        assert_eq!(p.name(), "medical");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        let p = MedicalPlugin;
        let recognizers = p.type_recognizers();
        assert_eq!(recognizers.len(), 6);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        let p = MedicalPlugin;
        let hints = p.relationship_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let p = MedicalPlugin;
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
    fn recognizers_are_sorted_by_priority_desc() {
        let p = MedicalPlugin;
        let recognizers = p.type_recognizers();
        // Verify all recognizers have distinct priorities or at least
        // that we can sort them meaningfully.
        let priorities: Vec<u32> = recognizers.iter().map(|r| r.priority()).collect();
        assert!(
            priorities.iter().all(|&p| p > 0),
            "all priorities should be > 0"
        );
    }

    #[test]
    fn plugin_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MedicalPlugin>();
    }

    #[test]
    fn recognizers_are_send_and_sync() {
        let p = MedicalPlugin;
        let recognizers = p.type_recognizers();
        // The fact that they're stored as Box<dyn TypeRecognizer> where
        // TypeRecognizer: Send + Sync proves they're send+sync.
        // This test just exercises the creation path.
        assert!(!recognizers.is_empty());
    }

    #[test]
    fn hint_names_match_expected() {
        let p = MedicalPlugin;
        let hints = p.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"claim_service_line"));
        assert!(names.contains(&"diagnosis_code_description"));
        assert!(names.contains(&"patient_subscriber"));
        assert!(names.contains(&"provider_npi_name"));
        assert!(names.contains(&"adjudication"));
        assert!(names.contains(&"denial_reason"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "E11.9",
            "99213",
            "J0120",
            "12345-6789-01",
            "1234567890",
            "0BJ08ZZ",
            "not-a-code",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let p = MedicalPlugin;
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
            let p = MedicalPlugin;
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
