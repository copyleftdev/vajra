//! Governance tools — read-only analysis of contributor patterns and project health.

use std::process::Command;

use serde_json::{json, Value};

use super::{run_vajra, vajra_bin, CallToolResult, Tool, ToolError};

// ─────────────────────────────────────────────────
// vajra_governance
// ─────────────────────────────────────────────────

pub struct GovernanceTool;

impl Tool for GovernanceTool {
    fn name(&self) -> &'static str {
        "vajra_governance"
    }

    fn description(&self) -> &'static str {
        "Governance metrics — bus factor, merge equity, contributor churn, \
         and concentration analysis for commit/PR data."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file (commits, PRs, etc.)"
                },
                "author_field": {
                    "type": "string",
                    "description": "JSONPath to the author field (e.g. '$.author')",
                    "default": "$.author"
                },
                "time_field": {
                    "type": "string",
                    "description": "JSONPath to the timestamp field (e.g. '$.date')",
                    "default": "$.date"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Governance",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("governance").arg(path).arg("--quiet");

        if let Some(af) = params["author_field"].as_str() {
            cmd.arg("--author-field").arg(af);
        }
        if let Some(tf) = params["time_field"].as_str() {
            cmd.arg("--time-field").arg(tf);
        }

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_core_team
// ─────────────────────────────────────────────────

pub struct CoreTeamTool;

impl Tool for CoreTeamTool {
    fn name(&self) -> &'static str {
        "vajra_core_team"
    }

    fn description(&self) -> &'static str {
        "Detect core team members from commit patterns — identifies sustained \
         contributors, their specializations, and contribution patterns."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to commit data file"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Core Team",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("core-team").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_score
// ─────────────────────────────────────────────────

pub struct ScoreTool;

impl Tool for ScoreTool {
    fn name(&self) -> &'static str {
        "vajra_score"
    }

    fn description(&self) -> &'static str {
        "Automated health scoring with letter grades — computes composite \
         scores across activity, governance, code quality, and community dimensions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "author_field": {
                    "type": "string",
                    "description": "JSONPath to the author field",
                    "default": "$.author"
                },
                "time_field": {
                    "type": "string",
                    "description": "JSONPath to the timestamp field",
                    "default": "$.date"
                },
                "message_field": {
                    "type": "string",
                    "description": "JSONPath to the commit message field",
                    "default": "$.message"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Score",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("score").arg(path).arg("--quiet");

        if let Some(af) = params["author_field"].as_str() {
            cmd.arg("--author-field").arg(af);
        }
        if let Some(tf) = params["time_field"].as_str() {
            cmd.arg("--time-field").arg(tf);
        }
        if let Some(mf) = params["message_field"].as_str() {
            cmd.arg("--message-field").arg(mf);
        }

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}
