//! DevOps/infrastructure domain relationship hints.
//!
//! These hints describe expected co-occurrence and dependency patterns
//! in infrastructure data (Kubernetes, CI/CD, Terraform), allowing Vajra's
//! analysis engine to better understand domain-specific field relationships.

use vajra_types::traits::{RelationshipHint, RelationshipType};

/// Returns all DevOps domain relationship hints.
pub fn devops_hints() -> Vec<RelationshipHint> {
    vec![
        k8s_pod_spec(),
        deployment_metadata(),
        service_endpoint(),
        terraform_resource_block(),
        ci_pipeline_stage(),
        container_spec(),
    ]
}

/// Kubernetes pod spec: container image + resources + ports co-occur.
fn k8s_pod_spec() -> RelationshipHint {
    RelationshipHint {
        name: "k8s_pod_spec".to_owned(),
        fields: vec![
            "**/containers/*/image".to_owned(),
            "**/containers/*/resources".to_owned(),
            "**/containers/*/ports".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.95,
    }
}

/// Deployment metadata: name + namespace + labels co-occur.
fn deployment_metadata() -> RelationshipHint {
    RelationshipHint {
        name: "deployment_metadata".to_owned(),
        fields: vec![
            "**/metadata/name".to_owned(),
            "**/metadata/namespace".to_owned(),
            "**/metadata/labels".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.90,
    }
}

/// Service endpoint: port + target port + protocol co-occur.
fn service_endpoint() -> RelationshipHint {
    RelationshipHint {
        name: "service_endpoint".to_owned(),
        fields: vec![
            "**/spec/ports/*/port".to_owned(),
            "**/spec/ports/*/targetPort".to_owned(),
            "**/spec/ports/*/protocol".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// Terraform resource block: type + provider + attributes co-occur.
fn terraform_resource_block() -> RelationshipHint {
    RelationshipHint {
        name: "terraform_resource_block".to_owned(),
        fields: vec![
            "**/type".to_owned(),
            "**/provider".to_owned(),
            "**/instances/*/attributes".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.85,
    }
}

/// CI pipeline stage: name + status + duration co-occur.
fn ci_pipeline_stage() -> RelationshipHint {
    RelationshipHint {
        name: "ci_pipeline_stage".to_owned(),
        fields: vec![
            "**/stages/*/name".to_owned(),
            "**/stages/*/status".to_owned(),
            "**/stages/*/duration".to_owned(),
        ],
        relationship: RelationshipType::CoOccurrence,
        weight: 0.80,
    }
}

/// Container spec: image + command + env + volumeMounts co-occur.
fn container_spec() -> RelationshipHint {
    RelationshipHint {
        name: "container_spec".to_owned(),
        fields: vec![
            "**/image".to_owned(),
            "**/command".to_owned(),
            "**/env".to_owned(),
            "**/volumeMounts".to_owned(),
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
        let hints = devops_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn hint_names_are_unique() {
        let hints = devops_hints();
        let mut names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), hints.len(), "hint names must be unique");
    }

    #[test]
    fn all_hints_have_fields() {
        let hints = devops_hints();
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
        let hints = devops_hints();
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
        let hints = devops_hints();
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
    fn k8s_pod_spec_hint() {
        let h = k8s_pod_spec();
        assert_eq!(h.name, "k8s_pod_spec");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
        assert!(h.fields.iter().any(|f| f.contains("image")));
        assert!(h.fields.iter().any(|f| f.contains("resources")));
        assert!(h.fields.iter().any(|f| f.contains("ports")));
    }

    #[test]
    fn deployment_metadata_hint() {
        let h = deployment_metadata();
        assert_eq!(h.name, "deployment_metadata");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn service_endpoint_hint() {
        let h = service_endpoint();
        assert_eq!(h.name, "service_endpoint");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn terraform_resource_block_hint() {
        let h = terraform_resource_block();
        assert_eq!(h.name, "terraform_resource_block");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn ci_pipeline_stage_hint() {
        let h = ci_pipeline_stage();
        assert_eq!(h.name, "ci_pipeline_stage");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 3);
    }

    #[test]
    fn container_spec_hint() {
        let h = container_spec();
        assert_eq!(h.name, "container_spec");
        assert_eq!(h.relationship, RelationshipType::CoOccurrence);
        assert_eq!(h.fields.len(), 4);
    }
}
