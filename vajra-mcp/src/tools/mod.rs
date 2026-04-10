//! Tool trait, registry, and shared helpers.
//!
//! Every MCP tool implements the `Tool` trait. The `ToolRegistry` collects
//! them and provides lookup by name for the router.

pub mod analyze;
pub mod compare;
pub mod governance;
pub mod pipeline;

use std::collections::HashMap;
use std::fmt;
use std::process::Command;

use serde_json::Value;

use crate::protocol::{CallToolResult, ToolDescriptor};

// ─────────────────────────────────────────────────
// Tool trait
// ─────────────────────────────────────────────────

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> Value;
    fn annotations(&self) -> Option<Value> {
        None
    }
    fn call(&self, params: Value) -> Result<CallToolResult, ToolError>;
}

// ─────────────────────────────────────────────────
// Tool errors
// ─────────────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
pub enum ToolError {
    InvalidParams(String),
    Execution(String),
    Internal(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParams(msg) => write!(f, "Invalid params: {msg}"),
            Self::Execution(msg) => write!(f, "Execution error: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

// ─────────────────────────────────────────────────
// Registry
// ─────────────────────────────────────────────────

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    order: Vec<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        self.order.push(name.clone());
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.order
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|t| ToolDescriptor {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
                annotations: t.annotations(),
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────
// Register all tools
// ─────────────────────────────────────────────────

pub fn register_all() -> ToolRegistry {
    let mut reg = ToolRegistry::new();

    // Analysis tools (read-only, idempotent)
    reg.register(Box::new(analyze::InspectTool));
    reg.register(Box::new(analyze::StatsTool));
    reg.register(Box::new(analyze::AnomaliesTool));
    reg.register(Box::new(analyze::FingerprintTool));
    reg.register(Box::new(analyze::EssenceTool));
    reg.register(Box::new(analyze::InvariantsTool));
    reg.register(Box::new(analyze::QueryTool));
    reg.register(Box::new(analyze::DriftTool));
    reg.register(Box::new(analyze::CascadeTool));
    reg.register(Box::new(analyze::BatchTool));
    reg.register(Box::new(analyze::ClusterTool));

    // Governance tools (read-only, idempotent)
    reg.register(Box::new(governance::GovernanceTool));
    reg.register(Box::new(governance::CoreTeamTool));
    reg.register(Box::new(governance::ScoreTool));

    // Compare tools (read-only, idempotent)
    reg.register(Box::new(compare::CompareTool));

    // Pipeline tools (may modify filesystem)
    reg.register(Box::new(pipeline::IngestGithubTool));
    reg.register(Box::new(pipeline::ReportTool));
    reg.register(Box::new(pipeline::AuditTool));

    // Meta
    reg.register(Box::new(analyze::ProfilesTool));

    reg
}

// ─────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────

/// Run a vajra CLI command and return a `CallToolResult`.
pub fn run_vajra(cmd: &mut Command) -> Result<CallToolResult, ToolError> {
    let output = cmd
        .output()
        .map_err(|e| ToolError::Execution(format!("failed to execute vajra: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(CallToolResult::text(stdout))
    } else {
        let msg = if stderr.is_empty() { stdout } else { stderr };
        Ok(CallToolResult::error(msg))
    }
}

/// Resolve the vajra binary name. Checks `VAJRA_BIN` env var first,
/// then falls back to `"vajra"` on PATH.
pub fn vajra_bin() -> String {
    std::env::var("VAJRA_BIN").unwrap_or_else(|_| "vajra".into())
}
