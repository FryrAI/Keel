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

    /// Open the telemetry store and build a base event, or `None` when
    /// telemetry is disabled or unavailable. Shared prologue of both
    /// recorders.
    fn open_event(
        &self,
        command: &str,
        duration_ms: u64,
        exit_code: i32,
    ) -> Option<(TelemetryStore, telemetry::TelemetryEvent)> {
        if self.no_telemetry || !self.config.telemetry.enabled {
            return None;
        }
        let keel_dir = self.keel_dir.as_ref()?;
        let store = TelemetryStore::open(&keel_dir.join("telemetry.db")).ok()?;
        let mut event = telemetry::new_event(command, duration_ms, exit_code);
        event.client_name.clone_from(&self.client_name);
        Some((store, event))
    }

    /// Record a telemetry event for an MCP tool call.
    ///
    /// `response` is the full JSON-RPC response; compile is the only command
    /// whose payload is inspected, and the unwrap happens here so that rule
    /// lives in exactly one place.
    fn record_tool_event(&self, command: &str, duration_ms: u64, exit_code: i32, response: &Value) {
        let Some((store, mut event)) = self.open_event(command, duration_ms, exit_code) else {
            return;
        };

        // Extract error/warning counts from compile results
        if command == "mcp:compile" {
            let result = response
                .get("result")
                .map(crate::mcp_tools::tool_payload)
                .unwrap_or(Value::Null);
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
        let duration = self.session_start.elapsed().as_millis() as u64;
        let Some((store, mut event)) = self.open_event("mcp:session", duration, 0) else {
            return;
        };
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

/// Resolve which tool a request invokes, or `None` if it is not a tool call.
///
/// Spec-compliant MCP clients call every tool through `tools/call`, carrying
/// the tool name in `params.name`; the legacy convention passes the tool name
/// as the JSON-RPC method directly. Both must be recorded, otherwise telemetry
/// goes blind the moment a real client connects.
fn effective_tool_name(method: &str, params: &Option<Value>) -> Option<String> {
    match method {
        "tools/call" => params
            .as_ref()?
            .get("name")?
            .as_str()
            .map(|s| s.to_string()),
        m if m.starts_with("keel/") => Some(m.to_string()),
        _ => None,
    }
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
    let session = McpSession::new(keel_dir, no_telemetry);
    run_loop(&store, &engine, session, stdin.lock(), stdout.lock())
}

/// Drive the MCP request/response loop over arbitrary reader and writer.
///
/// Extracted from `run_stdio` so the loop — including telemetry wiring — is
/// exercisable in tests without touching the process's real stdin/stdout.
fn run_loop<R: BufRead, W: Write>(
    store: &SharedStore,
    engine: &mcp::SharedEngine,
    mut session: McpSession,
    reader: R,
    mut writer: W,
) -> io::Result<()> {
    for line in reader.lines() {
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
        let response = mcp::process_line(store, engine, &line);
        let call_duration = call_start.elapsed().as_millis() as u64;

        if response.is_empty() {
            continue;
        }

        // Instrument tool calls with telemetry, whether they arrived via
        // `tools/call` (spec-compliant clients) or as a direct method (legacy).
        if let Some(tool) = effective_tool_name(&method, &params) {
            session.tool_call_count += 1;

            let result_value: Value = serde_json::from_str(&response).unwrap_or(Value::Null);
            // Two failure shapes exist and BOTH must be honored: legacy
            // direct-method failures are top-level JSON-RPC errors, while
            // `tools/call` execution failures are in-band per MCP spec
            // (`result.isError: true`, built by handle_tools_call). Dropping
            // the isError branch would record exit 0 for every failing
            // tools/call from a spec-compliant client.
            let failed = result_value.get("error").is_some()
                || result_value
                    .pointer("/result/isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            let exit_code = if failed { 1 } else { 0 };
            let command = format!("mcp:{}", tool.strip_prefix("keel/").unwrap_or(&tool));
            session.record_tool_event(&command, call_duration, exit_code, &result_value);
        }

        writeln!(writer, "{}", response)?;
        writer.flush()?;
    }

    // Session ended (stdin EOF) — record summary
    session.record_session_end();

    Ok(())
}

#[cfg(test)]
#[path = "mcp_stdio_tests.rs"]
mod tests;
