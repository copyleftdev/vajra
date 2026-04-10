//! Compare tools — cross-repo comparison and benchmarking.

use std::process::Command;

use serde_json::{json, Value};

use super::{run_vajra, vajra_bin, CallToolResult, Tool, ToolError};

// ─────────────────────────────────────────────────
// vajra_compare
// ─────────────────────────────────────────────────

pub struct CompareTool;

impl Tool for CompareTool {
    fn name(&self) -> &'static str {
        "vajra_compare"
    }

    fn description(&self) -> &'static str {
        "Cross-repo comparison — multi-project benchmarking across governance, \
         activity, code quality, and community metrics. Requires two or more \
         data files to compare."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 2,
                    "description": "Two or more file paths to compare"
                },
                "labels": {
                    "type": "string",
                    "description": "Comma-separated labels for each dataset (default: filenames)"
                },
                "author_field": {
                    "type": "string",
                    "description": "JSONPath to the author field",
                    "default": "$.author"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                }
            },
            "required": ["paths"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Compare",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let paths = params["paths"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidParams("paths array is required".into()))?;

        if paths.len() < 2 {
            return Err(ToolError::InvalidParams(
                "paths must contain at least two files".into(),
            ));
        }

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("compare");

        for p in paths {
            if let Some(s) = p.as_str() {
                cmd.arg(s);
            }
        }

        cmd.arg("--quiet");

        if let Some(labels) = params["labels"].as_str() {
            cmd.arg("--labels").arg(labels);
        }
        if let Some(af) = params["author_field"].as_str() {
            cmd.arg("--author-field").arg(af);
        }

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}
