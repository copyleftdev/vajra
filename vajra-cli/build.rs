//! Embed the commit the binary was built from.
//!
//! `vajra --version` reported `0.5.0` for every build over 36 commits and eight
//! output-schema changes, so two binaries four months apart were
//! indistinguishable by the only identifier the tool exposed. For a tool whose
//! contract is "same input + same config = byte-identical output", determinism
//! is only useful if you can say *which* deterministic function you ran.
//! See #104.
//!
//! The commit **date** is used, never the build time. A build timestamp would
//! make the binary itself non-reproducible — two builds of identical source
//! would differ — which is the opposite of what this embeds it for.

use std::process::Command;

fn main() {
    // Re-run when the checked-out commit changes. `.git/HEAD` covers checkouts
    // and `.git/logs/HEAD` covers commits on the current branch; both are
    // cheap to stat and neither exists in a packaged source tree, where the
    // directives are simply inert.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/logs/HEAD");
    // An explicit override for packagers and reproducible-build systems that
    // build from a tarball with no git metadata.
    println!("cargo:rerun-if-env-changed=VAJRA_BUILD_COMMIT");

    let commit = std::env::var("VAJRA_BUILD_COMMIT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| git(&["rev-parse", "--short=7", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());

    let date = git(&["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".to_owned());

    // A dirty tree is worth saying out loud: the commit alone would misdescribe
    // the binary. Only reported when git answered; absence of git is not
    // evidence of cleanliness.
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|s| !s.trim().is_empty());

    let suffix = if dirty { "-dirty" } else { "" };
    println!("cargo:rustc-env=VAJRA_BUILD_COMMIT={commit}{suffix}");
    println!("cargo:rustc-env=VAJRA_BUILD_DATE={date}");
}

/// Run a git command, returning trimmed stdout when it succeeds.
///
/// Every failure mode — git absent, not a checkout, command error — collapses
/// to `None` so a build outside a repository still succeeds.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}
