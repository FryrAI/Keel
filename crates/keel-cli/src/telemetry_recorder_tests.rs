use super::*;
use keel_core::config::{KeelConfig, TelemetryConfig};
use keel_core::telemetry;

// --- Existing tests (moved from inline) ---

#[test]
fn try_send_remote_skips_when_disabled() {
    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let event = telemetry::new_event("compile", 100, 0);
    // Should return immediately without attempting network I/O
    try_send_remote(&config, &event);
}

#[test]
fn telemetry_event_serializes() {
    let mut event = telemetry::new_event("compile", 150, 0);
    event.error_count = 2;
    event.warning_count = 5;
    event.node_count = 100;
    event.edge_count = 200;
    event.language_mix.insert("typescript".to_string(), 60);

    let json = serde_json::to_string(&event).expect("TelemetryEvent should serialize");
    assert!(json.contains("\"command\":\"compile\""));
    assert!(json.contains("\"duration_ms\":150"));
    assert!(json.contains("\"error_count\":2"));
    assert!(json.contains("\"typescript\":60"));
}

// --- detect_client tests ---

#[test]
fn detect_client_claude_code() {
    std::env::set_var("CLAUDECODE", "1");
    assert_eq!(detect_client(), Some("claude-code".into()));
    std::env::remove_var("CLAUDECODE");
}

#[test]
fn detect_client_none_when_clean() {
    // Clear all known env vars to test fallback
    std::env::remove_var("CLAUDECODE");
    std::env::remove_var("CURSOR_CLI");
    // Note: we can't safely remove VSCODE_PID or CI in test since
    // they may be set by the real environment. Just test that the
    // function returns *something* without panicking.
    let _ = detect_client();
}

// --- command_name tests ---

#[test]
fn command_name_init() {
    let cmd = crate::cli_args::Commands::Init {
        merge: false,
        yes: false,
    };
    assert_eq!(command_name(&cmd), "init");
}

#[test]
fn command_name_compile() {
    let cmd = crate::cli_args::Commands::Compile {
        files: vec![],
        batch_start: false,
        batch_end: false,
        strict: false,
        tier3: false,
        suppress: None,
        depth: 1,
        changed: false,
        since: None,
        delta: false,
        timeout: None,
    };
    assert_eq!(command_name(&cmd), "compile");
}

#[test]
fn command_name_push() {
    let cmd = crate::cli_args::Commands::Push { yes: false };
    assert_eq!(command_name(&cmd), "push");
}

#[test]
fn command_name_login() {
    let cmd = crate::cli_args::Commands::Login;
    assert_eq!(command_name(&cmd), "login");
}

#[test]
fn command_name_logout() {
    let cmd = crate::cli_args::Commands::Logout;
    assert_eq!(command_name(&cmd), "logout");
}

// --- truncate_to_hour tests ---

#[test]
fn truncate_to_hour_standard() {
    assert_eq!(
        truncate_to_hour("2026-02-23T14:35:22Z"),
        "2026-02-23T14:00:00Z"
    );
}

#[test]
fn truncate_to_hour_already_on_hour() {
    assert_eq!(
        truncate_to_hour("2026-02-23T14:00:00Z"),
        "2026-02-23T14:00:00Z"
    );
}

#[test]
fn truncate_to_hour_short_string() {
    // Strings shorter than 13 chars returned as-is
    assert_eq!(truncate_to_hour("2026"), "2026");
}

// --- bucket_count tests ---

#[test]
fn bucket_count_zero() {
    assert_eq!(bucket_count(0), "0");
}

#[test]
fn bucket_count_small() {
    assert_eq!(bucket_count(1), "1-10");
    assert_eq!(bucket_count(10), "1-10");
}

#[test]
fn bucket_count_medium() {
    assert_eq!(bucket_count(11), "11-50");
    assert_eq!(bucket_count(50), "11-50");
    assert_eq!(bucket_count(51), "51-100");
    assert_eq!(bucket_count(100), "51-100");
}

#[test]
fn bucket_count_large() {
    assert_eq!(bucket_count(500), "101-500");
    assert_eq!(bucket_count(1000), "501-1k");
    assert_eq!(bucket_count(5000), "1k-5k");
    assert_eq!(bucket_count(10000), "5k-10k");
    assert_eq!(bucket_count(99999), "10k+");
}

// --- sanitize_for_remote tests ---

#[test]
fn sanitize_strips_id_and_truncates_timestamp() {
    let mut event = telemetry::new_event("map", 500, 0);
    event.id = Some(42);
    event.timestamp = "2026-02-23T14:35:22Z".into();
    event.node_count = 150;
    event.edge_count = 3000;

    let payload = sanitize_for_remote(&event);

    // id should not be present (RemotePayload has no id field)
    let json = serde_json::to_string(&payload).unwrap();
    assert!(!json.contains("\"id\""));

    assert_eq!(payload.timestamp_hour, "2026-02-23T14:00:00Z");
    assert_eq!(payload.node_count_bucket, "101-500");
    assert_eq!(payload.edge_count_bucket, "1k-5k");
    assert_eq!(payload.command, "map");
}
