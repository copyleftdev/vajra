//! DevOps/infrastructure domain type recognizers.
//!
//! Each recognizer identifies a specific infrastructure data type
//! (container IDs, semver versions, AWS ARNs, git SHAs, Docker images,
//! Kubernetes resources, Terraform identifiers) by matching string values
//! against compiled regular expressions. Regexes are compiled once using
//! `OnceLock`.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

/// Helper to compile a regex once and return a reference.
/// Returns `None` if the regex is invalid (should not happen with known patterns).
fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ---------------------------------------------------------------------------
// Container ID Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Docker container IDs: 64 hex chars (full) or 12 hex chars (short).
pub struct ContainerIdRecognizer;

static CONTAINER_ID_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ContainerIdRecognizer {
    fn type_name(&self) -> &str {
        "container_id"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&CONTAINER_ID_RE, r"^[a-f0-9]{12}$|^[a-f0-9]{64}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        90
    }
}

// ---------------------------------------------------------------------------
// Semver Recognizer
// ---------------------------------------------------------------------------

/// Recognizes semantic versions: optional 'v' prefix, then MAJOR.MINOR.PATCH
/// with optional prerelease and build metadata.
/// Examples: 1.0.0, v2.3.4-beta.1, 1.0.0+build.123
pub struct SemverRecognizer;

static SEMVER_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SemverRecognizer {
    fn type_name(&self) -> &str {
        "semver"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &SEMVER_RE,
            r"^v?\d+\.\d+\.\d+(-[\w][\w.]*)?(\+[\w][\w.]*)?$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        100
    }
}

// ---------------------------------------------------------------------------
// Git SHA Recognizer
// ---------------------------------------------------------------------------

/// Recognizes git commit SHAs: 40 hex chars (full) or 7-12 hex chars (short).
/// Uses lowercase hex only to avoid false positives with other hex strings.
pub struct GitShaRecognizer;

static GIT_SHA_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for GitShaRecognizer {
    fn type_name(&self) -> &str {
        "git_sha"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&GIT_SHA_RE, r"^[a-f0-9]{7,12}$|^[a-f0-9]{40}$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        85
    }
}

// ---------------------------------------------------------------------------
// Docker Image Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Docker image references: [registry/]repository:tag or
/// [registry/]repository@sha256:digest.
/// Examples: nginx:latest, gcr.io/project/image:v1.2.3, ubuntu:22.04
pub struct DockerImageRecognizer;

static DOCKER_IMAGE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for DockerImageRecognizer {
    fn type_name(&self) -> &str {
        "docker_image"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &DOCKER_IMAGE_RE,
            r"^(?:[a-zA-Z0-9][-a-zA-Z0-9.]*(?::\d+)?/)?[a-z0-9]+(?:[._/-][a-z0-9]+)*(?::[a-zA-Z0-9][\w.-]{0,127}|@sha256:[a-f0-9]{64})$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        95
    }
}

// ---------------------------------------------------------------------------
// AWS ARN Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Amazon Resource Names (ARNs).
/// Format: arn:partition:service:region:account-id:resource
/// Examples: arn:aws:s3:::my-bucket, arn:aws:ec2:us-east-1:123456789012:instance/i-123
pub struct ArnRecognizer;

static ARN_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ArnRecognizer {
    fn type_name(&self) -> &str {
        "aws_arn"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &ARN_RE,
            r"^arn:(?:aws|aws-cn|aws-us-gov):[a-zA-Z0-9-]+:[a-z0-9-]*:\d{0,12}:.+$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        110
    }
}

// ---------------------------------------------------------------------------
// GCP Resource Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Google Cloud Platform resource paths.
/// Format: projects/{project}/... or organizations/{org}/...
pub struct GcpResourceRecognizer;

static GCP_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for GcpResourceRecognizer {
    fn type_name(&self) -> &str {
        "gcp_resource"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &GCP_RE,
            r"^(?:projects|organizations|folders)/[a-zA-Z0-9_-]+(?:/[a-zA-Z0-9_/-]+)+$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        105
    }
}

// ---------------------------------------------------------------------------
// CIDR Block Recognizer
// ---------------------------------------------------------------------------

/// Recognizes CIDR notation: an IPv4 address followed by /prefix (0-32).
pub struct CidrBlockRecognizer;

static CIDR_BLOCK_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CidrBlockRecognizer {
    fn type_name(&self) -> &str {
        "cidr_block"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &CIDR_BLOCK_RE,
            r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)/(?:[0-9]|[12][0-9]|3[0-2])$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        108
    }
}

