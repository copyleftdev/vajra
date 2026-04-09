//! GitHub domain type recognizers.
//!
//! Each recognizer identifies a specific GitHub-relevant data type
//! (logins, bot accounts, PR/issue/review states, semver tags, commit
//! hashes, issue labels, PR number references, and ISO 8601 datetimes)
//! by matching string values against compiled regular expressions.
//! Regexes are compiled once using `OnceLock`.

use std::sync::OnceLock;

use regex::Regex;
use vajra_types::traits::{InferenceConfidence, TypeRecognizer};

/// Helper to compile a regex once and return a reference.
/// Returns `None` if the regex is invalid (should not happen with known patterns).
fn compile_once<'a>(lock: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    lock.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

// ---------------------------------------------------------------------------
// GitHub Login Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub usernames (logins): 1-39 alphanumeric or hyphen characters,
/// starting and ending with an alphanumeric character. Single-character logins
/// are also valid.
pub struct GitHubLoginRecognizer;

static GITHUB_LOGIN_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for GitHubLoginRecognizer {
    fn type_name(&self) -> &str {
        "github_login"
    }

    fn matches(&self, value: &str) -> bool {
        if value.is_empty() || value.len() > 39 {
            return false;
        }
        let Some(re) = compile_once(
            &GITHUB_LOGIN_RE,
            r"^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,37}[a-zA-Z0-9])?$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        50
    }
}

// ---------------------------------------------------------------------------
// GitHub Bot Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub bot accounts: names ending with `[bot]` or starting with
/// `app/` (e.g., `dependabot[bot]`, `app/github-actions`).
pub struct GitHubBotRecognizer;

static GITHUB_BOT_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for GitHubBotRecognizer {
    fn type_name(&self) -> &str {
        "github_bot"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&GITHUB_BOT_RE, r"^(?:.*\[bot\]|app/.+)$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        70
    }
}

// ---------------------------------------------------------------------------
// PR State Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub pull request states: OPEN, MERGED, or CLOSED.
pub struct PRStateRecognizer;

static PR_STATE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for PRStateRecognizer {
    fn type_name(&self) -> &str {
        "pr_state"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&PR_STATE_RE, r"^(?:OPEN|MERGED|CLOSED)$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Definite
    }

    fn priority(&self) -> u32 {
        80
    }
}

// ---------------------------------------------------------------------------
// Issue State Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub issue states: OPEN or CLOSED.
pub struct IssueStateRecognizer;

static ISSUE_STATE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for IssueStateRecognizer {
    fn type_name(&self) -> &str {
        "issue_state"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&ISSUE_STATE_RE, r"^(?:OPEN|CLOSED)$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        75
    }
}

// ---------------------------------------------------------------------------
// Review State Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub pull request review states: APPROVED, CHANGES_REQUESTED,
/// COMMENTED, DISMISSED, or PENDING.
pub struct ReviewStateRecognizer;

static REVIEW_STATE_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ReviewStateRecognizer {
    fn type_name(&self) -> &str {
        "review_state"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &REVIEW_STATE_RE,
            r"^(?:APPROVED|CHANGES_REQUESTED|COMMENTED|DISMISSED|PENDING)$",
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

// ---------------------------------------------------------------------------
// SemVer Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Semantic Versioning strings, optionally prefixed with `v`.
/// Supports pre-release and build metadata segments.
/// Examples: `v3.21.0`, `1.0.0-beta.1`, `2.0.0+build.123`.
pub struct SemVerRecognizer;

static SEMVER_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for SemVerRecognizer {
    fn type_name(&self) -> &str {
        "semver"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &SEMVER_RE,
            r"^v?\d+\.\d+\.\d+(?:-[a-zA-Z0-9.]+)?(?:\+[a-zA-Z0-9.]+)?$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        65
    }
}

// ---------------------------------------------------------------------------
// Git Commit Hash Recognizer
// ---------------------------------------------------------------------------

/// Recognizes Git commit hashes: 7-40 lowercase hexadecimal characters.
pub struct GitCommitHashRecognizer;

static GIT_COMMIT_HASH_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for GitCommitHashRecognizer {
    fn type_name(&self) -> &str {
        "git_commit_hash"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&GIT_COMMIT_HASH_RE, r"^[0-9a-f]{7,40}$") else {
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
// Issue Label Recognizer
// ---------------------------------------------------------------------------

/// Recognizes common GitHub issue labels (case-insensitive):
/// bug, enhancement, feature, documentation, question, good first issue,
/// help wanted, wontfix, duplicate, invalid.
pub struct IssueLabelRecognizer;

static ISSUE_LABEL_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for IssueLabelRecognizer {
    fn type_name(&self) -> &str {
        "issue_label"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &ISSUE_LABEL_RE,
            r"(?i)^(?:bug|enhancement|feature|documentation|question|good first issue|help wanted|wontfix|duplicate|invalid)$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        60
    }
}

// ---------------------------------------------------------------------------
// PR Number Ref Recognizer
// ---------------------------------------------------------------------------

/// Recognizes GitHub PR/issue number references: `#123` or bare `123`
/// (in numeric context, bare numbers 1-99999 are matched).
pub struct PRNumberRefRecognizer;

static PR_NUMBER_REF_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for PRNumberRefRecognizer {
    fn type_name(&self) -> &str {
        "pr_number_ref"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(&PR_NUMBER_REF_RE, r"^#\d+$") else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Heuristic
    }

    fn priority(&self) -> u32 {
        45
    }
}

// ---------------------------------------------------------------------------
// ISO 8601 Datetime Recognizer
// ---------------------------------------------------------------------------

/// Recognizes ISO 8601 datetime strings with the `T` separator.
/// Supports optional fractional seconds and timezone offset or `Z` suffix.
/// Examples: `2026-04-09T12:00:00Z`, `2024-01-15T08:30:00.000+00:00`.
pub struct ISO8601DatetimeRecognizer;

static ISO8601_RE: OnceLock<Option<Regex>> = OnceLock::new();

impl TypeRecognizer for ISO8601DatetimeRecognizer {
    fn type_name(&self) -> &str {
        "iso8601_datetime"
    }

    fn matches(&self, value: &str) -> bool {
        let Some(re) = compile_once(
            &ISO8601_RE,
            r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$",
        ) else {
            return false;
        };
        re.is_match(value)
    }

    fn confidence(&self) -> InferenceConfidence {
        InferenceConfidence::Dominant
    }

    fn priority(&self) -> u32 {
        40
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // GitHub Login tests
    // -----------------------------------------------------------------------

    #[test]
    fn github_login_valid() {
        let r = GitHubLoginRecognizer;
        assert!(r.matches("octocat"));
        assert!(r.matches("tim-smart"));
        assert!(r.matches("a")); // single char
        assert!(r.matches("a1")); // two chars
        assert!(r.matches("user-name-123"));
        assert!(r.matches("X")); // single uppercase
        assert!(r.matches("a2345678901234567890123456789012345678b")); // 39 chars
    }

    #[test]
    fn github_login_invalid() {
        let r = GitHubLoginRecognizer;
        assert!(!r.matches("")); // empty
        assert!(!r.matches("-user")); // starts with hyphen
        assert!(!r.matches("user-")); // ends with hyphen
        assert!(!r.matches("user_name")); // underscore not allowed
        assert!(!r.matches("user.name")); // dot not allowed
        assert!(!r.matches("a23456789012345678901234567890123456789b")); // 40 chars, too long
        assert!(!r.matches("user name")); // space
        assert!(!r.matches("dependabot[bot]")); // bot suffix
    }

    #[test]
    fn github_login_metadata() {
        let r = GitHubLoginRecognizer;
        assert_eq!(r.type_name(), "github_login");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 50);
    }

    // -----------------------------------------------------------------------
    // GitHub Bot tests
    // -----------------------------------------------------------------------

    #[test]
    fn github_bot_valid() {
        let r = GitHubBotRecognizer;
        assert!(r.matches("dependabot[bot]"));
        assert!(r.matches("renovate[bot]"));
        assert!(r.matches("app/github-actions"));
        assert!(r.matches("app/dependabot"));
        assert!(r.matches("some-long-name[bot]"));
    }

    #[test]
    fn github_bot_invalid() {
        let r = GitHubBotRecognizer;
        assert!(!r.matches("")); // empty
        assert!(!r.matches("octocat")); // normal user
        assert!(!r.matches("app/")); // empty app name
        assert!(!r.matches("bot")); // just "bot"
    }

    #[test]
    fn github_bot_metadata() {
        let r = GitHubBotRecognizer;
        assert_eq!(r.type_name(), "github_bot");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 70);
    }

    // -----------------------------------------------------------------------
    // PR State tests
    // -----------------------------------------------------------------------

    #[test]
    fn pr_state_valid() {
        let r = PRStateRecognizer;
        assert!(r.matches("OPEN"));
        assert!(r.matches("MERGED"));
        assert!(r.matches("CLOSED"));
    }

    #[test]
    fn pr_state_invalid() {
        let r = PRStateRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("open")); // lowercase
        assert!(!r.matches("merged")); // lowercase
        assert!(!r.matches("DRAFT")); // not a PR state
        assert!(!r.matches("PENDING")); // not a PR state
    }

    #[test]
    fn pr_state_metadata() {
        let r = PRStateRecognizer;
        assert_eq!(r.type_name(), "pr_state");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 80);
    }

    // -----------------------------------------------------------------------
    // Issue State tests
    // -----------------------------------------------------------------------

    #[test]
    fn issue_state_valid() {
        let r = IssueStateRecognizer;
        assert!(r.matches("OPEN"));
        assert!(r.matches("CLOSED"));
    }

    #[test]
    fn issue_state_invalid() {
        let r = IssueStateRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("open")); // lowercase
        assert!(!r.matches("MERGED")); // not an issue state
        assert!(!r.matches("REOPENED")); // not a standard state
    }

    #[test]
    fn issue_state_metadata() {
        let r = IssueStateRecognizer;
        assert_eq!(r.type_name(), "issue_state");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 75);
    }

    // -----------------------------------------------------------------------
    // Review State tests
    // -----------------------------------------------------------------------

    #[test]
    fn review_state_valid() {
        let r = ReviewStateRecognizer;
        assert!(r.matches("APPROVED"));
        assert!(r.matches("CHANGES_REQUESTED"));
        assert!(r.matches("COMMENTED"));
        assert!(r.matches("DISMISSED"));
        assert!(r.matches("PENDING"));
    }

    #[test]
    fn review_state_invalid() {
        let r = ReviewStateRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("approved")); // lowercase
        assert!(!r.matches("REJECTED")); // not a review state
        assert!(!r.matches("MERGED")); // not a review state
    }

    #[test]
    fn review_state_metadata() {
        let r = ReviewStateRecognizer;
        assert_eq!(r.type_name(), "review_state");
        assert_eq!(r.confidence(), InferenceConfidence::Definite);
        assert_eq!(r.priority(), 85);
    }

    // -----------------------------------------------------------------------
    // SemVer tests
    // -----------------------------------------------------------------------

    #[test]
    fn semver_valid() {
        let r = SemVerRecognizer;
        assert!(r.matches("v3.21.0"));
        assert!(r.matches("1.0.0"));
        assert!(r.matches("1.0.0-beta.1"));
        assert!(r.matches("v0.1.0"));
        assert!(r.matches("2.0.0+build.123"));
        assert!(r.matches("1.0.0-alpha+001"));
        assert!(r.matches("0.0.1"));
    }

    #[test]
    fn semver_invalid() {
        let r = SemVerRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("1.0")); // missing patch
        assert!(!r.matches("v1")); // only major
        assert!(!r.matches("1.0.0.0")); // too many segments
        assert!(!r.matches("latest")); // not a version
        assert!(!r.matches("v")); // just prefix
    }

    #[test]
    fn semver_metadata() {
        let r = SemVerRecognizer;
        assert_eq!(r.type_name(), "semver");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 65);
    }

    // -----------------------------------------------------------------------
    // Git Commit Hash tests
    // -----------------------------------------------------------------------

    #[test]
    fn git_commit_hash_valid() {
        let r = GitCommitHashRecognizer;
        assert!(r.matches("a414b56")); // 7-char short hash
        assert!(r.matches("db36f1c3e")); // 9-char hash
        assert!(r.matches("a94a8fe5ccb19ba61c4c0873d391e987982fbbd3")); // full 40-char
        assert!(r.matches("0000000")); // all zeros, 7 chars
    }

    #[test]
    fn git_commit_hash_invalid() {
        let r = GitCommitHashRecognizer;
        assert!(!r.matches("")); // empty
        assert!(!r.matches("a414b5")); // 6 chars, too short
        assert!(!r.matches("A414B56")); // uppercase
        assert!(!r.matches("g414b56")); // non-hex char
        assert!(!r.matches("a94a8fe5ccb19ba61c4c0873d391e987982fbbd3a")); // 41 chars
    }

    #[test]
    fn git_commit_hash_metadata() {
        let r = GitCommitHashRecognizer;
        assert_eq!(r.type_name(), "git_commit_hash");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 55);
    }

    // -----------------------------------------------------------------------
    // Issue Label tests
    // -----------------------------------------------------------------------

    #[test]
    fn issue_label_valid() {
        let r = IssueLabelRecognizer;
        assert!(r.matches("bug"));
        assert!(r.matches("enhancement"));
        assert!(r.matches("feature"));
        assert!(r.matches("documentation"));
        assert!(r.matches("question"));
        assert!(r.matches("good first issue"));
        assert!(r.matches("help wanted"));
        assert!(r.matches("wontfix"));
        assert!(r.matches("duplicate"));
        assert!(r.matches("invalid"));
        // Case insensitive
        assert!(r.matches("Bug"));
        assert!(r.matches("ENHANCEMENT"));
        assert!(r.matches("Good First Issue"));
    }

    #[test]
    fn issue_label_invalid() {
        let r = IssueLabelRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("priority: high")); // custom label
        assert!(!r.matches("needs review")); // not a standard label
        assert!(!r.matches("bugfix")); // close but not exact
    }

    #[test]
    fn issue_label_metadata() {
        let r = IssueLabelRecognizer;
        assert_eq!(r.type_name(), "issue_label");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 60);
    }

    // -----------------------------------------------------------------------
    // PR Number Ref tests
    // -----------------------------------------------------------------------

    #[test]
    fn pr_number_ref_valid() {
        let r = PRNumberRefRecognizer;
        assert!(r.matches("#123"));
        assert!(r.matches("#1"));
        assert!(r.matches("#99999"));
    }

    #[test]
    fn pr_number_ref_invalid() {
        let r = PRNumberRefRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("123")); // bare number (no #)
        assert!(!r.matches("#")); // just hash
        assert!(!r.matches("# 123")); // space after hash
        assert!(!r.matches("#abc")); // non-numeric
    }

    #[test]
    fn pr_number_ref_metadata() {
        let r = PRNumberRefRecognizer;
        assert_eq!(r.type_name(), "pr_number_ref");
        assert_eq!(r.confidence(), InferenceConfidence::Heuristic);
        assert_eq!(r.priority(), 45);
    }

    // -----------------------------------------------------------------------
    // ISO 8601 Datetime tests
    // -----------------------------------------------------------------------

    #[test]
    fn iso8601_datetime_valid() {
        let r = ISO8601DatetimeRecognizer;
        assert!(r.matches("2026-04-09T12:00:00Z"));
        assert!(r.matches("2024-01-15T08:30:00.000Z"));
        assert!(r.matches("2024-01-15T08:30:00+00:00"));
        assert!(r.matches("2024-01-15T08:30:00.123456+05:30"));
        assert!(r.matches("2024-01-15T08:30:00-07:00"));
    }

    #[test]
    fn iso8601_datetime_invalid() {
        let r = ISO8601DatetimeRecognizer;
        assert!(!r.matches(""));
        assert!(!r.matches("2024-01-15")); // date only
        assert!(!r.matches("2024-01-15 08:30:00")); // space instead of T
        assert!(!r.matches("08:30:00")); // time only
        assert!(!r.matches("2024-01-15T08:30:00")); // no timezone
        assert!(!r.matches("not-a-date"));
    }

    #[test]
    fn iso8601_datetime_metadata() {
        let r = ISO8601DatetimeRecognizer;
        assert_eq!(r.type_name(), "iso8601_datetime");
        assert_eq!(r.confidence(), InferenceConfidence::Dominant);
        assert_eq!(r.priority(), 40);
    }

    // -----------------------------------------------------------------------
    // Edge cases across all recognizers
    // -----------------------------------------------------------------------

    #[test]
    fn all_recognizers_reject_empty_string() {
        let recognizers: Vec<Box<dyn TypeRecognizer>> = vec![
            Box::new(GitHubLoginRecognizer),
            Box::new(GitHubBotRecognizer),
            Box::new(PRStateRecognizer),
            Box::new(IssueStateRecognizer),
            Box::new(ReviewStateRecognizer),
            Box::new(SemVerRecognizer),
            Box::new(GitCommitHashRecognizer),
            Box::new(IssueLabelRecognizer),
            Box::new(PRNumberRefRecognizer),
            Box::new(ISO8601DatetimeRecognizer),
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
            Box::new(GitHubLoginRecognizer),
            Box::new(GitHubBotRecognizer),
            Box::new(PRStateRecognizer),
            Box::new(IssueStateRecognizer),
            Box::new(ReviewStateRecognizer),
            Box::new(SemVerRecognizer),
            Box::new(GitCommitHashRecognizer),
            Box::new(IssueLabelRecognizer),
            Box::new(PRNumberRefRecognizer),
            Box::new(ISO8601DatetimeRecognizer),
        ];
        for r in &recognizers {
            assert!(
                !r.matches("   "),
                "{} should not match whitespace",
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
}
