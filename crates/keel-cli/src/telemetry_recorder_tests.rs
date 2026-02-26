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
    let dir = tempfile::tempdir().unwrap();
    // Should return immediately without attempting network I/O
    try_send_remote(&config, dir.path(), &event);
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
    let result = detect_client_with(|k| {
        if k == "CLAUDECODE" {
            Some("1".into())
        } else {
            None
        }
    });
    assert_eq!(result, Some("claude-code".into()));
}

#[test]
fn detect_client_cursor() {
    let result = detect_client_with(|k| {
        if k == "CURSOR_CLI" {
            Some("1".into())
        } else {
            None
        }
    });
    assert_eq!(result, Some("cursor".into()));
}

#[test]
fn detect_client_none_when_clean() {
    let result = detect_client_with(|_| None);
    assert_eq!(result, None);
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

// --- to_iso8601_hour tests ---

#[test]
fn to_iso8601_hour_standard() {
    assert_eq!(
        to_iso8601_hour("2026-02-23 14:35:22"),
        "2026-02-23T14:00:00Z"
    );
}

#[test]
fn to_iso8601_hour_already_on_hour() {
    assert_eq!(
        to_iso8601_hour("2026-02-23 14:00:00"),
        "2026-02-23T14:00:00Z"
    );
}

#[test]
fn to_iso8601_hour_short_string() {
    // Strings shorter than 13 chars returned as-is
    assert_eq!(to_iso8601_hour("2026"), "2026");
}

// --- sanitize_for_remote tests ---

// --- record_event end-to-end tests ---

#[test]
fn record_event_writes_to_telemetry_db() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false, // no network
            ..Default::default()
        },
        ..Default::default()
    };

    let metrics = EventMetrics {
        error_count: 3,
        warning_count: 1,
        node_count: 50,
        edge_count: 120,
        client_name: Some("claude-code".into()),
        ..Default::default()
    };

    record_event(
        &keel_dir,
        &config,
        "compile",
        std::time::Duration::from_millis(142),
        0,
        metrics,
    );

    // Verify event landed in the database
    let db_path = keel_dir.join("telemetry.db");
    assert!(db_path.exists(), "telemetry.db should be created");

    let store = telemetry::TelemetryStore::open(&db_path).unwrap();
    let events = store.recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "compile");
    assert_eq!(events[0].duration_ms, 142);
    assert_eq!(events[0].exit_code, 0);
    assert_eq!(events[0].error_count, 3);
    assert_eq!(events[0].warning_count, 1);
    assert_eq!(events[0].node_count, 50);
    assert_eq!(events[0].edge_count, 120);
    assert_eq!(events[0].client_name, Some("claude-code".into()));
}

#[test]
fn record_event_skips_when_telemetry_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: false,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };

    record_event(
        &keel_dir,
        &config,
        "compile",
        std::time::Duration::from_millis(100),
        0,
        EventMetrics::default(),
    );

    // telemetry.db should not even be created
    let db_path = keel_dir.join("telemetry.db");
    assert!(!db_path.exists(), "telemetry.db should not exist when disabled");
}

#[test]
fn record_event_includes_error_codes_and_language_mix() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut error_codes = std::collections::HashMap::new();
    error_codes.insert("E001".to_string(), 2);
    error_codes.insert("E005".to_string(), 1);

    let mut language_mix = std::collections::HashMap::new();
    language_mix.insert("typescript".to_string(), 70);
    language_mix.insert("python".to_string(), 30);

    let metrics = EventMetrics {
        error_count: 3,
        warning_count: 0,
        error_codes,
        language_mix,
        ..Default::default()
    };

    record_event(
        &keel_dir,
        &config,
        "compile",
        std::time::Duration::from_millis(200),
        1,
        metrics,
    );

    let store = telemetry::TelemetryStore::open(&keel_dir.join("telemetry.db")).unwrap();
    let events = store.recent_events(10).unwrap();
    assert_eq!(events[0].exit_code, 1);
    assert_eq!(events[0].error_codes.get("E001"), Some(&2));
    assert_eq!(events[0].error_codes.get("E005"), Some(&1));
    assert_eq!(events[0].language_mix.get("typescript"), Some(&70));
    assert_eq!(events[0].language_mix.get("python"), Some(&30));
}

#[test]
fn record_event_prunes_old_events() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    // Pre-seed the database with a very old event
    let db_path = keel_dir.join("telemetry.db");
    let store = telemetry::TelemetryStore::open(&db_path).unwrap();
    let mut old_event = telemetry::new_event("map", 5000, 0);
    old_event.timestamp = "2020-01-01 00:00:00".into();
    store.record(&old_event).unwrap();
    assert_eq!(store.recent_events(10).unwrap().len(), 1);
    drop(store);

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };

    record_event(
        &keel_dir,
        &config,
        "compile",
        std::time::Duration::from_millis(50),
        0,
        EventMetrics::default(),
    );

    // Old event should have been pruned, only the new one remains
    let store = telemetry::TelemetryStore::open(&db_path).unwrap();
    let events = store.recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "compile");
}

