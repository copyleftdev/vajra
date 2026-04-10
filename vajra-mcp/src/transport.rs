//! Stdio transport — line-delimited JSON-RPC over stdin/stdout.
//!
//! Each message is a single JSON line. Blank lines are ignored.

use std::io::{self, BufRead, Write};

use crate::protocol::{JsonRpcError, JsonRpcMessage};
use crate::router::Router;

/// Run the MCP server loop, reading JSON-RPC from stdin and writing to stdout.
pub fn run_stdio(router: Router) {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = stdin.lock();

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                let _ = write_parse_error(&mut stdout, &e.to_string());
                continue;
            }
        };

        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        let msg: JsonRpcMessage = match serde_json::from_str(&trimmed) {
            Ok(m) => m,
            Err(e) => {
                let err_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": JsonRpcError::parse_error(&e.to_string()).code,
                        "message": format!("Parse error: {e}")
                    }
                });
                let _ = write_json(&mut stdout, &err_resp);
                continue;
            }
        };

        if let Some(response) = router.handle(msg) {
            match serde_json::to_value(&response) {
                Ok(val) => {
                    let _ = write_json(&mut stdout, &val);
                }
                Err(e) => {
                    let _ = write_parse_error(&mut stdout, &e.to_string());
                }
            }
        }
    }
}

fn write_json(stdout: &mut io::Stdout, value: &serde_json::Value) -> io::Result<()> {
    let s = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    stdout.write_all(s.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

fn write_parse_error(stdout: &mut io::Stdout, detail: &str) -> io::Result<()> {
    let err = serde_json::json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": -32700,
            "message": format!("Parse error: {detail}")
        }
    });
    write_json(stdout, &err)
}
