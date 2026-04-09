//! Git log ingestion adapter.
//!
//! Shells out to the `git` CLI to extract commit metadata from a local
//! repository, then converts it to a JSON array of commit objects that
//! can be fed through the standard Vajra analysis pipeline.

use std::path::{Path, PathBuf};
use std::process::Command;

use vajra_types::{Document, VajraError};

use crate::parse::parse_str;

/// Configuration for git log extraction.
#[derive(Debug, Clone)]
pub struct GitLogConfig {
    /// Maximum number of commits to extract.
    pub limit: usize,
    /// Branch or revision to read (defaults to HEAD).
    pub branch: Option<String>,
}

impl Default for GitLogConfig {
    fn default() -> Self {
        Self {
            limit: 500,
            branch: None,
        }
    }
}

/// Load git log from a repository directory and return it as a [`Document`].
///
/// The returned document wraps a JSON array of commit objects, each with:
/// - `hash` (abbreviated commit hash)
/// - `author_name`
/// - `author_email`
/// - `date` (ISO 8601)
/// - `subject` (first line of commit message)
///
/// # Errors
///
/// Returns [`VajraError::Io`] if the directory does not exist or git is not
/// found. Returns [`VajraError::Config`] if the path is not a git repository.
/// Returns [`VajraError::Parse`] if the git output cannot be parsed.
pub fn load_git_log(repo_path: &Path, config: &GitLogConfig) -> Result<Document, VajraError> {
    validate_repo(repo_path)?;

    let raw_output = run_git_log(repo_path, config)?;

    let commits = parse_git_output(&raw_output)?;

    let json_text = serde_json::to_string(&commits).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("failed to serialize git log to JSON: {e}"),
        source_path: Some(repo_path.to_path_buf()),
    })?;

    parse_str(&json_text)
}

/// Validate that the given path is an existing directory containing a `.git/`
/// subdirectory, and that `git` is available on PATH.
fn validate_repo(repo_path: &Path) -> Result<(), VajraError> {
    if !repo_path.exists() {
        return Err(VajraError::Io {
            path: repo_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("directory does not exist: {}", repo_path.display()),
            ),
        });
    }

    if !repo_path.is_dir() {
        return Err(VajraError::Io {
            path: repo_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not a directory: {}", repo_path.display()),
            ),
        });
    }

    // Check for .git subdirectory or a bare repo (has HEAD file)
    let git_dir = repo_path.join(".git");
    let head_file = repo_path.join("HEAD");
    if !git_dir.exists() && !head_file.exists() {
        return Err(VajraError::Config {
            message: format!("not a git repository: {}", repo_path.display()),
        });
    }

    // Verify git is on PATH
    let git_check = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|e| VajraError::Io {
            path: PathBuf::from("git"),
            source: std::io::Error::new(
                e.kind(),
                "git command not found; ensure git is installed and on PATH",
            ),
        })?;

    if !git_check.status.success() {
        return Err(VajraError::Config {
            message: "git command failed to report its version".to_string(),
        });
    }

    Ok(())
}

/// Run `git log` with a null-byte-delimited format string and return stdout.
fn run_git_log(repo_path: &Path, config: &GitLogConfig) -> Result<String, VajraError> {
    // Use %x00 (null byte) as field delimiter to avoid issues with special
    // characters in commit messages, author names, etc.
    // Format: hash\0author_name\0author_email\0date_iso\0subject\0\n
    let format_str = "%h%x00%an%x00%ae%x00%aI%x00%s%x00";

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo_path)
        .arg("log")
        .arg(format!("--format={format_str}"))
        .arg(format!("-n{}", config.limit))
        .arg("--no-merges");

    if let Some(ref branch) = config.branch {
        cmd.arg(branch);
    }

    let output = cmd.output().map_err(|e| VajraError::Io {
        path: repo_path.to_path_buf(),
        source: std::io::Error::new(e.kind(), format!("failed to execute git log: {e}")),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VajraError::Config {
            message: format!(
                "git log failed for {}: {}",
                repo_path.display(),
                stderr.trim()
            ),
        });
    }

    String::from_utf8(output.stdout).map_err(|e| VajraError::Parse {
        byte_offset: 0,
        message: format!("git log output is not valid UTF-8: {e}"),
        source_path: Some(repo_path.to_path_buf()),
    })
}

