//! Package-manifest domain plugin for Vajra.
//!
//! Covers npm, Python (PyPI) and Cargo manifests. Two kinds of domain
//! knowledge, because manifests need both:
//!
//! - **Type recognizers** classify values: package names, semver ranges,
//!   registry URLs, SPDX licences, lifecycle hook names.
//! - **Structural detectors** read shape: whether an install-time hook is
//!   declared, dependency and script counts, whether provenance fields are
//!   present. None of that is a property of any single value, which is why the
//!   [`vajra_types::traits::StructuralDetector`] surface exists.
//!
//! The install-hook signal is the point of this plugin. Code in an npm
//! `preinstall` runs on `npm install`, before anyone has read the package —
//! and it is a property of a *key* existing, so no value recogniser can see it.

pub mod detectors;
pub mod recognizers;

use vajra_types::traits::{StructuralDetector, TypeRecognizer, VajraPlugin};

use crate::detectors::{CargoManifestDetector, NpmManifestDetector, PythonManifestDetector};
use crate::recognizers::{
    LifecycleHookNameRecognizer, NpmPackageNameRecognizer, RegistryUrlRecognizer,
    SemVerRangeRecognizer, SpdxLicenseRecognizer,
};

/// The package-manifest domain plugin.
pub struct PackagePlugin;

impl VajraPlugin for PackagePlugin {
    fn name(&self) -> &str {
        "package"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
            Box::new(RegistryUrlRecognizer),
            Box::new(SpdxLicenseRecognizer),
            Box::new(LifecycleHookNameRecognizer),
            Box::new(SemVerRangeRecognizer),
            Box::new(NpmPackageNameRecognizer),
        ]
    }

    fn structural_detectors(&self) -> Vec<Box<dyn StructuralDetector>> {
        vec![
            Box::new(NpmManifestDetector),
            Box::new(PythonManifestDetector),
            Box::new(CargoManifestDetector),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_identity() {
        let p = PackagePlugin;
        assert_eq!(p.name(), "package");
        assert!(!p.version().is_empty());
    }

    #[test]
    fn registers_recognizers_and_detectors() {
        let p = PackagePlugin;
        assert_eq!(p.type_recognizers().len(), 5);
        assert_eq!(p.structural_detectors().len(), 3);
    }

    /// Recognizers are consulted in priority order, and the first match wins,
    /// so the loose package-name matcher must sort below the specific ones.
    #[test]
    fn recognizer_priorities_are_descending() {
        let p = PackagePlugin;
        let priorities: Vec<u32> = p.type_recognizers().iter().map(|r| r.priority()).collect();
        assert!(
            priorities.windows(2).all(|w| w[0] >= w[1]),
            "expected descending priorities, got {priorities:?}"
        );
    }

    /// Exactly one detector should claim any given manifest, or a document would
    /// be reported twice under different ecosystems.
    #[test]
    fn detectors_do_not_overlap() -> Result<(), Box<dyn std::error::Error>> {
        let cases = [
            r#"{"name":"x","version":"1.0.0","scripts":{"preinstall":"a"}}"#,
            r#"{"build-system":{"requires":["setuptools"]}}"#,
            r#"{"package":{"name":"x","version":"0.1.0"}}"#,
        ];
        let p = PackagePlugin;
        let detectors = p.structural_detectors();
        for case in cases {
            let v: serde_json::Value = serde_json::from_str(case)?;
            let claimed = detectors.iter().filter(|d| d.applies(&v)).count();
            assert_eq!(claimed, 1, "{case} claimed by {claimed} detectors");
        }
        Ok(())
    }

    #[test]
    fn no_detector_claims_unrelated_data() -> Result<(), Box<dyn std::error::Error>> {
        let p = PackagePlugin;
        let detectors = p.structural_detectors();
        for case in [
            r#"{"name":"Alice","age":30}"#,
            r#"{"status":"denied","amount":42}"#,
            r#"[1,2,3]"#,
            r#""just a string""#,
        ] {
            let v: serde_json::Value = serde_json::from_str(case)?;
            assert_eq!(
                detectors.iter().filter(|d| d.applies(&v)).count(),
                0,
                "{case} should not be claimed"
            );
        }
        Ok(())
    }
}
