//! Structural detectors for package manifests.
//!
//! These read *shape*, not values: whether a lifecycle hook is declared, how
//! many dependencies a manifest pulls in, whether provenance fields are
//! present. None of that can be expressed by a value recogniser, which is why
//! this uses [`StructuralDetector`].

use serde_json::Value;
use vajra_types::traits::{FindingSeverity, StructuralDetector, StructuralFinding};

/// npm lifecycle scripts that execute automatically, without the user asking.
///
/// These are the ones that matter: code here runs on `npm install`, so a
/// package can act before anyone has read a line of it. `prepublish` is
/// included because it still fires on install for older npm versions.
const NPM_INSTALL_HOOKS: &[&str] = &[
    "preinstall",
    "install",
    "postinstall",
    "prepare",
    "prepublish",
];

/// Provenance fields whose absence makes a package harder to attribute.
const NPM_PROVENANCE: &[&str] = &["repository", "homepage", "bugs", "author", "license"];

fn finding(
    signal: &str,
    path: &str,
    detail: String,
    severity: FindingSeverity,
) -> StructuralFinding {
    StructuralFinding {
        signal: signal.to_owned(),
        path: path.to_owned(),
        detail,
        severity,
    }
}

/// Count the entries of an object-valued key, if present.
fn object_len(root: &Value, key: &str) -> Option<usize> {
    root.get(key).and_then(|v| v.as_object()).map(|m| m.len())
}

// ---------------------------------------------------------------------------
// npm package.json
// ---------------------------------------------------------------------------

/// Reads an npm `package.json`.
pub struct NpmManifestDetector;

impl StructuralDetector for NpmManifestDetector {
    fn name(&self) -> &str {
        "npm_manifest"
    }

    fn applies(&self, value: &Value) -> bool {
        // A name plus any one of the fields only a package manifest carries.
        // `name` alone is far too common to key on.
        let Some(map) = value.as_object() else {
            return false;
        };
        map.contains_key("name")
            && [
                "version",
                "dependencies",
                "devDependencies",
                "scripts",
                "main",
            ]
            .iter()
            .any(|k| map.contains_key(*k))
    }

    fn inspect(&self, value: &Value) -> Vec<StructuralFinding> {
        let Some(map) = value.as_object() else {
            return Vec::new();
        };
        let mut out = Vec::new();

        // Install-time hooks — the highest-value signal in a manifest.
        if let Some(scripts) = map.get("scripts").and_then(|v| v.as_object()) {
            let mut hooks: Vec<&str> = NPM_INSTALL_HOOKS
                .iter()
                .filter(|h| scripts.contains_key(**h))
                .copied()
                .collect();
            hooks.sort_unstable();
            for hook in &hooks {
                out.push(finding(
                    "npm_install_hook",
                    &format!("$.scripts.{hook}"),
                    format!("`{hook}` runs automatically at install time"),
                    FindingSeverity::Concern,
                ));
            }
            out.push(finding(
                "npm_script_count",
                "$.scripts",
                format!("{} script(s) declared", scripts.len()),
                FindingSeverity::Info,
            ));
        }

        for (key, label) in [
            ("dependencies", "runtime"),
            ("devDependencies", "development"),
            ("optionalDependencies", "optional"),
            ("peerDependencies", "peer"),
        ] {
            if let Some(n) = object_len(value, key) {
                out.push(finding(
                    "npm_dependency_count",
                    &format!("$.{key}"),
                    format!("{n} {label} dependency/ies"),
                    FindingSeverity::Info,
                ));
            }
        }

        // Bundled dependencies ship code that never came from the registry, so
        // it is not covered by lockfile or audit tooling.
        if map.contains_key("bundledDependencies") || map.contains_key("bundleDependencies") {
            out.push(finding(
                "npm_bundled_dependencies",
                "$.bundledDependencies",
                "bundles dependencies, so shipped code bypasses registry resolution".to_owned(),
                FindingSeverity::Notable,
            ));
        }

        if map.contains_key("bin") {
            out.push(finding(
                "npm_bin_entry",
                "$.bin",
                "installs an executable onto PATH".to_owned(),
                FindingSeverity::Notable,
            ));
        }

        let missing: Vec<&str> = NPM_PROVENANCE
            .iter()
            .filter(|f| !map.contains_key(**f))
            .copied()
            .collect();
        if !missing.is_empty() {
            out.push(finding(
                "npm_missing_provenance",
                "$",
                format!("no {}", missing.join(", ")),
                FindingSeverity::Notable,
            ));
        }

        out
    }
}