#[test]
fn record_event_multiple_commands_aggregate() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };

    // Record several commands
    for (cmd, ms, errors) in [("compile", 100, 2), ("compile", 200, 0), ("map", 3000, 0)] {
        let metrics = EventMetrics {
            error_count: errors,
            ..Default::default()
        };
        record_event(
            &keel_dir,
            &config,
            cmd,
            std::time::Duration::from_millis(ms),
            if errors > 0 { 1 } else { 0 },
            metrics,
        );
    }

    let store = telemetry::TelemetryStore::open(&keel_dir.join("telemetry.db")).unwrap();
    let agg = store.aggregate(30).unwrap();
    assert_eq!(agg.total_invocations, 3);
    assert_eq!(agg.command_counts.get("compile"), Some(&2));
    assert_eq!(agg.command_counts.get("map"), Some(&1));
    assert_eq!(agg.total_errors, 2);
    assert!((agg.avg_compile_ms.unwrap() - 150.0).abs() < 1.0);
    assert!((agg.avg_map_ms.unwrap() - 3000.0).abs() < 1.0);
}

// --- adoption metrics tests ---

#[test]
fn record_event_adoption_metrics_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().to_path_buf();

    let config = KeelConfig {
        telemetry: TelemetryConfig {
            enabled: true,
            remote: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let metrics = EventMetrics {
        error_count: 2,
        warning_count: 1,
        violations_resolved: 3,
        violations_persisted: 1,
        violations_new: 2,
        client_name: Some("claude-code".into()),
        ..Default::default()
    };

    record_event(
        &keel_dir,
        &config,
        "compile",
        std::time::Duration::from_millis(80),
        1,
        metrics,
    );

    let store = telemetry::TelemetryStore::open(&keel_dir.join("telemetry.db")).unwrap();
    let events = store.recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].violations_resolved, 3);
    assert_eq!(events[0].violations_persisted, 1);
    assert_eq!(events[0].violations_new, 2);
}

// --- sanitize_for_remote tests ---

#[test]
fn sanitize_strips_id_and_truncates_timestamp() {
    let mut event = telemetry::new_event("map", 500, 0);
    event.id = Some(42);
    event.timestamp = "2026-02-23 14:35:22".into();
    event.node_count = 150;
    event.edge_count = 3000;

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());

    // id should not be present (RemotePayload has no id field)
    let json = serde_json::to_string(&payload).unwrap();
    assert!(!json.contains("\"id\""));

    assert_eq!(payload.timestamp, "2026-02-23T14:00:00Z");
    assert_eq!(payload.node_count, 150);
    assert_eq!(payload.edge_count, 3000);
    assert_eq!(payload.command, "map");
    assert!(!payload.project_hash.is_empty());
    assert_eq!(payload.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn sanitize_remote_payload_json_has_expected_field_names() {
    let mut event = telemetry::new_event("compile", 200, 0);
    event.timestamp = "2026-02-26 09:15:42".into();
    event.node_count = 500;
    event.edge_count = 1200;

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());
    let json = serde_json::to_string(&payload).unwrap();

    // API-required fields must be present with correct names
    assert!(json.contains("\"timestamp\":"), "must have 'timestamp' not 'timestamp_hour'");
    assert!(json.contains("\"node_count\":"), "must have 'node_count' not 'node_count_bucket'");
    assert!(json.contains("\"edge_count\":"), "must have 'edge_count' not 'edge_count_bucket'");

    // Old field names must NOT appear
    assert!(!json.contains("timestamp_hour"), "old field 'timestamp_hour' must not appear");
    assert!(!json.contains("node_count_bucket"), "old field 'node_count_bucket' must not appear");
    assert!(!json.contains("edge_count_bucket"), "old field 'edge_count_bucket' must not appear");
}

#[test]
fn sanitize_remote_payload_timestamp_is_iso8601() {
    let mut event = telemetry::new_event("compile", 100, 0);
    event.timestamp = "2026-12-31 23:59:59".into();

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());

    // Must be ISO 8601 with T separator and Z suffix, truncated to hour
    assert_eq!(payload.timestamp, "2026-12-31T23:00:00Z");
    assert!(payload.timestamp.contains('T'), "must use T separator");
    assert!(payload.timestamp.ends_with('Z'), "must end with Z");
}

