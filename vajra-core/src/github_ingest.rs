//! Native GitHub API ingestion adapter.
//!
//! Shells out to the `gh` CLI (GitHub CLI) to fetch repository metadata
//! (pull requests, issues, releases) and to `git` for commit history.
//! Produces flattened JSON files suitable for Vajra analysis.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vajra_types::VajraError;

/// Configuration for GitHub repository ingestion.
#[derive(Debug, Clone)]
pub struct GitHubIngestConfig {
    /// Owner/repo identifier, e.g. "facebook/react".
    pub owner_repo: String,
    /// Directory to write output files into.
    pub output_dir: PathBuf,
    /// Maximum number of pull requests to fetch.
    pub pr_limit: usize,
    /// Maximum number of issues to fetch.
    pub issue_limit: usize,
    /// Maximum number of commits to fetch.
    pub commit_limit: usize,
}

impl Default for GitHubIngestConfig {
    fn default() -> Self {
        Self {
            owner_repo: String::new(),
            output_dir: PathBuf::from("."),
            pr_limit: 500,
            issue_limit: 500,
            commit_limit: 800,
        }
    }
}

/// Summary of what was ingested.
#[derive(Debug)]
pub struct IngestResult {
    /// Number of commits written.
    pub commits: usize,
    /// Number of pull requests written.
    pub pull_requests: usize,
    /// Number of issues written.
    pub issues: usize,
    /// Number of releases written.
    pub releases: usize,
    /// Output directory path.
    pub output_dir: PathBuf,
}

/// Run the full GitHub ingestion pipeline.
///
/// Fetches commits (via shallow clone + git log), pull requests, issues,
/// and releases for the given repository, writing each as a JSON array
/// file in `config.output_dir`.
///
/// # Errors
///
/// Returns [`VajraError::Config`] if `gh` is not installed, authentication
/// is missing, or the repository is not found.
/// Returns [`VajraError::Io`] if the output directory cannot be created.
pub fn ingest_github(config: &GitHubIngestConfig) -> Result<IngestResult, VajraError> {
    validate_gh_cli()?;
    ensure_output_dir(&config.output_dir)?;

    let commits = ingest_commits(config)?;
    let prs = ingest_prs(config)?;
    let issues = ingest_issues(config)?;
    let releases = ingest_releases(config)?;

    Ok(IngestResult {
        commits,
        pull_requests: prs,
        issues,
        releases,
        output_dir: config.output_dir.clone(),
    })
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Verify that the `gh` CLI is available and authenticated.
fn validate_gh_cli() -> Result<(), VajraError> {
    let output = Command::new("gh")
        .arg("--version")
        .output()
        .map_err(|_| VajraError::Config {
            message: "GitHub CLI (gh) not found. Install from https://cli.github.com".to_string(),
        })?;

    if !output.status.success() {
        return Err(VajraError::Config {
            message: "GitHub CLI (gh) not found. Install from https://cli.github.com".to_string(),
        });
    }

    // Check authentication status
    let auth_output = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map_err(|_| VajraError::Config {
            message: "gh auth login required. Run: gh auth login".to_string(),
        })?;

    if !auth_output.status.success() {
        return Err(VajraError::Config {
            message: "gh auth login required. Run: gh auth login".to_string(),
        });
    }

    Ok(())
}

/// Create the output directory if it does not exist.
fn ensure_output_dir(dir: &Path) -> Result<(), VajraError> {
    fs::create_dir_all(dir).map_err(|e| VajraError::Io {
        path: dir.to_path_buf(),
        source: e,
    })
}

// ---------------------------------------------------------------------------
// Commit ingestion (via shallow clone + git log)
// ---------------------------------------------------------------------------

fn ingest_commits(config: &GitHubIngestConfig) -> Result<usize, VajraError> {
    let clone_url = format!("https://github.com/{}.git", config.owner_repo);
    let clone_dir = config.output_dir.join("_repo_clone");

    // Shallow clone
    let clone_output = Command::new("git")
        .args([
            "clone",
            "--depth",
            &config.commit_limit.to_string(),
            "--single-branch",
            &clone_url,
        ])
        .arg(&clone_dir)
        .output()
        .map_err(|e| VajraError::Io {
            path: PathBuf::from("git"),
            source: std::io::Error::new(e.kind(), format!("failed to run git clone: {e}")),
        })?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        // Clean up partial clone
        let _ = fs::remove_dir_all(&clone_dir);
        return Err(classify_git_error(&stderr, &config.owner_repo));
    }

    // Run git log
    let log_config = crate::git::GitLogConfig {
        limit: config.commit_limit,
        branch: None,
    };
    let doc = crate::git::load_git_log(&clone_dir, &log_config)?;

    // The document contains a JSON array; count elements and write it out
    let value = doc.value();
    let count = value.as_array().map_or(0, Vec::len);

    let out_path = config.output_dir.join("commits.json");
    let pretty = serde_json::to_string_pretty(value).unwrap_or_else(|_| "[]".to_string());
    fs::write(&out_path, pretty).map_err(|e| VajraError::Io {
        path: out_path,
        source: e,
    })?;

    // Clean up clone directory
    let _ = fs::remove_dir_all(&clone_dir);

    Ok(count)
}

