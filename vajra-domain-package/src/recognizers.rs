//! Value recognizers for package-ecosystem data types.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

/// Recognizes npm package names, including the scoped `@scope/name` form.
pub struct NpmPackageNameRecognizer;

static NPM_NAME_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for NpmPackageNameRecognizer {
    fn type_name(&self) -> &str {
        "npm_package_name"
    }

    fn matches(&self, value: &str) -> bool {
        // npm caps names at 214 characters and forbids uppercase.
        if value.is_empty() || value.len() > 214 {
            return false;
        }
        if value != value.to_lowercase() {
            return false;
        }
        let Some(re) = compile_once(
            &NPM_NAME_RE,
            r"^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        // Plain lowercase words also match, so this is a weak signal alone.
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        // Below more specific types; a scoped name is distinctive, a bare word
        // is not.
        30
    }
}

/// Recognizes npm/semver version *ranges*, e.g. `^1.2.3`, `~2.0`, `>=1 <2`, `*`.
pub struct SemVerRangeRecognizer;

static SEMVER_RANGE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SemVerRangeRecognizer {
    fn type_name(&self) -> &str {
        "semver_range"
    }

    fn matches(&self, value: &str) -> bool {
        if value == "*" || value == "latest" {
            return true;
        }
        let Some(re) = compile_once(
            &SEMVER_RANGE_RE,
            r"^(?:[\^~]|>=?|<=?|=)?\s*\d+(?:\.\d+){0,2}(?:-[0-9A-Za-z.-]+)?(?:\s+(?:[\^~]|>=?|<=?|=)?\s*\d+(?:\.\d+){0,2})*$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        60
    }
}

/// Recognizes package-registry URLs.
pub struct RegistryUrlRecognizer;

impl TypeRecognizer for RegistryUrlRecognizer {
    fn type_name(&self) -> &str {
        "registry_url"
    }

    fn matches(&self, value: &str) -> bool {
        const HOSTS: &[&str] = &[
            "registry.npmjs.org",
            "registry.yarnpkg.com",
            "pypi.org",
            "files.pythonhosted.org",
            "crates.io",
            "static.crates.io",
            "rubygems.org",
            "proxy.golang.org",
        ];
        HOSTS.iter().any(|h| value.contains(h))
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        90
    }
}

/// Recognizes the names of npm lifecycle scripts that run automatically.
///
/// Useful when script names appear as *values* — a lockfile listing which hooks
/// a dependency declares, or an audit report enumerating them.
pub struct LifecycleHookNameRecognizer;

impl TypeRecognizer for LifecycleHookNameRecognizer {
    fn type_name(&self) -> &str {
        "lifecycle_install_hook"
    }

    fn matches(&self, value: &str) -> bool {
        matches!(
            value,
            "preinstall" | "install" | "postinstall" | "prepare" | "prepublish"
        )
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        80
    }
}

/// Recognizes common SPDX licence identifiers.
pub struct SpdxLicenseRecognizer;

static SPDX_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SpdxLicenseRecognizer {
    fn type_name(&self) -> &str {
        "spdx_license"
    }

    fn matches(&self, value: &str) -> bool {
        if value.len() > 64 {
            return false;
        }
        let Some(re) = compile_once(
            &SPDX_RE,
            r"^(?:MIT|ISC|Apache-2\.0|BSD-2-Clause|BSD-3-Clause|GPL-[23]\.0(?:-only|-or-later)?|LGPL-[23]\.[01](?:-only|-or-later)?|MPL-2\.0|Unlicense|CC0-1\.0|AGPL-3\.0(?:-only|-or-later)?)$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        85
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_names() {
        let r = NpmPackageNameRecognizer;
        assert!(r.matches("lodash"));
        assert!(r.matches("@scope/pkg"));
        assert!(r.matches("some.pkg-name_2"));
        // Uppercase is forbidden by npm.
        assert!(!r.matches("Lodash"));
        assert!(!r.matches(""));
        assert!(!r.matches("@/bad"));
        assert!(!r.matches("has space"));
        assert!(!r.matches(&"a".repeat(215)));
    }

    #[test]
    fn semver_ranges() {
        let r = SemVerRangeRecognizer;
        for good in [
            "1.2.3",
            "^1.2.3",
            "~2.0",
            ">=1.0.0",
            "*",
            "latest",
            "1.0.0-beta.1",
        ] {
            assert!(r.matches(good), "{good} should match");
        }
        for bad in ["", "abc", "not-a-version", "v"] {
            assert!(!r.matches(bad), "{bad} should not match");
        }
    }

    #[test]
    fn registry_urls() {
        let r = RegistryUrlRecognizer;
        assert!(r.matches("https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"));
        assert!(r.matches("https://files.pythonhosted.org/packages/x/y.whl"));
        assert!(r.matches("https://static.crates.io/crates/serde/serde-1.0.0.crate"));
        assert!(!r.matches("https://example.com/pkg.tgz"));
    }

    #[test]
    fn lifecycle_hook_names() {
        let r = LifecycleHookNameRecognizer;
        for hook in [
            "preinstall",
            "install",
            "postinstall",
            "prepare",
            "prepublish",
        ] {
            assert!(r.matches(hook));
        }
        // Not automatic on install.
        for other in ["build", "test", "lint", "start"] {
            assert!(!r.matches(other), "{other} is not an install hook");
        }
    }

    #[test]
    fn spdx_licenses() {
        let r = SpdxLicenseRecognizer;
        for good in [
            "MIT",
            "Apache-2.0",
            "BSD-3-Clause",
            "GPL-3.0-or-later",
            "ISC",
        ] {
            assert!(r.matches(good), "{good} should match");
        }
        for bad in ["", "Proprietary", "MIT OR Apache-2.0", "mit"] {
            assert!(!r.matches(bad), "{bad} should not match");
        }
    }
}