#[test]
fn sanitize_remote_payload_counts_are_raw_integers() {
    let mut event = telemetry::new_event("map", 300, 0);
    event.node_count = 7777;
    event.edge_count = 0;

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());

    // Counts must be raw u32 values, not bucket strings
    assert_eq!(payload.node_count, 7777);
    assert_eq!(payload.edge_count, 0);

    // Verify they serialize as JSON numbers, not strings
    let json = serde_json::to_string(&payload).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["node_count"].is_number(), "node_count must serialize as number");
    assert!(v["edge_count"].is_number(), "edge_count must serialize as number");
    assert_eq!(v["node_count"].as_u64(), Some(7777));
    assert_eq!(v["edge_count"].as_u64(), Some(0));
}

#[test]
fn sanitize_remote_payload_preserves_all_fields() {
    let mut event = telemetry::new_event("compile", 250, 1);
    event.timestamp = "2026-02-26 10:30:00".into();
    event.error_count = 5;
    event.warning_count = 3;
    event.node_count = 100;
    event.edge_count = 200;
    event.circuit_breaker_events = 2;
    event.violations_resolved = 4;
    event.violations_persisted = 1;
    event.violations_new = 3;
    event.client_name = Some("cursor".into());
    event.language_mix.insert("python".into(), 80);
    event.resolution_tiers.insert("tier1".into(), 90);
    event.error_codes.insert("E001".into(), 2);

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());

    assert_eq!(payload.command, "compile");
    assert_eq!(payload.duration_ms, 250);
    assert_eq!(payload.exit_code, 1);
    assert_eq!(payload.error_count, 5);
    assert_eq!(payload.warning_count, 3);
    assert_eq!(payload.node_count, 100);
    assert_eq!(payload.edge_count, 200);
    assert_eq!(payload.circuit_breaker_events, 2);
    assert_eq!(payload.violations_resolved, 4);
    assert_eq!(payload.violations_persisted, 1);
    assert_eq!(payload.violations_new, 3);
    assert_eq!(payload.client_name, Some("cursor".into()));
    assert_eq!(payload.language_mix.get("python"), Some(&80));
    assert_eq!(payload.resolution_tiers.get("tier1"), Some(&90));
    assert_eq!(payload.error_codes.get("E001"), Some(&2));
}

#[test]
fn sanitize_remote_payload_json_snapshot() {
    let mut event = telemetry::new_event("compile", 142, 1);
    event.timestamp = "2026-02-26 10:22:33".into();
    event.node_count = 3296;
    event.edge_count = 5423;
    event.error_count = 2;
    event.warning_count = 15;
    event.client_name = Some("claude-code".into());
    event.language_mix.insert("rust".into(), 99);
    event.error_codes.insert("E001".into(), 2);

    let dir = tempfile::tempdir().unwrap();
    let payload = sanitize_for_remote(&event, dir.path());
    let json = serde_json::to_value(&payload).unwrap();

    // Verify the shape matches the API schema exactly
    assert!(json.is_object());
    let obj = json.as_object().unwrap();

    // Required fields exist
    assert!(obj.contains_key("timestamp"));
    assert!(obj.contains_key("node_count"));
    assert!(obj.contains_key("edge_count"));
    assert!(obj.contains_key("project_hash"));
    assert!(obj.contains_key("version"));
    assert!(obj.contains_key("command"));

    // Types are correct
    assert!(obj["timestamp"].is_string());
    assert!(obj["node_count"].is_number());
    assert!(obj["edge_count"].is_number());
    assert!(obj["project_hash"].is_string());
    assert!(obj["duration_ms"].is_number());
    assert!(obj["exit_code"].is_number());

    // Values are correct
    assert_eq!(obj["timestamp"].as_str().unwrap(), "2026-02-26T10:00:00Z");
    assert_eq!(obj["node_count"].as_u64().unwrap(), 3296);
    assert_eq!(obj["edge_count"].as_u64().unwrap(), 5423);
    assert_eq!(obj["command"].as_str().unwrap(), "compile");
    assert_eq!(obj["duration_ms"].as_u64().unwrap(), 142);
    assert_eq!(obj["exit_code"].as_i64().unwrap(), 1);
    assert_eq!(obj["error_count"].as_u64().unwrap(), 2);
    assert_eq!(obj["warning_count"].as_u64().unwrap(), 15);
    assert_eq!(obj["client_name"].as_str().unwrap(), "claude-code");

}

#[test]
fn to_iso8601_hour_midnight() {
    assert_eq!(
        to_iso8601_hour("2026-01-01 00:00:00"),
        "2026-01-01T00:00:00Z"
    );
}

#[test]
fn to_iso8601_hour_end_of_day() {
    assert_eq!(
        to_iso8601_hour("2026-12-31 23:59:59"),
        "2026-12-31T23:00:00Z"
    );
}