// ---------------------------------------------------------------------------
// Cron Expression Recognizer
// ---------------------------------------------------------------------------

/// Recognizes 5-field cron expressions (minute hour dom month dow).
/// Example: "0 */6 * * *", "30 2 * * 1-5"
pub struct CronExprRecognizer;

static CRON_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for CronExprRecognizer {
    fn type_name(&self) -> &str {
        "cron_expression"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&CRON_RE, r"^(?:[0-9*,/\-]+\s+){4}[0-9*,/\-]+$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        55
    }
}

// ---------------------------------------------------------------------------
// Kubernetes Namespace Recognizer
// ---------------------------------------------------------------------------

/// Recognizes well-known Kubernetes system namespaces and DNS-1123 label format
/// namespace names. Only matches names that look like K8s namespaces
/// (lowercase, hyphens allowed, no dots).
pub struct K8sNamespaceRecognizer;

static K8S_NS_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for K8sNamespaceRecognizer {
    fn type_name(&self) -> &str {
        "k8s_namespace"
    }

    fn matches(&self, value: &str) -> bool {
        // Match well-known system namespaces exactly
        matches!(
            value,
            "default"
                | "kube-system"
                | "kube-public"
                | "kube-node-lease"
                | "istio-system"
                | "cert-manager"
                | "ingress-nginx"
                | "monitoring"
                | "logging"
                | "argocd"
        ) || {
            // Or match DNS-1123 label format if it contains a hyphen (to reduce false positives)
            let Some(re) = compile_once(&K8S_NS_RE, r"^[a-z][a-z0-9]*(?:-[a-z0-9]+)+$") else {
                return false;
            };
            re.is_match(value)
        }
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        60
    }
}

// ---------------------------------------------------------------------------
// Terraform Resource ID Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Terraform resource identifiers: provider_type.name format.
/// Examples: aws_instance.web, google_compute_instance.default, azurerm_resource_group.main
pub struct TerraformResourceRecognizer;