// ---------------------------------------------------------------------------
// PyPI pyproject.toml / setup.cfg (parsed to JSON by the caller)
// ---------------------------------------------------------------------------

/// Reads a Python project manifest.
pub struct PythonManifestDetector;

impl StructuralDetector for PythonManifestDetector {
    fn name(&self) -> &str {
        "python_manifest"
    }

    fn applies(&self, value: &Value) -> bool {
        let Some(map) = value.as_object() else {
            return false;
        };
        map.contains_key("build-system")
            || map.contains_key("project")
            || map.contains_key("tool")
            || (map.contains_key("metadata") && map.contains_key("options"))
    }

    fn inspect(&self, value: &Value) -> Vec<StructuralFinding> {
        let Some(map) = value.as_object() else {
            return Vec::new();
        };
        let mut out = Vec::new();

        // `entry_points` / `console_scripts` install executables; `cmdclass`
        // overrides setuptools commands, which is how a setup.py runs code at
        // install time.
        for (container, key, signal, detail, severity) in [
            (
                "project",
                "scripts",
                "python_console_script",
                "installs a console script onto PATH",
                FindingSeverity::Notable,
            ),
            (
                "project",
                "entry-points",
                "python_entry_points",
                "declares entry points",
                FindingSeverity::Info,
            ),
            (
                "project",
                "dependencies",
                "python_dependency_list",
                "declares runtime dependencies",
                FindingSeverity::Info,
            ),
        ] {
            if let Some(section) = map.get(container).and_then(|v| v.as_object()) {
                if let Some(found) = section.get(key) {
                    let count = match found {
                        Value::Object(m) => m.len(),
                        Value::Array(a) => a.len(),
                        _ => 1,
                    };
                    out.push(finding(
                        signal,
                        &format!("$.{container}.{key}"),
                        format!("{detail} ({count})"),
                        severity,
                    ));
                }
            }
        }

        if let Some(build) = map.get("build-system").and_then(|v| v.as_object()) {
            if let Some(backend) = build.get("build-backend").and_then(|v| v.as_str()) {
                out.push(finding(
                    "python_build_backend",
                    "$.build-system.build-backend",
                    format!("build backend `{backend}`"),
                    FindingSeverity::Info,
                ));
            }
            // A custom in-tree backend means arbitrary local code runs to build
            // the package.
            if build.contains_key("backend-path") {
                out.push(finding(
                    "python_in_tree_backend",
                    "$.build-system.backend-path",
                    "in-tree build backend: local code runs during build".to_owned(),
                    FindingSeverity::Concern,
                ));
            }
        }

        // setup.cfg style.
        if let Some(options) = map.get("options").and_then(|v| v.as_object()) {
            if options.contains_key("cmdclass") {
                out.push(finding(
                    "python_cmdclass_override",
                    "$.options.cmdclass",
                    "overrides setuptools commands, which can run code at install time".to_owned(),
                    FindingSeverity::Concern,
                ));
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Cargo.toml
// ---------------------------------------------------------------------------

/// Reads a Cargo manifest.
pub struct CargoManifestDetector;

impl StructuralDetector for CargoManifestDetector {
    fn name(&self) -> &str {
        "cargo_manifest"
    }

    fn applies(&self, value: &Value) -> bool {
        value
            .as_object()
            .is_some_and(|m| m.contains_key("package") && !m.contains_key("build-system"))
    }

    fn inspect(&self, value: &Value) -> Vec<StructuralFinding> {
        let Some(map) = value.as_object() else {
            return Vec::new();
        };
        let mut out = Vec::new();

        if let Some(package) = map.get("package").and_then(|v| v.as_object()) {
            // A build script is arbitrary code compiled and run at build time.
            if let Some(build) = package.get("build") {
                let target = build.as_str().unwrap_or("build.rs");
                out.push(finding(
                    "cargo_build_script",
                    "$.package.build",
                    format!("build script `{target}` compiles and runs at build time"),
                    FindingSeverity::Concern,
                ));
            }
            for field in ["repository", "license", "description"] {
                if !package.contains_key(field) {
                    out.push(finding(
                        "cargo_missing_provenance",
                        &format!("$.package.{field}"),
                        format!("no {field}"),
                        FindingSeverity::Notable,
                    ));
                }
            }
        }

        for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(n) = object_len(value, key) {
                out.push(finding(
                    "cargo_dependency_count",
                    &format!("$.{key}"),
                    format!("{n} entry/ies under {key}"),
                    FindingSeverity::Info,
                ));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(s)
    }

    fn signals(findings: &[StructuralFinding]) -> Vec<&str> {
        findings.iter().map(|f| f.signal.as_str()).collect()
    }

    // -- npm ---------------------------------------------------------------

    #[test]
    fn npm_detects_install_hooks() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"name":"x","version":"1.0.0","scripts":{"preinstall":"node i.js"}}"#)?;
        assert!(NpmManifestDetector.applies(&v));
        let f = NpmManifestDetector.inspect(&v);
        assert!(signals(&f).contains(&"npm_install_hook"));
        let hook = f
            .iter()
            .find(|x| x.signal == "npm_install_hook")
            .ok_or("missing hook")?;
        assert_eq!(hook.severity, FindingSeverity::Concern);
        assert_eq!(hook.path, "$.scripts.preinstall");
        Ok(())
    }

    /// Every automatically-executed hook must be reported, not just the first.
    #[test]
    fn npm_reports_every_install_hook() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"name":"x","version":"1.0.0","scripts":
                {"preinstall":"a","postinstall":"b","test":"c"}}"#,
        )?;
        let f = NpmManifestDetector.inspect(&v);
        let hooks: Vec<&str> = f
            .iter()
            .filter(|x| x.signal == "npm_install_hook")
            .map(|x| x.path.as_str())
            .collect();
        assert_eq!(hooks, vec!["$.scripts.postinstall", "$.scripts.preinstall"]);
        Ok(())
    }

