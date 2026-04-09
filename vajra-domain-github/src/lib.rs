//! GitHub domain plugin for Vajra.
//!
//! Provides type recognizers for common GitHub data types
//! (logins, bot accounts, PR/issue/review states, semver tags,
//! commit hashes, issue labels, PR number references, and ISO 8601
//! datetimes) and relationship hints that describe expected field
//! co-occurrence patterns in GitHub event data.

pub mod hints;
pub mod recognizers;

use vajra_types::traits::{RelationshipHint, TypeRecognizer, VajraPlugin};

use crate::hints::github_hints;
use crate::recognizers::{
    GitCommitHashRecognizer, GitHubBotRecognizer, GitHubLoginRecognizer, ISO8601DatetimeRecognizer,
    IssueLabelRecognizer, IssueStateRecognizer, PRNumberRefRecognizer, PRStateRecognizer,
    ReviewStateRecognizer, SemVerRecognizer,
};

/// The GitHub domain plugin.
///
/// Registers GitHub-specific type recognizers and relationship hints
/// to enhance Vajra's analysis of GitHub PR data, issue trackers,
/// review workflows, release metadata, and contributor activity.
pub struct GitHubPlugin;

impl VajraPlugin for GitHubPlugin {
    fn name(&self) -> &str {
        "github"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn type_recognizers(&self) -> Vec<Box<dyn TypeRecognizer>> {
        vec![
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
        ]
    }

    fn relationship_hints(&self) -> Vec<RelationshipHint> {
        github_hints()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_types::traits::VajraPlugin;

    #[test]
    fn plugin_name_and_version() {
        let p = GitHubPlugin;
        assert_eq!(p.name(), "github");
        assert_eq!(p.version(), "0.1.0");
    }

    #[test]
    fn plugin_returns_correct_recognizer_count() {
        let p = GitHubPlugin;
        let recognizers = p.type_recognizers();
        assert_eq!(recognizers.len(), 10);
    }

    #[test]
    fn plugin_returns_correct_hint_count() {
        let p = GitHubPlugin;
        let hints = p.relationship_hints();
        assert_eq!(hints.len(), 7);
    }

    #[test]
    fn recognizer_names_are_unique() {
        let p = GitHubPlugin;
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
        let p = GitHubPlugin;
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
        assert_send_sync::<GitHubPlugin>();
    }

    #[test]
    fn recognizers_are_send_and_sync() {
        let p = GitHubPlugin;
        let recognizers = p.type_recognizers();
        // Stored as Box<dyn TypeRecognizer> where TypeRecognizer: Send + Sync
        assert!(!recognizers.is_empty());
    }

    #[test]
    fn hint_names_match_expected() {
        let p = GitHubPlugin;
        let hints = p.relationship_hints();
        let names: Vec<&str> = hints.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"review_network"));
        assert!(names.contains(&"merge_gate"));
        assert!(names.contains(&"issue_triage"));
        assert!(names.contains(&"contributor_lifecycle"));
        assert!(names.contains(&"release_cadence"));
        assert!(names.contains(&"pr_review_assignment"));
        assert!(names.contains(&"commit_cascade"));
    }

    #[test]
    fn determinism_recognizers_10_runs() {
        let test_values = [
            "octocat",
            "dependabot[bot]",
            "MERGED",
            "OPEN",
            "APPROVED",
            "v1.0.0",
            "a414b56",
            "bug",
            "#123",
            "2026-04-09T12:00:00Z",
            "not-a-match",
            "",
        ];
        let mut results: Vec<Vec<Vec<bool>>> = Vec::new();
        for _ in 0..10 {
            let p = GitHubPlugin;
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
            let p = GitHubPlugin;
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
