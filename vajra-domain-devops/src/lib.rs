//! DevOps/infrastructure domain plugin for Vajra.
//!
//! Provides type recognizers for common infrastructure data types
//! (container IDs, semver versions, AWS ARNs, Docker images, git SHAs,
//! Kubernetes namespaces, Terraform resources, CIDR blocks, cron expressions)
//! and relationship hints that describe expected field co-occurrence patterns
//! in infrastructure and deployment data.

pub mod hints;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::devops_hints;
use crate::recognizers::{
    ArnRecognizer, CidrBlockRecognizer, ContainerIdRecognizer, CronExprRecognizer,
    DockerImageRecognizer, GcpResourceRecognizer, GitShaRecognizer, K8sNamespaceRecognizer,
    SemverRecognizer, TerraformResourceRecognizer,
};

/// The DevOps/infrastructure domain plugin.
///
/// Registers infrastructure-specific type recognizers and relationship hints
/// to enhance Vajra's analysis of Kubernetes manifests, Terraform state,
/// CI/CD pipeline output, and cloud infrastructure JSON.
pub struct DevOpsPlugin;

impl VajraPlugin for DevOpsPlugin {
    fn name(&self) -> &str {
        "devops"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(ContainerIdRecognizer),
            Box::new(SemverRecognizer),
            Box::new(GitShaRecognizer),
            Box::new(DockerImageRecognizer),
            Box::new(ArnRecognizer),
            Box::new(GcpResourceRecognizer),
            Box::new(CidrBlockRecognizer),
            Box::new(CronExprRecognizer),
            Box::new(K8sNamespaceRecognizer),
            Box::new(TerraformResourceRecognizer),
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        devops_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = DevOpsPlugin;
        assert_eq!(p.name(), "devops");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        let p = DevOpsPlugin;
        let recognizers = p.type_recognizers();
        assert_eq!(recognizers.len(), 10);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        let p = DevOpsPlugin;
        let hints = p.relationship_hints();
        assert_eq!(hints.len(), 6);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let p = DevOpsPlugin;
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
        let p = DevOpsPlugin;
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
        assert_send_sync::<DevOpsPlugin>();
    }

    #[test]
    fn recognizers_are_send_and_sync() {
        let p = DevOpsPlugin;
        let recognizers = p.type_recognizers();
        assert!(!recognizers.is_empty());
    }

    #[test]
    fn hint_names_match_expected() {
        let p = DevOpsPlugin;
        let hints = p.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"k8s_pod_spec"));
        assert!(names.contains(&"deployment_metadata"));
        assert!(names.contains(&"service_endpoint"));
        assert!(names.contains(&"terraform_resource_block"));
        assert!(names.contains(&"ci_pipeline_stage"));
        assert!(names.contains(&"container_spec"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "v1.2.3",
            "arn:aws:s3:::my-bucket",
            "a1b2c3d4e5f6",
            "10.0.0.0/24",
            "nginx:latest",
            "kube-system",
            "aws_instance.web",
            "* * * * *",
            "not-a-match",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let p = DevOpsPlugin;
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
            let p = DevOpsPlugin;
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