    /// A build-only script is not an install hook and must not be flagged as one.
    #[test]
    fn npm_ignores_non_install_scripts() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"name":"x","version":"1.0.0","scripts":{"build":"tsc","test":"jest"}}"#)?;
        let f = NpmManifestDetector.inspect(&v);
        assert!(!signals(&f).contains(&"npm_install_hook"));
        assert!(signals(&f).contains(&"npm_script_count"));
        Ok(())
    }

    #[test]
    fn npm_reports_missing_provenance() -> Result<(), Box<dyn std::error::Error>> {
        let bare = json(r#"{"name":"x","version":"1.0.0","main":"i.js"}"#)?;
        let f = NpmManifestDetector.inspect(&bare);
        let missing = f
            .iter()
            .find(|x| x.signal == "npm_missing_provenance")
            .ok_or("expected missing-provenance finding")?;
        assert!(missing.detail.contains("repository"));

        let full = json(
            r#"{"name":"x","version":"1.0.0","repository":"git+https://e.com/x",
                "homepage":"https://e.com","bugs":"https://e.com/i","author":"a","license":"MIT"}"#,
        )?;
        let full_findings = NpmManifestDetector.inspect(&full);
        assert!(!signals(&full_findings).contains(&"npm_missing_provenance"));
        Ok(())
    }

    #[test]
    fn npm_counts_dependencies() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"name":"x","version":"1.0.0","dependencies":{"a":"^1","b":"^2"},
                "devDependencies":{"c":"^3"}}"#,
        )?;
        let f = NpmManifestDetector.inspect(&v);
        let counts: Vec<&String> = f
            .iter()
            .filter(|x| x.signal == "npm_dependency_count")
            .map(|x| &x.detail)
            .collect();
        assert!(counts.iter().any(|d| d.contains("2 runtime")));
        assert!(counts.iter().any(|d| d.contains("1 development")));
        Ok(())
    }

    #[test]
    fn npm_flags_bundled_and_bin() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"name":"x","version":"1.0.0","bin":{"x":"cli.js"},
                "bundledDependencies":["a"]}"#,
        )?;
        let f = NpmManifestDetector.inspect(&v);
        let s = signals(&f);
        assert!(s.contains(&"npm_bin_entry"));
        assert!(s.contains(&"npm_bundled_dependencies"));
        Ok(())
    }

    /// `name` alone is far too common to key on — an arbitrary record with a
    /// name must not be treated as a manifest.
    #[test]
    fn npm_does_not_claim_arbitrary_objects() -> Result<(), Box<dyn std::error::Error>> {
        assert!(!NpmManifestDetector.applies(&json(r#"{"name":"Alice","age":30}"#)?));
        assert!(!NpmManifestDetector.applies(&json(r#"{"version":"1.0.0"}"#)?));
        assert!(!NpmManifestDetector.applies(&json("[1,2,3]")?));
        Ok(())
    }

    // -- python ------------------------------------------------------------

    #[test]
    fn python_flags_in_tree_backend() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"build-system":{"requires":["setuptools"],
                "build-backend":"local","backend-path":["."]}}"#,
        )?;
        assert!(PythonManifestDetector.applies(&v));
        let f = PythonManifestDetector.inspect(&v);
        let s = signals(&f);
        assert!(s.contains(&"python_in_tree_backend"));
        assert!(s.contains(&"python_build_backend"));
        let concern = f
            .iter()
            .find(|x| x.signal == "python_in_tree_backend")
            .ok_or("missing")?;
        assert_eq!(concern.severity, FindingSeverity::Concern);
        Ok(())
    }

    #[test]
    fn python_flags_cmdclass_override() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"metadata":{"name":"x"},"options":{"cmdclass":{"install":"Custom"}}}"#)?;
        let f = PythonManifestDetector.inspect(&v);
        assert!(signals(&f).contains(&"python_cmdclass_override"));
        Ok(())
    }

    #[test]
    fn python_reports_scripts_and_dependencies() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"project":{"name":"x","scripts":{"cli":"x:main"},
                "dependencies":["requests","urllib3"]}}"#,
        )?;
        let f = PythonManifestDetector.inspect(&v);
        let s = signals(&f);
        assert!(s.contains(&"python_console_script"));
        assert!(s.contains(&"python_dependency_list"));
        Ok(())
    }

    // -- cargo -------------------------------------------------------------

    #[test]
    fn cargo_flags_build_script() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"package":{"name":"x","version":"0.1.0","build":"build.rs"}}"#)?;
        assert!(CargoManifestDetector.applies(&v));
        let f = CargoManifestDetector.inspect(&v);
        let build = f
            .iter()
            .find(|x| x.signal == "cargo_build_script")
            .ok_or("missing build script finding")?;
        assert_eq!(build.severity, FindingSeverity::Concern);
        assert!(build.detail.contains("build.rs"));
        Ok(())
    }

    #[test]
    fn cargo_reports_missing_provenance() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"package":{"name":"x","version":"0.1.0"}}"#)?;
        let f = CargoManifestDetector.inspect(&v);
        let missing: Vec<&String> = f
            .iter()
            .filter(|x| x.signal == "cargo_missing_provenance")
            .map(|x| &x.detail)
            .collect();
        assert_eq!(missing.len(), 3, "repository, license, description");
        Ok(())
    }

    /// A pyproject.toml also has a `[package]`-like shape in some tools; the
    /// Cargo detector must not claim documents that declare a build-system.
    #[test]
    fn cargo_does_not_claim_pyproject() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(r#"{"package":{"name":"x"},"build-system":{"requires":[]}}"#)?;
        assert!(!CargoManifestDetector.applies(&v));
        Ok(())
    }

    #[test]
    fn detectors_are_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let v = json(
            r#"{"name":"x","version":"1.0.0","scripts":{"postinstall":"a","preinstall":"b"},
                "dependencies":{"a":"^1"}}"#,
        )?;
        let a = NpmManifestDetector.inspect(&v);
        let b = NpmManifestDetector.inspect(&v);
        assert_eq!(a, b);
        Ok(())
    }
}