fn classify_git_error(stderr: &str, owner_repo: &str) -> VajraError {
    let lower = stderr.to_lowercase();
    if lower.contains("not found") || lower.contains("does not exist") {
        VajraError::Config {
            message: format!("repository not found: {owner_repo}"),
        }
    } else {
        VajraError::Config {
            message: format!("git clone failed for {owner_repo}: {}", stderr.trim()),
        }
    }
}

// ---------------------------------------------------------------------------
// Pull request ingestion
// ---------------------------------------------------------------------------

fn ingest_prs(config: &GitHubIngestConfig) -> Result<usize, VajraError> {
    let json_fields =
        "number,author,createdAt,title,state,mergedAt,additions,deletions,labels,closedAt";
    let raw = run_gh_command(&[
        "pr",
        "list",
        "--repo",
        &config.owner_repo,
        "--state",
        "all",
        "--limit",
        &config.pr_limit.to_string(),
        "--json",
        json_fields,
    ])?;

    let mut items: Vec<serde_json::Value> =
        serde_json::from_str(&raw).unwrap_or_else(|_| Vec::new());

    for item in &mut items {
        flatten_author(item);
        flatten_labels(item);
    }

    let count = items.len();
    let out_path = config.output_dir.join("prs.json");
    let pretty = serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string());
    fs::write(&out_path, pretty).map_err(|e| VajraError::Io {
        path: out_path,
        source: e,
    })?;

    Ok(count)
}

// ---------------------------------------------------------------------------
// Issue ingestion
// ---------------------------------------------------------------------------

fn ingest_issues(config: &GitHubIngestConfig) -> Result<usize, VajraError> {
    let json_fields = "number,author,createdAt,title,state,closedAt,labels,comments";
    let raw = run_gh_command(&[
        "issue",
        "list",
        "--repo",
        &config.owner_repo,
        "--state",
        "all",
        "--limit",
        &config.issue_limit.to_string(),
        "--json",
        json_fields,
    ])?;

    let mut items: Vec<serde_json::Value> =
        serde_json::from_str(&raw).unwrap_or_else(|_| Vec::new());

    for item in &mut items {
        flatten_author(item);
        flatten_labels(item);
        flatten_comments(item);
    }

    let count = items.len();
    let out_path = config.output_dir.join("issues.json");
    let pretty = serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string());
    fs::write(&out_path, pretty).map_err(|e| VajraError::Io {
        path: out_path,
        source: e,
    })?;

    Ok(count)
}

// ---------------------------------------------------------------------------
// Release ingestion
// ---------------------------------------------------------------------------

fn ingest_releases(config: &GitHubIngestConfig) -> Result<usize, VajraError> {
    let json_fields = "tagName,createdAt,name,isPrerelease";
    let raw = run_gh_command(&[
        "release",
        "list",
        "--repo",
        &config.owner_repo,
        "--limit",
        "50",
        "--json",
        json_fields,
    ])?;

    let items: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap_or_else(|_| Vec::new());

    let count = items.len();
    let out_path = config.output_dir.join("releases.json");
    let pretty = serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string());
    fs::write(&out_path, pretty).map_err(|e| VajraError::Io {
        path: out_path,
        source: e,
    })?;

    Ok(count)
}

