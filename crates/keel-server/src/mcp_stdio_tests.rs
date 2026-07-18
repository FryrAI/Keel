//! Telemetry instrumentation tests for the MCP stdio loop.
//!
//! Regression coverage for the `tools/call` blind spot: telemetry used to key
//! off the JSON-RPC method, so every tool call from a spec-compliant client
//! (which routes through `tools/call`) went unrecorded.

use super::*;

use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_core::telemetry::TelemetryEvent;
use tempfile::TempDir;

use crate::mcp_tools::tool_payload;

/// A `keel_dir` with telemetry enabled by default.
fn test_dir() -> TempDir {
    tempfile::tempdir().expect("temp dir")
}

fn test_store() -> SharedStore {
    Arc::new(Mutex::new(
        SqliteGraphStore::in_memory().expect("in-memory store"),
    ))
}

/// Feed `lines` through the loop against a real telemetry db, returning every
/// event that was recorded.
fn run_and_collect_events(keel_dir: &Path, lines: &[&str]) -> Vec<TelemetryEvent> {
    let store = test_store();
    let engine = mcp::create_shared_engine(None);
    let session = McpSession::new(Some(keel_dir), false);

    let input = lines.join("\n");
    let mut output: Vec<u8> = Vec::new();
    run_loop(
        &store,
        &engine,
        session,
        io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .expect("loop should not fail");

    let store = TelemetryStore::open(&keel_dir.join("telemetry.db")).expect("telemetry db");
    store.recent_events(100).expect("recent events")
}

fn tool_events(events: &[TelemetryEvent]) -> Vec<&TelemetryEvent> {
    events
        .iter()
        .filter(|e| e.command != "mcp:session")
        .collect()
}

// --- effective_tool_name ---

#[test]
fn test_effective_tool_name_from_tools_call() {
    let params = Some(serde_json::json!({"name": "keel/compile", "arguments": {}}));
    assert_eq!(
        effective_tool_name("tools/call", &params).as_deref(),
        Some("keel/compile")
    );
}

#[test]
fn test_effective_tool_name_from_legacy_direct_method() {
    assert_eq!(
        effective_tool_name("keel/discover", &None).as_deref(),
        Some("keel/discover")
    );
}

#[test]
fn test_effective_tool_name_ignores_non_tool_methods() {
    assert_eq!(effective_tool_name("initialize", &None), None);
    assert_eq!(effective_tool_name("tools/list", &None), None);
    assert_eq!(
        effective_tool_name("notifications/initialized", &None),
        None
    );
}

#[test]
fn test_effective_tool_name_tools_call_without_name() {
    let params = Some(serde_json::json!({"arguments": {}}));
    assert_eq!(effective_tool_name("tools/call", &params), None);
    assert_eq!(effective_tool_name("tools/call", &None), None);
}

// --- tool_payload ---
// (The wrap→unwrap roundtrip against the real wrapper lives next to the pair
// in mcp_tools.rs; these cover the legacy and malformed inputs.)

#[test]
fn test_tool_payload_passes_through_legacy_result() {
    let direct = serde_json::json!({"status": "ok", "errors": []});
    assert_eq!(tool_payload(&direct), direct);
}

#[test]
fn test_tool_payload_falls_back_on_non_json_text() {
    let wrapped = serde_json::json!({
        "content": [{ "type": "text", "text": "not json" }],
    });
    // Unparseable text must not panic — fall back to the raw result.
    assert_eq!(tool_payload(&wrapped), wrapped);
}

// --- end-to-end telemetry recording ---

/// The regression: a tool invoked via `tools/call` must be recorded, under the
/// tool's own name rather than the transport method.
#[test]
fn test_tools_call_is_recorded() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
        ],
    );

    let tools = tool_events(&events);
    assert_eq!(tools.len(), 1, "tools/call must produce one event");
    assert_eq!(tools[0].command, "mcp:compile");
    assert_eq!(tools[0].exit_code, 0);
}

#[test]
fn test_legacy_direct_method_still_recorded() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[r#"{"jsonrpc":"2.0","id":1,"method":"keel/compile","params":{"files":[]}}"#],
    );

    let tools = tool_events(&events);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].command, "mcp:compile");
}

/// Both calling conventions must record under the same command name, so
/// telemetry stays comparable across client versions.
#[test]
fn test_both_conventions_record_same_command() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"keel/compile","params":{"files":[]}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
        ],
    );

    let tools = tool_events(&events);
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().all(|e| e.command == "mcp:compile"));
}

/// Non-tool traffic must not be counted as tool calls.
#[test]
fn test_handshake_traffic_is_not_recorded_as_tool_call() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"t","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        ],
    );

    assert!(
        tool_events(&events).is_empty(),
        "handshake must not record tool events"
    );
}

/// A failing tool call is recorded with a non-zero exit code.
#[test]
fn test_failed_tools_call_records_exit_code_one() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"keel/discover","arguments":{"hash":"nope"}}}"#,
        ],
    );

    let tools = tool_events(&events);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].command, "mcp:discover");
    assert_eq!(tools[0].exit_code, 1);
}

/// The client name from `initialize` must still be attached to events that
/// arrive through `tools/call`.
#[test]
fn test_client_name_attached_to_tools_call_events() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"claude-code","version":"2.1"}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
        ],
    );

    let tools = tool_events(&events);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].client_name.as_deref(), Some("claude-code"));
}

/// Session summary counts tool calls that arrived via `tools/call`.
#[test]
fn test_session_summary_counts_tools_call() {
    let dir = test_dir();
    let events = run_and_collect_events(
        dir.path(),
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#,
        ],
    );

    let session = events
        .iter()
        .find(|e| e.command == "mcp:session")
        .expect("session summary event");
    // Convention: node_count carries tool_call_count for session events.
    assert_eq!(session.node_count, 2);
}

/// `--no-telemetry` must suppress recording on the `tools/call` path too.
#[test]
fn test_no_telemetry_flag_suppresses_tools_call_events() {
    let dir = test_dir();
    let store = test_store();
    let engine = mcp::create_shared_engine(None);
    let session = McpSession::new(Some(dir.path()), true);

    let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"keel/compile","arguments":{"files":[]}}}"#;
    let mut output: Vec<u8> = Vec::new();
    run_loop(
        &store,
        &engine,
        session,
        io::Cursor::new(input.as_bytes().to_vec()),
        &mut output,
    )
    .unwrap();

    assert!(
        !dir.path().join("telemetry.db").exists(),
        "no telemetry db should be created when disabled"
    );
}

/// The loop must still write exactly one response per request and stay silent
/// for notifications.
#[test]
fn test_loop_writes_one_response_per_request() {
    let dir = test_dir();
    let store = test_store();
    let engine = mcp::create_shared_engine(None);
    let session = McpSession::new(Some(dir.path()), true);

    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ]
    .join("\n");

    let mut output: Vec<u8> = Vec::new();
    run_loop(
        &store,
        &engine,
        session,
        io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .unwrap();

    let text = String::from_utf8(output).unwrap();
    let responses: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(responses.len(), 2, "notification must produce no output");
}
