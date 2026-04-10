//! Pipeline tools — may modify filesystem (ingest, report generation, audit).

use std::process::Command;

use serde_json::{json, Value};

use super::{run_vajra, vajra_bin, CallToolResult, Tool, ToolError};

// ─────────────────────────────────────────────────
// vajra_ingest_github
// ─────────────────────────────────────────────────

pub struct IngestGithubTool;

impl Tool for IngestGithubTool {
    fn name(&self) -> &'static str {
        "vajra_ingest_github"
    }

    fn description(&self) -> &'static str {
        "Ingest GitHub repository data (PRs, issues, commits, releases) via \
         the gh CLI. Writes JSON files to the output directory for downstream \
         analysis. Requires gh CLI to be authenticated."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "repo": {
                    "type": "string",
                    "description": "Owner/repo identifier (e.g. 'facebook/react')"
                },
                "output": {
                    "type": "string",
                    "description": "Output directory for ingested JSON files (default: current dir)"
                },
                "pr_limit": {
                    "type": "integer",
                    "description": "Maximum number of pull requests to fetch",
                    "default": 500
                },
                "issue_limit": {
                    "type": "integer",
                    "description": "Maximum number of issues to fetch",
                    "default": 500
                },
                "commit_limit": {
                    "type": "integer",
                    "description": "Maximum number of commits to fetch",
                    "default": 800
                }
            },
            "required": ["repo"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Ingest GitHub",
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let repo = params["repo"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("repo is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("ingest-github").arg(repo).arg("--quiet");

        if let Some(output) = params["output"].as_str() {
            cmd.arg("--output").arg(output);
        }
        if let Some(pr_limit) = params["pr_limit"].as_u64() {
            cmd.arg("--pr-limit").arg(pr_limit.to_string());
        }
        if let Some(issue_limit) = params["issue_limit"].as_u64() {
            cmd.arg("--issue-limit").arg(issue_limit.to_string());
        }
        if let Some(commit_limit) = params["commit_limit"].as_u64() {
            cmd.arg("--commit-limit").arg(commit_limit.to_string());
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_report
// ─────────────────────────────────────────────────

pub struct ReportTool;

impl Tool for ReportTool {
    fn name(&self) -> &'static str {
        "vajra_report"
    }

    fn description(&self) -> &'static str {
        "Generate an HTML analysis report from pre-computed JSON files \
         (stats.json, anomalies.json, etc.). Produces a self-contained \
         HTML file with charts and tables."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input_dir": {
                    "type": "string",
                    "description": "Directory containing analysis JSON files"
                },
                "title": {
                    "type": "string",
                    "description": "Report title",
                    "default": "Analysis Report"
                },
                "output": {
                    "type": "string",
                    "description": "Output HTML file path",
                    "default": "report.html"
                },
                "repo_name": {
                    "type": "string",
                    "description": "Repository name (e.g. 'facebook/react')"
                }
            },
            "required": ["input_dir", "title", "output"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Report",
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let input_dir = params["input_dir"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("input_dir is required".into()))?;
        let title = params["title"].as_str().unwrap_or("Analysis Report");
        let output = params["output"].as_str().unwrap_or("report.html");

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("report")
            .arg(input_dir)
            .arg("--title")
            .arg(title)
            .arg("--output")
            .arg(output)
            .arg("--quiet");

        if let Some(repo_name) = params["repo_name"].as_str() {
            cmd.arg("--repo-name").arg(repo_name);
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_audit
// ─────────────────────────────────────────────────

pub struct AuditTool;

impl Tool for AuditTool {
    fn name(&self) -> &'static str {
        "vajra_audit"
    }

    fn description(&self) -> &'static str {
        "One-command audit: ingest GitHub data, run full analysis, and \
         generate an HTML report. Requires gh CLI authentication. \
         Produces a comprehensive project health report."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "repo": {
                    "type": "string",
                    "description": "Repository URL or owner/repo (e.g. 'owner/repo')"
                },
                "output": {
                    "type": "string",
                    "description": "Output HTML report path (default: '{owner}-{repo}-report.html')"
                },
                "commit_limit": {
                    "type": "integer",
                    "description": "Maximum number of commits to fetch",
                    "default": 800
                },
                "pr_limit": {
                    "type": "integer",
                    "description": "Maximum number of pull requests to fetch",
                    "default": 500
                },
                "issue_limit": {
                    "type": "integer",
                    "description": "Maximum number of issues to fetch",
                    "default": 500
                }
            },
            "required": ["repo"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Audit",
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let repo = params["repo"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("repo is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("audit").arg(repo).arg("--quiet");

        if let Some(output) = params["output"].as_str() {
            cmd.arg("--output").arg(output);
        }
        if let Some(commit_limit) = params["commit_limit"].as_u64() {
            cmd.arg("--commit-limit").arg(commit_limit.to_string());
        }
        if let Some(pr_limit) = params["pr_limit"].as_u64() {
            cmd.arg("--pr-limit").arg(pr_limit.to_string());
        }
        if let Some(issue_limit) = params["issue_limit"].as_u64() {
            cmd.arg("--issue-limit").arg(issue_limit.to_string());
        }

        run_vajra(&mut cmd)
    }
}
