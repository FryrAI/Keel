//! MCP stdio loop with telemetry instrumentation and session tracking.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;

use keel_core::config::KeelConfig;
use keel_core::telemetry::{self, TelemetryStore};

use crate::mcp::{self, SharedStore};

/// MCP session state for telemetry tracking.
struct McpSession {
    keel_dir: Option<PathBuf>,
    config: KeelConfig,
    no_telemetry: bool,
    client_name: Option<String>,
    tool_call_count: u32,
    session_start: Instant,
}

impl McpSession {
    fn new(keel_dir: Option<&Path>, no_telemetry: bool) -> Self {
        let config = keel_dir.map(KeelConfig::load).unwrap_or_default();
        Self {
            keel_dir: keel_dir.map(|d| d.to_path_buf()),
            config,
            no_telemetry,
            client_name: None,
            tool_call_count: 0,
            session_start: Instant::now(),
        }
    }

    /// Record a telemetry event for an MCP tool call.
    fn record_tool_event(&self, command: &str, duration_ms: u64, exit_code: i32, result: &Value) {
        if self.no_telemetry || !self.config.telemetry.enabled {
            return;
        }
        let keel_dir = match &self.keel_dir {
            Some(d) => d,
            None => return,
        };
        let db_path = keel_dir.join("telemetry.db");
        let store = match TelemetryStore::open(&db_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut event = telemetry::new_event(command, duration_ms, exit_code);
        event.client_name.clone_from(&self.client_name);

        // Extract error/warning counts from compile results
        if command == "mcp:compile" {
            if let Some(errors) = result.get("errors").and_then(|v| v.as_array()) {
                event.error_count = errors.len() as u32;
                for err in errors {
                    if let Some(code) = err.get("code").and_then(|c| c.as_str()) {
                        *event.error_codes.entry(code.to_string()).or_default() += 1;
                    }
                }
            }
            if let Some(warnings) = result.get("warnings").and_then(|v| v.as_array()) {
                event.warning_count = warnings.len() as u32;
                for warn in warnings {
                    if let Some(code) = warn.get("code").and_then(|c| c.as_str()) {
                        *event.error_codes.entry(code.to_string()).or_default() += 1;
                    }
                }
            }
        }

        let _ = store.record(&event);
    }

    /// Record a session summary event when the MCP connection closes.
    fn record_session_end(&self) {
        if self.no_telemetry || !self.config.telemetry.enabled {
            return;
        }
        let keel_dir = match &self.keel_dir {
            Some(d) => d,
            None => return,
        };
        let db_path = keel_dir.join("telemetry.db");
        let store = match TelemetryStore::open(&db_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let duration = self.session_start.elapsed().as_millis() as u64;
        let mut event = telemetry::new_event("mcp:session", duration, 0);
        event.client_name.clone_from(&self.client_name);
        // Convention: node_count is repurposed as tool_call_count for MCP session events.
        // See TelemetryEvent::node_count doc comment.
        event.node_count = self.tool_call_count;

        let _ = store.record(&event);
    }
}

/// Extract clientInfo.name from MCP initialize params.
fn extract_client_name(params: &Option<Value>) -> Option<String> {
    params
        .as_ref()?
        .get("clientInfo")?
        .get("name")?
        .as_str()
        .map(|s| s.to_string())
}

/// Run the MCP server loop, reading JSON-RPC from stdin and writing to stdout.
/// Instruments each tool call with telemetry recording.
pub fn run_stdio(
    store: SharedStore,
    db_path: Option<&str>,
    keel_dir: Option<&Path>,
    no_telemetry: bool,
) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let engine = mcp::create_shared_engine(db_path);
    let mut session = McpSession::new(keel_dir, no_telemetry);

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        // Peek at the request to extract method + params for telemetry
        let parsed: Option<Value> = serde_json::from_str(&line).ok();
        let method = parsed
            .as_ref()
            .and_then(|v| v.get("method"))
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let params = parsed.as_ref().and_then(|v| v.get("params").cloned());

        // Extract clientInfo on initialize
        if method == "initialize" {
            session.client_name = extract_client_name(&params);
        }

        let call_start = Instant::now();
        let response = mcp::process_line(&store, &engine, &line);
        let call_duration = call_start.elapsed().as_millis() as u64;

        if response.is_empty() {
            continue;
        }

        // Instrument keel/* tool calls with telemetry
        if method.starts_with("keel/") {
            session.tool_call_count += 1;

            let result_value: Value = serde_json::from_str(&response).unwrap_or(Value::Null);
            let exit_code = if result_value.get("error").is_some() {
                1
            } else {
                0
            };
            let inner_result = result_value.get("result").cloned().unwrap_or(Value::Null);

            let command = format!("mcp:{}", method.strip_prefix("keel/").unwrap_or(&method));
            session.record_tool_event(&command, call_duration, exit_code, &inner_result);
        }

        let mut out = stdout.lock();
        writeln!(out, "{}", response)?;
        out.flush()?;
    }

    // Session ended (stdin EOF) — record summary
    session.record_session_end();

    Ok(())
}
