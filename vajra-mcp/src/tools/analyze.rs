//! Analysis tools — read-only, idempotent operations on data files.

use std::process::Command;

use serde_json::{json, Value};

use super::{run_vajra, vajra_bin, CallToolResult, Tool, ToolError};

// ─────────────────────────────────────────────────
// vajra_inspect
// ─────────────────────────────────────────────────

pub struct InspectTool;

impl Tool for InspectTool {
    fn name(&self) -> &'static str {
        "vajra_inspect"
    }

    fn description(&self) -> &'static str {
        "Full structural analysis of a data file — paths, types, depth, \
         BLAKE3 fingerprints, domain hints. Supports JSON, YAML, CSV, NDJSON, \
         Markdown, PDF, source code, git repos."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to file or directory to analyze"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json",
                    "description": "Output format"
                },
                "input_format": {
                    "type": "string",
                    "enum": ["json", "ndjson", "yaml", "csv", "tsv", "markdown", "pdf", "cpuprofile", "strace", "source", "git"],
                    "description": "Force input format instead of auto-detecting"
                },
                "semantic_paths": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include semantic labels on tree-sitter nodes (source code input)"
                },
                "lang": {
                    "type": "string",
                    "description": "Source language (rust, python, go, javascript, etc.)"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Inspect",
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
        cmd.arg("inspect").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        if let Some(input_fmt) = params["input_format"].as_str() {
            cmd.arg("--input-format").arg(input_fmt);
        }
        if params["semantic_paths"].as_bool().unwrap_or(false) {
            cmd.arg("--semantic-paths");
        }
        if let Some(lang) = params["lang"].as_str() {
            cmd.arg("--lang").arg(lang);
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_stats
// ─────────────────────────────────────────────────

pub struct StatsTool;

impl Tool for StatsTool {
    fn name(&self) -> &'static str {
        "vajra_stats"
    }

    fn description(&self) -> &'static str {
        "Statistical summary of a data file — distributions, cardinalities, \
         numeric ranges, temporal patterns, and optional time-series windowing."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                },
                "window": {
                    "type": "string",
                    "enum": ["month", "week", "day"],
                    "description": "Time-series window granularity"
                },
                "time_field": {
                    "type": "string",
                    "description": "JSONPath to the timestamp field (e.g. '$.date')"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Stats",
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
        cmd.arg("stats").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        if let Some(window) = params["window"].as_str() {
            cmd.arg("--window").arg(window);
        }
        if let Some(tf) = params["time_field"].as_str() {
            cmd.arg("--time-field").arg(tf);
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_anomalies
// ─────────────────────────────────────────────────

pub struct AnomaliesTool;

impl Tool for AnomaliesTool {
    fn name(&self) -> &'static str {
        "vajra_anomalies"
    }

    fn description(&self) -> &'static str {
        "Anomaly detection — finds structural outliers, type inconsistencies, \
         unusual distributions, and suspicious patterns in data files."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
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
            "title": "Anomalies",
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
        cmd.arg("anomalies").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_fingerprint
// ─────────────────────────────────────────────────

pub struct FingerprintTool;

impl Tool for FingerprintTool {
    fn name(&self) -> &'static str {
        "vajra_fingerprint"
    }

    fn description(&self) -> &'static str {
        "Structural fingerprints — BLAKE3 content hashes, schema hashes, and \
         similarity hashes for comparing data files across versions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
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
            "title": "Fingerprint",
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
        cmd.arg("fingerprint").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_essence
// ─────────────────────────────────────────────────

pub struct EssenceTool;

impl Tool for EssenceTool {
    fn name(&self) -> &'static str {
        "vajra_essence"
    }

    fn description(&self) -> &'static str {
        "Generate concern-oriented essence — a compressed, priority-ranked \
         summary of a data file tailored to a specific profile (engineer, \
         security, executive, etc.) with optional token budget."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "profile": {
                    "type": "string",
                    "description": "Concern profile (engineer, security, executive, etc.)",
                    "default": "engineer"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                },
                "budget": {
                    "type": "integer",
                    "description": "Approximate max token budget for output"
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Essence",
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
        cmd.arg("essence").arg(path).arg("--quiet");

        if let Some(profile) = params["profile"].as_str() {
            cmd.arg("--profile").arg(profile);
        }

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        if let Some(budget) = params["budget"].as_u64() {
            cmd.arg("--budget").arg(budget.to_string());
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_invariants
// ─────────────────────────────────────────────────

pub struct InvariantsTool;

impl Tool for InvariantsTool {
    fn name(&self) -> &'static str {
        "vajra_invariants"
    }

    fn description(&self) -> &'static str {
        "Discover cross-field relationships — correlations, functional \
         dependencies, and invariants across fields in a dataset."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Maximum number of field pairs to consider",
                    "default": 50
                }
            },
            "required": ["path"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Invariants",
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
        cmd.arg("invariants").arg(path).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        if let Some(top_k) = params["top_k"].as_u64() {
            cmd.arg("--top-k").arg(top_k.to_string());
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_query
// ─────────────────────────────────────────────────

pub struct QueryTool;

impl Tool for QueryTool {
    fn name(&self) -> &'static str {
        "vajra_query"
    }

    fn description(&self) -> &'static str {
        "Run a query expression against a data file. Supports entropy, \
         cardinality, distribution, and statistical predicates \
         (e.g. 'entropy($.claims[*].status) > 0.5')."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "expression": {
                    "type": "string",
                    "description": "Query expression (e.g. 'entropy($.claims[*].status) > 0.5')"
                }
            },
            "required": ["path", "expression"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Query",
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
        let expression = params["expression"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("expression is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("query").arg(path).arg(expression).arg("--quiet");

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_drift
// ─────────────────────────────────────────────────

pub struct DriftTool;

impl Tool for DriftTool {
    fn name(&self) -> &'static str {
        "vajra_drift"
    }

    fn description(&self) -> &'static str {
        "Detect drift between two data files — structural changes, schema \
         evolution, value distribution shifts. Supports population-level \
         drift with --group-by."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "baseline": {
                    "type": "string",
                    "description": "Baseline file path"
                },
                "candidate": {
                    "type": "string",
                    "description": "Candidate file path to compare"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                },
                "group_by": {
                    "type": "string",
                    "description": "JSONPath field to partition records by for population-level drift"
                }
            },
            "required": ["baseline"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Drift",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let baseline = params["baseline"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("baseline is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("drift").arg(baseline);

        if let Some(candidate) = params["candidate"].as_str() {
            cmd.arg(candidate);
        }

        cmd.arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        if let Some(group_by) = params["group_by"].as_str() {
            cmd.arg("--group-by").arg(group_by);
        }

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_cascade
// ─────────────────────────────────────────────────

pub struct CascadeTool;

impl Tool for CascadeTool {
    fn name(&self) -> &'static str {
        "vajra_cascade"
    }

    fn description(&self) -> &'static str {
        "Detect temporal cause-effect chains in event data — finds cascading \
         failures, trigger-response patterns, and causal sequences."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to data file"
                },
                "entity_field": {
                    "type": "string",
                    "description": "Field identifying the entity (default: file)",
                    "default": "file"
                },
                "time_field": {
                    "type": "string",
                    "description": "Field identifying the timestamp (default: date)",
                    "default": "date"
                },
                "event_field": {
                    "type": "string",
                    "description": "Field identifying the event type (default: intent)",
                    "default": "intent"
                },
                "response_values": {
                    "type": "string",
                    "description": "Comma-separated response event values (default: fix,revert)",
                    "default": "fix,revert"
                }
            },
            "required": ["path", "entity_field", "time_field", "event_field"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Cascade",
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
        let entity_field = params["entity_field"].as_str().unwrap_or("file");
        let time_field = params["time_field"].as_str().unwrap_or("date");
        let event_field = params["event_field"].as_str().unwrap_or("intent");

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("cascade")
            .arg(path)
            .arg("--entity-field")
            .arg(entity_field)
            .arg("--time-field")
            .arg(time_field)
            .arg("--event-field")
            .arg(event_field)
            .arg("--quiet");

        if let Some(rv) = params["response_values"].as_str() {
            cmd.arg("--response-values").arg(rv);
        }

        cmd.arg("--format").arg("json");

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_batch
// ─────────────────────────────────────────────────

pub struct BatchTool;

impl Tool for BatchTool {
    fn name(&self) -> &'static str {
        "vajra_batch"
    }

    fn description(&self) -> &'static str {
        "Parallel batch analysis of all data files in a directory — runs \
         inspect + stats + anomalies across every file and returns aggregated results."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "directory": {
                    "type": "string",
                    "description": "Directory containing data files to analyze"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "text", "markdown", "compact-ai"],
                    "default": "json"
                }
            },
            "required": ["directory"]
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Batch",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }))
    }

    fn call(&self, params: Value) -> Result<CallToolResult, ToolError> {
        let directory = params["directory"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("directory is required".into()))?;

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("batch").arg(directory).arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_cluster
// ─────────────────────────────────────────────────

pub struct ClusterTool;

impl Tool for ClusterTool {
    fn name(&self) -> &'static str {
        "vajra_cluster"
    }

    fn description(&self) -> &'static str {
        "Cluster similar documents in a batch — groups files by structural \
         similarity using fingerprint-based distance metrics."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Array of file paths to cluster"
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
            "title": "Cluster",
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

        if paths.is_empty() {
            return Err(ToolError::InvalidParams(
                "paths must contain at least one file".into(),
            ));
        }

        let mut cmd = Command::new(vajra_bin());
        cmd.arg("cluster");

        for p in paths {
            if let Some(s) = p.as_str() {
                cmd.arg(s);
            }
        }

        cmd.arg("--quiet");

        let fmt = params["format"].as_str().unwrap_or("json");
        cmd.arg("--format").arg(fmt);

        run_vajra(&mut cmd)
    }
}

// ─────────────────────────────────────────────────
// vajra_profiles
// ─────────────────────────────────────────────────

pub struct ProfilesTool;

impl Tool for ProfilesTool {
    fn name(&self) -> &'static str {
        "vajra_profiles"
    }

    fn description(&self) -> &'static str {
        "List all available concern profiles (built-in and custom) for \
         the essence command."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn annotations(&self) -> Option<Value> {
        Some(json!({
            "title": "Profiles",
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }))
    }

    fn call(&self, _params: Value) -> Result<CallToolResult, ToolError> {
        let mut cmd = Command::new(vajra_bin());
        cmd.arg("profiles")
            .arg("--quiet")
            .arg("--format")
            .arg("json");

        run_vajra(&mut cmd)
    }
}