/// Parse the null-byte-delimited git log output into a JSON array of commit
/// objects.
fn parse_git_output(raw: &str) -> Result<Vec<serde_json::Value>, VajraError> {
    let mut commits = Vec::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\0').collect();

        // We expect 5 fields plus a trailing empty element from the final \0
        if fields.len() < 5 {
            continue;
        }

        let commit = serde_json::json!({
            "hash": fields[0],
            "author_name": fields[1],
            "author_email": fields[2],
            "date": fields[3],
            "subject": fields[4],
        });

        commits.push(commit);
    }

    Ok(commits)
}

/// Detect whether a path looks like a git repository (contains `.git/`).
#[must_use]
pub fn is_git_repo(path: &Path) -> bool {
    path.is_dir() && (path.join(".git").exists() || path.join("HEAD").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_git_output_basic() {
        let raw =
            "abc1234\0Alice\0alice@example.com\02024-01-15T10:30:00+00:00\0Initial commit\0\n\
                   def5678\0Bob\0bob@example.com\02024-01-16T11:00:00+00:00\0Add feature\0\n";
        let commits = parse_git_output(raw).unwrap_or_default();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0]["hash"], "abc1234");
        assert_eq!(commits[0]["author_name"], "Alice");
        assert_eq!(commits[0]["author_email"], "alice@example.com");
        assert_eq!(commits[0]["date"], "2024-01-15T10:30:00+00:00");
        assert_eq!(commits[0]["subject"], "Initial commit");
        assert_eq!(commits[1]["hash"], "def5678");
        assert_eq!(commits[1]["author_name"], "Bob");
    }

    #[test]
    fn parse_git_output_empty() {
        let commits = parse_git_output("").unwrap_or_default();
        assert!(commits.is_empty());
    }

    #[test]
    fn parse_git_output_whitespace_only() {
        let commits = parse_git_output("  \n  \n").unwrap_or_default();
        assert!(commits.is_empty());
    }

    #[test]
    fn parse_git_output_special_chars_in_subject() {
        let raw = "abc1234\0Alice\0alice@example.com\02024-01-15T10:30:00+00:00\0Fix: handle \"quotes\" & <brackets>\0\n";
        let commits = parse_git_output(raw).unwrap_or_default();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["subject"], "Fix: handle \"quotes\" & <brackets>");
    }

    #[test]
    fn parse_git_output_skips_malformed_lines() {
        let raw = "too\0few\0fields\n\
                   abc1234\0Alice\0alice@example.com\02024-01-15T10:30:00+00:00\0Good commit\0\n";
        let commits = parse_git_output(raw).unwrap_or_default();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["hash"], "abc1234");
    }

    #[test]
    fn is_git_repo_detects_nonexistent() {
        assert!(!is_git_repo(Path::new("/nonexistent/path")));
    }

    #[test]
    fn validate_repo_nonexistent_dir() {
        let result = validate_repo(Path::new("/nonexistent/path/to/repo"));
        assert!(result.is_err());
    }

    #[test]
    fn validate_repo_not_git() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let result = validate_repo(dir.path());
        assert!(result.is_err());
        let err = result.err().ok_or("expected error")?;
        let msg = format!("{err}");
        assert!(
            msg.contains("not a git repository"),
            "expected 'not a git repository' in: {msg}"
        );
        Ok(())
    }

    #[test]
    fn default_config() {
        let config = GitLogConfig::default();
        assert_eq!(config.limit, 500);
        assert!(config.branch.is_none());
    }

    #[test]
    fn load_git_log_nonexistent() {
        let config = GitLogConfig::default();
        let result = load_git_log(Path::new("/nonexistent/repo"), &config);
        assert!(result.is_err());
    }
}
