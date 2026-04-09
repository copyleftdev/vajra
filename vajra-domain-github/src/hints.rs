//! GitHub domain relationship hints.
//!
//! These hints describe expected co-occurrence and dependency patterns
//! in GitHub event data, allowing Vajra's analysis engine to better
//! understand domain-specific field relationships.

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all GitHub domain relationship hints.
pub fn github_hints() -> Vec<RelationshipHint> {
    vec![
        review_network(),
        merge_gate(),
        issue_triage(),
        contributor_lifecycle(),
        release_cadence(),
        pr_review_assignment(),
        commit_cascade(),
    ]
}

/// Review network: reviewer + PR author + review state co-occur.
fn review_network() -> RelationshipHint {
    RelationshipHint {
        name: "review_network".to_owned(),
        fields: vec![
            "**/reviewer".to_owned(),
            "**/pr_author".to_owned(),
            "**/review_state".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.90,
    }
}

/// Merge gate: author + state + merged flag + additions co-occur.
fn merge_gate() -> RelationshipHint {
    RelationshipHint {
        name: "merge_gate".to_owned(),
        fields: vec![
            "**/author".to_owned(),
            "**/state".to_owned(),
            "**/merged".to_owned(),
            "**/additions".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// Issue triage: author + labels + state + comment count co-occur.
fn issue_triage() -> RelationshipHint {
    RelationshipHint {
        name: "issue_triage".to_owned(),
        fields: vec![
            "**/author".to_owned(),
            "**/labels".to_owned(),
            "**/state".to_owned(),
            "**/comment_count".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// Contributor lifecycle: author + creation date + commits form a functional dependency.
fn contributor_lifecycle() -> RelationshipHint {
    RelationshipHint {
        name: "contributor_lifecycle".to_owned(),
        fields: vec![
            "**/author".to_owned(),
            "**/created".to_owned(),
            "**/commits".to_owned(),
        ],
        relationship: RelationshipType::FunctionalDependency,
        weight: 0.80,
    }
}

/// Release cadence: tag + creation date + pre-release flag co-occur.
fn release_cadence() -> RelationshipHint {
    RelationshipHint {
        name: "release_cadence".to_owned(),
        fields: vec![
            "**/tag".to_owned(),
            "**/created".to_owned(),
            "**/isPrerelease".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.75,
    }
}

/// PR review assignment: PR author + reviewer + approval rate form a functional dependency.
fn pr_review_assignment() -> RelationshipHint {
    RelationshipHint {
        name: "pr_review_assignment".to_owned(),
        fields: vec![
            "**/pr_author".to_owned(),
            "**/reviewer".to_owned(),
            "**/approval_rate".to_owned(),
        ],
        relationship: RelationshipType::FunctionalDependency,
        weight: 0.85,
    }
}

/// Commit cascade: trigger author + response author + same-author flag + file co-occur.
fn commit_cascade() -> RelationshipHint {
    RelationshipHint {
        name: "commit_cascade".to_owned(),
        fields: vec![
            "**/trigger_author".to_owned(),
            "**/response_author".to_owned(),
            "**/same_author".to_owned(),
            "**/file".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hints_returned() {
        let hints = github_hints();
        assert_eq!(hints.len(), 7);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = github_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len(), "hint names must be unique");
    }

    #[test]
    fn all_hints_have_fields() {
        let hints = github_hints();
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
        let hints = github_hints();
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
    fn all_fields_are_wildcard_patterns() {
        let hints = github_hints();
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

    #[test]
    fn review_network_hint() {
        let h = review_network();
        assert_eq!(h.name, "review_network");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
        assert!(h.fields.iter().any(|f| f.contains("reviewer")));
        assert!(h.fields.iter().any(|f| f.contains("pr_author")));
        assert!(h.fields.iter().any(|f| f.contains("review_state")));
    }

    #[test]
    fn merge_gate_hint() {
        let h = merge_gate();
        assert_eq!(h.name, "merge_gate");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 4);
    }

    #[test]
    fn issue_triage_hint() {
        let h = issue_triage();
        assert_eq!(h.name, "issue_triage");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 4);
    }

    #[test]
    fn contributor_lifecycle_hint() {
        let h = contributor_lifecycle();
        assert_eq!(h.name, "contributor_lifecycle");
        assert_eq!(h.relationship, RelationshipType::FunctionalDependency);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn release_cadence_hint() {
        let h = release_cadence();
        assert_eq!(h.name, "release_cadence");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn pr_review_assignment_hint() {
        let h = pr_review_assignment();
        assert_eq!(h.name, "pr_review_assignment");
        assert_eq!(h.relationship, RelationshipType::FunctionalDependency);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn commit_cascade_hint() {
        let h = commit_cascade();
        assert_eq!(h.name, "commit_cascade");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 4);
    }
}
