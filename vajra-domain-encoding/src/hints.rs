//! Encoding domain relationship hints.
//!
//! Co-occurrence patterns for fields that commonly contain encoded data.

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all encoding domain relationship hints.
pub fn encoding_hints() -> Vec<RelationshipHint> {
    vec![
        content_with_encoding(),
        transfer_encoding(),
        encoded_decoded_pair(),
    ]
}

/// Content-type field tends to co-occur with encoded body.
fn content_with_encoding() -> RelationshipHint {
    RelationshipHint {
        name: "content_with_encoding".to_owned(),
        fields: vec![
            "**/content_type".to_owned(),
            "**/content".to_owned(),
            "**/body".to_owned(),
        ],
        relationship: RelationshipType::ConditionalPresence,
        weight: 0.80,
    }
}

/// Transfer-encoding + payload.
fn transfer_encoding() -> RelationshipHint {
    RelationshipHint {
        name: "transfer_encoding".to_owned(),
        fields: vec![
            "**/transfer_encoding".to_owned(),
            "**/encoding".to_owned(),
            "**/data".to_owned(),
            "**/payload".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.75,
    }
}

/// Base64 fields often have a corresponding decoded/plain field.
fn encoded_decoded_pair() -> RelationshipHint {
    RelationshipHint {
        name: "encoded_decoded_pair".to_owned(),
        fields: vec![
            "**/*_encoded".to_owned(),
            "**/*_decoded".to_owned(),
            "**/*_base64".to_owned(),
            "**/*_raw".to_owned(),
        ],
        relationship: RelationshipType::CodeDescriptionPair,
        weight: 0.70,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hints_returned() {
        assert_eq!(encoding_hints().len(), 3);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = encoding_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len());
    }

    #[test]
    fn all_hints_have_valid_weight() {
        for hint in &encoding_hints() {
            assert!(
                (0.0..=1.0).contains(&hint.weight),
                "hint '{}' weight out of range",
                hint.name
            );
        }
    }

    #[test]
    fn all_hints_have_at_least_two_fields() {
        for hint in &encoding_hints() {
            assert!(
                hint.fields.len() >= 2,
                "hint '{}' needs at least 2 fields",
                hint.name
            );
        }
    }

    #[test]
    fn all_fields_are_wildcard_patterns() {
        for hint in &encoding_hints() {
            for field in &hint.fields {
                assert!(
                    field.contains('*'),
                    "field '{}' in '{}' should use wildcards",
                    field,
                    hint.name
                );
            }
        }
    }
}
