//! Source code domain relationship hints.
//!
//! These hints describe expected co-occurrence patterns in the JSON trees
//! produced by vajra-source (tree-sitter CST-to-JSON output).

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all source code domain relationship hints.
pub fn source_hints() -> Vec<RelationshipHint> {
    vec![
        function_definition(),
        class_definition(),
        import_statement(),
        parameter_list(),
        conditional_block(),
        loop_block(),
    ]
}

/// Function definition: name + parameters + body co-occur.
fn function_definition() -> RelationshipHint {
    RelationshipHint {
        name: "function_definition".to_owned(),
        fields: vec![
            "**/function_definition/*/field".to_owned(),
            "**/function_item/*/field".to_owned(),
            "**/method_definition/*/field".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.95,
    }
}

/// Class/struct definition: name + body + optional inheritance.
fn class_definition() -> RelationshipHint {
    RelationshipHint {
        name: "class_definition".to_owned(),
        fields: vec![
            "**/class_definition/*/field".to_owned(),
            "**/struct_item/*/field".to_owned(),
            "**/class_declaration/*/field".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.90,
    }
}

/// Import/use statement: path + optional alias.
fn import_statement() -> RelationshipHint {
    RelationshipHint {
        name: "import_statement".to_owned(),
        fields: vec![
            "**/import_statement/*/kind".to_owned(),
            "**/use_declaration/*/kind".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// Parameter list: type + name pairs.
fn parameter_list() -> RelationshipHint {
    RelationshipHint {
        name: "parameter_list".to_owned(),
        fields: vec![
            "**/parameters/*/kind".to_owned(),
            "**/formal_parameters/*/kind".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.80,
    }
}

/// Conditional block: condition + consequence + optional alternative.
fn conditional_block() -> RelationshipHint {
    RelationshipHint {
        name: "conditional_block".to_owned(),
        fields: vec![
            "**/if_expression/*/field".to_owned(),
            "**/if_statement/*/field".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.75,
    }
}

/// Loop block: condition/iterator + body.
fn loop_block() -> RelationshipHint {
    RelationshipHint {
        name: "loop_block".to_owned(),
        fields: vec![
            "**/for_statement/*/field".to_owned(),
            "**/while_statement/*/field".to_owned(),
            "**/for_expression/*/field".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.75,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hints_returned() {
        assert_eq!(source_hints().len(), 6);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = source_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len());
    }

    #[test]
    fn all_hints_have_valid_weight() {
        for hint in &source_hints() {
            assert!(
                (0.0..=1.0).contains(&hint.weight),
                "hint '{}' weight out of range",
                hint.name
            );
        }
    }

    #[test]
    fn all_hints_have_at_least_two_fields() {
        for hint in &source_hints() {
            assert!(
                hint.fields.len() >= 2,
                "hint '{}' needs at least 2 fields",
                hint.name
            );
        }
    }

    #[test]
    fn all_fields_are_wildcard_patterns() {
        for hint in &source_hints() {
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

    #[test]
    fn function_definition_hint() {
        let h = function_definition();
        assert_eq!(h.name, "function_definition");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
    }

    #[test]
    fn class_definition_hint() {
        let h = class_definition();
        assert_eq!(h.name, "class_definition");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
    }
}
