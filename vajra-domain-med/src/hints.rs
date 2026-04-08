//! Medical domain relationship hints.
//!
//! These hints describe expected co-occurrence and dependency patterns
//! in healthcare/EDI data, allowing Vajra's analysis engine to better
//! understand domain-specific field relationships.

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all medical domain relationship hints.
pub fn medical_hints() -> Vec<RelationshipHint> {
    vec![
        claim_service_line(),
        diagnosis_code_description(),
        patient_subscriber(),
        provider_npi_name(),
        adjudication(),
        denial_reason(),
    ]
}

/// Claim service line: procedure code, service date, and charge amount
/// tend to co-occur on the same service line.
fn claim_service_line() -> RelationshipHint {
    RelationshipHint {
        name: "claim_service_line".to_owned(),
        fields: vec![
            "**/procedure_code".to_owned(),
            "**/service_date".to_owned(),
            "**/charge_amount".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.9,
    }
}

/// A diagnosis code is typically paired with a human-readable description.
fn diagnosis_code_description() -> RelationshipHint {
    RelationshipHint {
        name: "diagnosis_code_description".to_owned(),
        fields: vec![
            "**/diagnosis_code".to_owned(),
            "**/diagnosis_description".to_owned(),
        ],
        relationship: RelationshipType::CodeDescriptionPair,
        weight: 0.85,
    }
}

/// Patient and subscriber demographic fields co-occur together.
fn patient_subscriber() -> RelationshipHint {
    RelationshipHint {
        name: "patient_subscriber".to_owned(),
        fields: vec![
            "**/patient_*".to_owned(),
            "**/subscriber_*".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.8,
    }
}

/// Provider NPI and provider name co-occur.
fn provider_npi_name() -> RelationshipHint {
    RelationshipHint {
        name: "provider_npi_name".to_owned(),
        fields: vec![
            "**/provider_npi".to_owned(),
            "**/provider_name".to_owned(),
        ],
        relationship: RelationshipType::FunctionalDependency,
        weight: 0.9,
    }
}

/// Adjudication fields: allowed amount, paid amount, and adjustment
/// co-occur within an adjudication block.
fn adjudication() -> RelationshipHint {
    RelationshipHint {
        name: "adjudication".to_owned(),
        fields: vec![
            "**/allowed_amount".to_owned(),
            "**/paid_amount".to_owned(),
            "**/adjustment_amount".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// When claim status is "denied", a denial reason should be present.
fn denial_reason() -> RelationshipHint {
    RelationshipHint {
        name: "denial_reason".to_owned(),
        fields: vec![
            "**/claim_status".to_owned(),
            "**/denial_reason".to_owned(),
        ],
        relationship: RelationshipType::ConditionalPresence,
        weight: 0.75,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hints_returned() {
        let hints = medical_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = medical_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len(), "hint names must be unique");
    }

    #[test]
    fn all_hints_have_fields() {
        let hints = medical_hints();
        for hint in &hints {
            assert!(
                hint.fields.len() >= 2,
                "hint '{}' must have at least 2 fields",
                hint.name
            );
        }
    }

    #[test]
    fn all_hints_have_valid_weight() {
        let hints = medical_hints();
        for hint in &hints {
            assert!(
                (0.0..=1.0).contains(&hint.weight),
                "hint '{}' weight {} must be in [0.0, 1.0]",
                hint.name,
                hint.weight
            );
        }
    }

    #[test]
    fn claim_service_line_hint() {
        let h = claim_service_line();
        assert_eq!(h.name, "claim_service_line");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
        assert!(h.fields.iter().any(|f| f.contains("procedure_code")));
        assert!(h.fields.iter().any(|f| f.contains("service_date")));
        assert!(h.fields.iter().any(|f| f.contains("charge_amount")));
    }

    #[test]
    fn diagnosis_code_description_hint() {
        let h = diagnosis_code_description();
        assert_eq!(h.name, "diagnosis_code_description");
        assert_eq!(h.relationship, RelationshipType::CodeDescriptionPair);
        assert_eq!(h.fields.len(), 2);
    }

    #[test]
    fn patient_subscriber_hint() {
        let h = patient_subscriber();
        assert_eq!(h.name, "patient_subscriber");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 2);
    }

    #[test]
    fn provider_npi_name_hint() {
        let h = provider_npi_name();
        assert_eq!(h.name, "provider_npi_name");
        assert_eq!(h.relationship, RelationshipType::FunctionalDependency);
        assert_eq!(h.fields.len(), 2);
    }

    #[test]
    fn adjudication_hint() {
        let h = adjudication();
        assert_eq!(h.name, "adjudication");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn denial_reason_hint() {
        let h = denial_reason();
        assert_eq!(h.name, "denial_reason");
        assert_eq!(h.relationship, RelationshipType::ConditionalPresence);
        assert_eq!(h.fields.len(), 2);
    }

    #[test]
    fn all_fields_are_wildcard_patterns() {
        let hints = medical_hints();
        for hint in &hints {
            for field in &hint.fields {
                assert!(
                    field.contains('*'),
                    "field '{}' in hint '{}' should be a wildcard pattern",
                    field,
                    hint.name
                );
            }
        }
    }
}
