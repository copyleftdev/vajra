//! Security domain relationship hints.
//!
//! These hints describe expected co-occurrence and dependency patterns
//! in security event data, allowing Vajra's analysis engine to better
//! understand domain-specific field relationships.

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all security domain relationship hints.
pub fn security_hints() -> Vec<RelationshipHint> {
    vec![
        network_flow(),
        alert_classification(),
        vulnerability_assessment(),
        authentication_event(),
        process_execution(),
        dns_query(),
    ]
}

/// Network flow: source/destination IPs and ports co-occur.
fn network_flow() -> RelationshipHint {
    RelationshipHint {
        name: "network_flow".to_owned(),
        fields: vec![
            "**/src_ip".to_owned(),
            "**/dst_ip".to_owned(),
            "**/src_port".to_owned(),
            "**/dst_port".to_owned(),
            "**/protocol".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.95,
    }
}

/// Alert classification: MITRE tactic + technique + severity.
fn alert_classification() -> RelationshipHint {
    RelationshipHint {
        name: "alert_classification".to_owned(),
        fields: vec![
            "**/tactic".to_owned(),
            "**/technique".to_owned(),
            "**/severity".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.90,
    }
}

/// Vulnerability: CVE + CVSS score + affected asset.
fn vulnerability_assessment() -> RelationshipHint {
    RelationshipHint {
        name: "vulnerability_assessment".to_owned(),
        fields: vec![
            "**/cve*".to_owned(),
            "**/cvss*".to_owned(),
            "**/affected_*".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.90,
    }
}

/// Auth events: username + source IP + success/failure.
fn authentication_event() -> RelationshipHint {
    RelationshipHint {
        name: "authentication_event".to_owned(),
        fields: vec![
            "**/user*".to_owned(),
            "**/source_ip".to_owned(),
            "**/auth_result".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// Process execution: process name + PID + command line + parent PID.
fn process_execution() -> RelationshipHint {
    RelationshipHint {
        name: "process_execution".to_owned(),
        fields: vec![
            "**/process_name".to_owned(),
            "**/pid".to_owned(),
            "**/command_line".to_owned(),
            "**/parent_pid".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.80,
    }
}

/// DNS query: query name + record type + response.
fn dns_query() -> RelationshipHint {
    RelationshipHint {
        name: "dns_query".to_owned(),
        fields: vec![
            "**/query_name".to_owned(),
            "**/query_type".to_owned(),
            "**/answer*".to_owned(),
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
        let hints = security_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = security_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len(), "hint names must be unique");
    }

    #[test]
    fn all_hints_have_fields() {
        let hints = security_hints();
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
        let hints = security_hints();
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
        let hints = security_hints();
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
    fn network_flow_hint() {
        let h = network_flow();
        assert_eq!(h.name, "network_flow");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 5);
        assert!(h.fields.iter().any(|f| f.contains("src_ip")));
        assert!(h.fields.iter().any(|f| f.contains("dst_ip")));
        assert!(h.fields.iter().any(|f| f.contains("src_port")));
        assert!(h.fields.iter().any(|f| f.contains("dst_port")));
        assert!(h.fields.iter().any(|f| f.contains("protocol")));
    }

    #[test]
    fn alert_classification_hint() {
        let h = alert_classification();
        assert_eq!(h.name, "alert_classification");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn vulnerability_assessment_hint() {
        let h = vulnerability_assessment();
        assert_eq!(h.name, "vulnerability_assessment");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn authentication_event_hint() {
        let h = authentication_event();
        assert_eq!(h.name, "authentication_event");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn process_execution_hint() {
        let h = process_execution();
        assert_eq!(h.name, "process_execution");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 4);
    }

    #[test]
    fn dns_query_hint() {
        let h = dns_query();
        assert_eq!(h.name, "dns_query");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }
}