// ---------------------------------------------------------------------------
// gh CLI runner
// ---------------------------------------------------------------------------

fn run_gh_command(args: &[&str]) -> Result<String, VajraError> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|_| VajraError::Config {
            message: "GitHub CLI (gh) not found. Install from https://cli.github.com".to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(classify_gh_error(&stderr, args));
    }

    String::from_utf8(output.stdout).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("gh output is not valid UTF-8: {e}"),
        source_path: None,
    })
}

fn classify_gh_error(stderr: &str, args: &[&str]) -> VajraError {
    let lower = stderr.to_lowercase();
    // Extract the repo from args if present
    let repo = args
        .windows(2)
        .find(|w| w[0] == "--repo")
        .map(|w| w[1])
        .unwrap_or("unknown");

    if lower.contains("could not resolve") || lower.contains("not found") {
        VajraError::Config {
            message: format!("repository not found: {repo}"),
        }
    } else if lower.contains("auth") || lower.contains("login") {
        VajraError::Config {
            message: "gh auth login required. Run: gh auth login".to_string(),
        }
    } else {
        VajraError::Config {
            message: format!("gh command failed: {}", stderr.trim()),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON flattening helpers
// ---------------------------------------------------------------------------

/// Flatten `{"author": {"login": "user"}}` to `{"author": "user"}`.
pub fn flatten_author(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        if let Some(author) = obj.get("author").cloned() {
            if let Some(login) = author.get("login").and_then(|v| v.as_str()) {
                obj.insert(
                    "author".to_string(),
                    serde_json::Value::String(login.to_string()),
                );
            }
        }
    }
}

/// Flatten `{"labels": [{"name": "bug"}, {"name": "high"}]}` to `{"labels": "bug,high"}`.
pub fn flatten_labels(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        if let Some(labels) = obj.get("labels").cloned() {
            if let Some(arr) = labels.as_array() {
                let names: Vec<&str> = arr
                    .iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                    .collect();
                obj.insert(
                    "labels".to_string(),
                    serde_json::Value::String(names.join(",")),
                );
            }
        }
    }
}

/// Replace `{"comments": [...]}` with `{"comment_count": N}`.
pub fn flatten_comments(value: &mut serde_json::Value) {
    if let Some(obj) = value.as_object_mut() {
        if let Some(comments) = obj.get("comments").cloned() {
            if let Some(arr) = comments.as_array() {
                let count = arr.len();
                obj.remove("comments");
                obj.insert(
                    "comment_count".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(count)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Flattening tests
    // -----------------------------------------------------------------------

    #[test]
    fn flatten_author_extracts_login() {
        let mut v = serde_json::json!({
            "number": 1,
            "author": {"login": "tim-smart"},
            "title": "Fix bug"
        });
        flatten_author(&mut v);
        assert_eq!(v["author"], "tim-smart");
    }

    #[test]
    fn flatten_author_noop_when_already_string() {
        let mut v = serde_json::json!({"author": "alice"});
        flatten_author(&mut v);
        assert_eq!(v["author"], "alice");
    }

    #[test]
    fn flatten_author_noop_when_missing() {
        let mut v = serde_json::json!({"title": "no author"});
        flatten_author(&mut v);
        assert!(v.get("author").is_none());
    }

    #[test]
    fn flatten_author_null_login() {
        let mut v = serde_json::json!({"author": {"login": null}});
        flatten_author(&mut v);
        // login is null, not a string, so author remains as the object
        assert!(v["author"].is_object());
    }

    #[test]
    fn flatten_labels_joins_names() {
        let mut v = serde_json::json!({
            "labels": [{"name": "bug"}, {"name": "high"}]
        });
        flatten_labels(&mut v);
        assert_eq!(v["labels"], "bug,high");
    }

    #[test]
    fn flatten_labels_empty_array() {
        let mut v = serde_json::json!({"labels": []});
        flatten_labels(&mut v);
        assert_eq!(v["labels"], "");
    }

    #[test]
    fn flatten_labels_single() {
        let mut v = serde_json::json!({"labels": [{"name": "enhancement"}]});
        flatten_labels(&mut v);
        assert_eq!(v["labels"], "enhancement");
    }

    #[test]
    fn flatten_labels_noop_when_missing() {
        let mut v = serde_json::json!({"title": "no labels"});
        flatten_labels(&mut v);
        assert!(v.get("labels").is_none());
    }

    #[test]
    fn flatten_comments_to_count() {
        let mut v = serde_json::json!({
            "comments": [
                {"body": "looks good"},
                {"body": "needs work"},
                {"body": "approved"}
            ]
        });
        flatten_comments(&mut v);
        assert!(v.get("comments").is_none());
        assert_eq!(v["comment_count"], 3);
    }

    #[test]
    fn flatten_comments_empty() {
        let mut v = serde_json::json!({"comments": []});
        flatten_comments(&mut v);
        assert_eq!(v["comment_count"], 0);
    }

    #[test]
    fn flatten_comments_noop_when_missing() {
        let mut v = serde_json::json!({"title": "no comments"});
        flatten_comments(&mut v);
        assert!(v.get("comment_count").is_none());
    }

    #[test]
    fn flatten_all_fields_combined() {
        let mut v = serde_json::json!({
            "number": 42,
            "author": {"login": "dependabot[bot]"},
            "labels": [{"name": "dependencies"}, {"name": "javascript"}],
            "comments": [{"body": "auto-merge"}, {"body": "merged"}],
            "title": "Bump lodash"
        });
        flatten_author(&mut v);
        flatten_labels(&mut v);
        flatten_comments(&mut v);

        assert_eq!(v["author"], "dependabot[bot]");
        assert_eq!(v["labels"], "dependencies,javascript");
        assert_eq!(v["comment_count"], 2);
        assert_eq!(v["number"], 42);
        assert_eq!(v["title"], "Bump lodash");
    }

    // -----------------------------------------------------------------------
    // Error classification tests
    // -----------------------------------------------------------------------

    #[test]
    fn classify_gh_error_not_found() {
        let err = classify_gh_error(
            "GraphQL: Could not resolve to a Repository",
            &["pr", "list", "--repo", "noexist/norepo"],
        );
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("repository not found"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn classify_gh_error_auth() {
        let err = classify_gh_error(
            "error: gh auth login required",
            &["pr", "list", "--repo", "owner/repo"],
        );
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("gh auth login required"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn classify_gh_error_generic() {
        let err = classify_gh_error(
            "some unexpected error occurred",
            &["pr", "list", "--repo", "owner/repo"],
        );
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("gh command failed"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn classify_git_error_not_found() {
        let err = classify_git_error(
            "fatal: repository 'https://github.com/noexist/norepo.git/' not found",
            "noexist/norepo",
        );
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("repository not found"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn classify_git_error_generic() {
        let err = classify_git_error("fatal: something else went wrong", "owner/repo");
        match err {
            VajraError::Config { message } => {
                assert!(
                    message.contains("git clone failed"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Directory creation test
    // -----------------------------------------------------------------------

    #[test]
    fn ensure_output_dir_creates_nested() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));
        let nested = tmp.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        ensure_output_dir(&nested).unwrap_or_else(|e| panic!("ensure_output_dir failed: {e}"));
        assert!(nested.is_dir());
    }

    #[test]
    fn ensure_output_dir_existing_is_ok() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir failed: {e}"));
        ensure_output_dir(tmp.path())
            .unwrap_or_else(|e| panic!("ensure_output_dir on existing dir failed: {e}"));
        assert!(tmp.path().is_dir());
    }

    // -----------------------------------------------------------------------
    // Config defaults
    // -----------------------------------------------------------------------

    #[test]
    fn default_config_values() {
        let cfg = GitHubIngestConfig::default();
        assert_eq!(cfg.pr_limit, 500);
        assert_eq!(cfg.issue_limit, 500);
        assert_eq!(cfg.commit_limit, 800);
        assert!(cfg.owner_repo.is_empty());
    }
}
