//! JSON-RPC 2.0 + MCP message types.
//!
//! Minimal implementation of the JSON-RPC 2.0 spec as used by MCP (2025-03-26).

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─────────────────────────────────────────────────
// Request ID — number or string per JSON-RPC 2.0
// ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    Text(String),
}

// ─────────────────────────────────────────────────
// Incoming message (request or notification)
// ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRpcMessage {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Id>,
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
}

impl JsonRpcMessage {
    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }
}

// ─────────────────────────────────────────────────
// Outgoing response
// ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: Id, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Id, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ─────────────────────────────────────────────────
// Error object
// ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

impl JsonRpcError {
    pub fn parse_error(detail: &str) -> Self {
        Self {
            code: PARSE_ERROR,
            message: format!("Parse error: {detail}"),
            data: None,
        }
    }

    #[allow(dead_code)]
    pub fn invalid_request(detail: &str) -> Self {
        Self {
            code: INVALID_REQUEST,
            message: format!("Invalid request: {detail}"),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: METHOD_NOT_FOUND,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    pub fn invalid_params(detail: &str) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: format!("Invalid params: {detail}"),
            data: None,
        }
    }

    pub fn internal(detail: &str) -> Self {
        Self {
            code: INTERNAL_ERROR,
            message: format!("Internal error: {detail}"),
            data: None,
        }
    }
}

// ─────────────────────────────────────────────────
// MCP types
// ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: &'static str,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
    pub instructions: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolListResult {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn text(text: String) -> Self {
        Self {
            content: vec![Content::Text { text }],
            is_error: false,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            content: vec![Content::Text { text: message }],
            is_error: true,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Serialize)]
pub struct PingResult {}