static TF_RESOURCE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for TerraformResourceRecognizer {
    fn type_name(&self) -> &str {
        "terraform_resource"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &TF_RESOURCE_RE,
            r"^(?:aws|google|azurerm|kubernetes|helm|null|random|local|tls)_[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        65
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Container ID tests
    // -----------------------------------------------------------------------

    #[test]
    fn container_id_valid() {
        let r = ContainerIdRecognizer;
        // Full 64-char Docker container ID
        assert!(r.matches("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"));
        // Short 12-char ID
        assert!(r.matches("a1b2c3d4e5f6"));
    }

    #[test]
    fn container_id_invalid() {
        let r = ContainerIdRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("a1b2c3")); // too short
        assert!(!r.matches("AABBCCDDEEFF")); // uppercase
        assert!(!r.matches("a1b2c3d4e5f6a")); // 13 chars
        assert!(!r.matches("g1b2c3d4e5f6")); // non-hex
    }

    #[test]
    fn container_id_metadata() {
        let r = ContainerIdRecognizer;
        assert_eq!(r.type_name(), "container_id");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 90);
    }

    // -----------------------------------------------------------------------
    // Semver tests
    // -----------------------------------------------------------------------

    #[test]
    fn semver_valid() {
        let r = SemverRecognizer;
        assert!(r.matches("1.0.0"));
        assert!(r.matches("v1.0.0"));
        assert!(r.matches("2.3.4-beta.1"));
        assert!(r.matches("1.0.0+build.123"));
        assert!(r.matches("0.0.1-alpha+001"));
        assert!(r.matches("v10.20.30"));
        assert!(r.matches("0.1.0"));
    }

    #[test]
    fn semver_invalid() {
        let r = SemverRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("1.0")); // missing patch
        assert!(!r.matches("1.0.0.0")); // too many parts
        assert!(!r.matches("latest")); // tag, not semver
        assert!(!r.matches("v")); // just prefix
        assert!(!r.matches("1.0.0-")); // dangling hyphen
    }

    #[test]
    fn semver_metadata() {
        let r = SemverRecognizer;
        assert_eq!(r.type_name(), "semver");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 100);
    }

    // -----------------------------------------------------------------------
    // Git SHA tests
    // -----------------------------------------------------------------------

    #[test]
    fn git_sha_valid() {
        let r = GitShaRecognizer;
        // Full 40-char SHA
        assert!(r.matches("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"));
        // Short 7-char
        assert!(r.matches("a1b2c3d"));
        // Short 12-char
        assert!(r.matches("a1b2c3d4e5f6"));
        // 8-char
        assert!(r.matches("a1b2c3d4"));
    }

    #[test]
    fn git_sha_invalid() {
        let r = GitShaRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("a1b2c3")); // 6 chars — too short
        assert!(!r.matches("AABBCCDD")); // uppercase
        assert!(!r.matches("a1b2c3d4e5f6a")); // 13 chars (between short and full)
        assert!(!r.matches("gggggggg")); // non-hex
    }

    #[test]
    fn git_sha_metadata() {
        let r = GitShaRecognizer;
        assert_eq!(r.type_name(), "git_sha");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 85);
    }

    // -----------------------------------------------------------------------
    // Docker Image tests
    // -----------------------------------------------------------------------

    #[test]
    fn docker_image_valid() {
        let r = DockerImageRecognizer;
        assert!(r.matches("nginx:latest"));
        assert!(r.matches("ubuntu:22.04"));
        assert!(r.matches("gcr.io/project/image:v1.2.3"));
        assert!(r.matches("registry.example.com/app:v1.2.3"));
        assert!(r.matches("myrepo/myimage:tag"));
    }

    #[test]
    fn docker_image_invalid() {
        let r = DockerImageRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("://invalid"));
        assert!(!r.matches(":tag")); // no repo
    }

    #[test]
    fn docker_image_metadata() {
        let r = DockerImageRecognizer;
        assert_eq!(r.type_name(), "docker_image");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 95);
    }

    // -----------------------------------------------------------------------
    // AWS ARN tests
    // -----------------------------------------------------------------------

    #[test]
    fn arn_valid() {
        let r = ArnRecognizer;
        assert!(r.matches("arn:aws:s3:::my-bucket"));
        assert!(r.matches("arn:aws:ec2:us-east-1:123456789012:instance/i-1234567890abcdef0"));
        assert!(r.matches("arn:aws:iam::123456789012:role/MyRole"));
        assert!(r.matches("arn:aws-cn:s3:::my-bucket")); // China region
        assert!(r.matches("arn:aws-us-gov:s3:::my-bucket")); // GovCloud
    }

    #[test]
    fn arn_invalid() {
        let r = ArnRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("arn:azure:something::")); // wrong provider
        assert!(!r.matches("aws:s3:::bucket")); // missing arn prefix
    }

    #[test]
    fn arn_metadata() {
        let r = ArnRecognizer;
        assert_eq!(r.type_name(), "aws_arn");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 110);
    }

    // -----------------------------------------------------------------------
    // GCP Resource tests
    // -----------------------------------------------------------------------

    #[test]
    fn gcp_resource_valid() {
        let r = GcpResourceRecognizer;
        assert!(r.matches("projects/my-project/zones/us-central1-a"));
        assert!(r.matches("organizations/123456/roles/custom-role"));
        assert!(r.matches("folders/12345/locations/global"));
        assert!(r.matches("projects/my-project/topics/my-topic"));
    }

    #[test]
    fn gcp_resource_invalid() {
        let r = GcpResourceRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("projects/")); // no project name
        assert!(!r.matches("projects/my-project")); // no sub-resource
        assert!(!r.matches("buckets/my-bucket")); // not a valid prefix
    }

    #[test]
    fn gcp_resource_metadata() {
        let r = GcpResourceRecognizer;
        assert_eq!(r.type_name(), "gcp_resource");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 105);
    }

    // -----------------------------------------------------------------------
    // CIDR Block tests
    // -----------------------------------------------------------------------

    #[test]
    fn cidr_block_valid() {
        let r = CidrBlockRecognizer;
        assert!(r.matches("10.0.0.0/8"));
        assert!(r.matches("192.168.1.0/24"));
        assert!(r.matches("0.0.0.0/0"));
        assert!(r.matches("255.255.255.255/32"));
    }

    #[test]
    fn cidr_block_invalid() {
        let r = CidrBlockRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("192.168.1.1")); // no prefix
        assert!(!r.matches("192.168.1.0/33")); // prefix > 32
        assert!(!r.matches("256.0.0.0/8")); // invalid octet
    }

    #[test]
    fn cidr_block_metadata() {
        let r = CidrBlockRecognizer;
        assert_eq!(r.type_name(), "cidr_block");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 108);
    }

    // -----------------------------------------------------------------------
    // Cron Expression tests
    // -----------------------------------------------------------------------

    #[test]
    fn cron_valid() {
        let r = CronExprRecognizer;
        assert!(r.matches("* * * * *")); // every minute
        assert!(r.matches("0 */6 * * *")); // every 6 hours
        assert!(r.matches("30 2 * * 1-5")); // weekdays at 2:30
        assert!(r.matches("0 0 1 * *")); // first of month
    }

    #[test]
    fn cron_invalid() {
        let r = CronExprRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("* * * *")); // only 4 fields
        assert!(!r.matches("hello world")); // not cron
        assert!(!r.matches("* * * * * *")); // 6 fields (extended, not standard 5)
    }

    #[test]
    fn cron_metadata() {
        let r = CronExprRecognizer;
        assert_eq!(r.type_name(), "cron_expression");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 55);
    }

    // -----------------------------------------------------------------------
    // K8s Namespace tests
    // -----------------------------------------------------------------------

    #[test]
    fn k8s_namespace_valid() {
        let r = K8sNamespaceRecognizer;
        assert!(r.matches("default"));
        assert!(r.matches("kube-system"));
        assert!(r.matches("kube-public"));
        assert!(r.matches("kube-node-lease"));
        assert!(r.matches("istio-system"));
        assert!(r.matches("cert-manager"));
        assert!(r.matches("my-namespace")); // DNS-1123 with hyphen
        assert!(r.matches("app-staging-v2"));
    }

    #[test]
    fn k8s_namespace_invalid() {
        let r = K8sNamespaceRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("My-Namespace")); // uppercase
        assert!(!r.matches("-invalid")); // starts with hyphen
        assert!(!r.matches("has spaces"));
        assert!(!r.matches("has.dots")); // dots not allowed in namespaces
    }

    #[test]
    fn k8s_namespace_metadata() {
        let r = K8sNamespaceRecognizer;
        assert_eq!(r.type_name(), "k8s_namespace");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 60);
    }

    // -----------------------------------------------------------------------
    // Terraform Resource tests
    // -----------------------------------------------------------------------

    #[test]
    fn terraform_resource_valid() {
        let r = TerraformResourceRecognizer;
        assert!(r.matches("aws_instance.web"));
        assert!(r.matches("aws_s3_bucket.main"));
        assert!(r.matches("google_compute_instance.default"));
        assert!(r.matches("azurerm_resource_group.main"));
        assert!(r.matches("kubernetes_deployment.app"));
        assert!(r.matches("null_resource.provisioner"));
        assert!(r.matches("random_string.suffix"));
    }

    #[test]
    fn terraform_resource_invalid() {
        let r = TerraformResourceRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("instance.web")); // no provider prefix
        assert!(!r.matches("aws_instance")); // no name
        assert!(!r.matches("AWS_instance.web")); // uppercase
        assert!(!r.matches("custom_provider.name")); // unknown provider
    }

    #[test]
    fn terraform_resource_metadata() {
        let r = TerraformResourceRecognizer;
        assert_eq!(r.type_name(), "terraform_resource");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 65);
    }

    // -----------------------------------------------------------------------
    // Edge cases across all recognizers
    // -----------------------------------------------------------------------

    #[test]
    fn all_recognizers_reject_empty_string() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
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
        ];
        for r in &recognizers {
            assert!(
                !r.matches(""),
                "{} should not match empty string",
                r.type_name()
            );
        }
    }

    #[test]
    fn all_recognizers_reject_whitespace() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
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
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match spaces",
                r.type_name()
            );
            assert!(!r.matches("\t"), "{} should not match tab", r.type_name());
            assert!(
                !r.matches("\n"),
                "{} should not match newline",
                r.type_name()
            );
        }
    }

    #[test]
    fn near_miss_boundary_cases() {
        // Container ID: 11 chars (one short)
        assert!(!ContainerIdRecognizer.matches("a1b2c3d4e5f"));

        // Semver: missing patch
        assert!(!SemverRecognizer.matches("1.0"));

        // Git SHA: 6 chars (one short of min)
        assert!(!GitShaRecognizer.matches("a1b2c3"));

        // ARN: wrong partition
        assert!(!ArnRecognizer.matches("arn:azure:s3:::bucket"));

        // CIDR: prefix 33
        assert!(!CidrBlockRecognizer.matches("10.0.0.0/33"));

        // Terraform: unknown provider
        assert!(!TerraformResourceRecognizer.matches("unknown_thing.name"));
    }
}
