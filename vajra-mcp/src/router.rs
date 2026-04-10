//! JSON-RPC method router.
//!
//! Dispatches incoming JSON-RPC messages to the appropriate handler:
//! initialize, tools/list, tools/call, ping, notifications.

use serde_json::Value;

use crate::protocol::*;
use crate::tools::ToolRegistry;

pub struct Router {
    tools: ToolRegistry,
}

impl Router {
    pub fn new(tools: ToolRegistry) -> Self {
        Self { tools }
    }

    pub fn handle(&self, msg: JsonRpcMessage) -> Option<JsonRpcResponse> {
        // Notifications (no id) don't get responses
        if msg.is_notification() {
            return None;
        }

        // Requests require an id
        let id = msg.id?;

        let method = match msg.method.as_deref() {
            Some(m) => m,
            None => {
                return Some(JsonRpcResponse::error(
                    id,
                    JsonRpcError::invalid_request("missing method"),
                ));
            }
        };

        let params = msg.params.unwrap_or(Value::Object(serde_json::Map::new()));

        let result = match method {
            "initialize" => self.handle_initialize(),
            "ping" => self.handle_ping(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(params),
            other => {
                return Some(JsonRpcResponse::error(
                    id,
                    JsonRpcError::method_not_found(other),
                ));
            }
        };

        match result {
            Ok(value) => Some(JsonRpcResponse::success(id, value)),
            Err(err) => Some(JsonRpcResponse::error(id, err)),
        }
    }

    fn handle_initialize(&self) -> Result<Value, JsonRpcError> {
        let result = InitializeResult {
            protocol_version: "2025-03-26",
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: false,
                }),
            },
            server_info: ServerInfo {
                name: "vajra".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Vajra is a deterministic structural-analysis engine for data files \
                 (JSON, YAML, CSV, NDJSON, Markdown, PDF, source code, git repos). \
                 It provides structural inspection, statistical summaries, anomaly \
                 detection, fingerprinting, drift detection, governance metrics, \
                 health scoring, and full GitHub repository auditing. \
                 Start with `vajra_inspect` to understand a file's structure, \
                 then use `vajra_stats`, `vajra_anomalies`, or `vajra_essence` \
                 for deeper analysis. Use `vajra_audit` for one-command GitHub \
                 repository health reports."
                    .into(),
            ),
        };

        serde_json::to_value(result).map_err(|e| JsonRpcError::internal(&e.to_string()))
    }

    fn handle_ping(&self) -> Result<Value, JsonRpcError> {
        serde_json::to_value(PingResult {}).map_err(|e| JsonRpcError::internal(&e.to_string()))
    }

    fn handle_tools_list(&self) -> Result<Value, JsonRpcError> {
        let result = ToolListResult {
            tools: self.tools.descriptors(),
        };
        serde_json::to_value(result).map_err(|e| JsonRpcError::internal(&e.to_string()))
    }

    fn handle_tools_call(&self, params: Value) -> Result<Value, JsonRpcError> {
        let call: CallToolParams = serde_json::from_value(params)
            .map_err(|e| JsonRpcError::invalid_params(&e.to_string()))?;

        let tool = self
            .tools
            .get(&call.name)
            .ok_or_else(|| JsonRpcError::invalid_params(&format!("unknown tool: {}", call.name)))?;

        let tool_params = call
            .arguments
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let result = tool.call(tool_params);

        match result {
            Ok(call_result) => serde_json::to_value(call_result)
                .map_err(|e| JsonRpcError::internal(&e.to_string())),
            Err(tool_err) => {
                let call_result = CallToolResult::error(tool_err.to_string());
                serde_json::to_value(call_result)
                    .map_err(|e| JsonRpcError::internal(&e.to_string()))
            }
        }
    }
}
