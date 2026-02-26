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

// --- truncate_to_hour tests ---

#[test]
fn truncate_to_hour_standard() {
    assert_eq!(
        truncate_to_hour("2026-02-23 14:35:22"),
        "2026-02-23 14:00:00"
    );
}

#[test]
fn truncate_to_hour_already_on_hour() {
    assert_eq!(
        truncate_to_hour("2026-02-23 14:00:00"),
        "2026-02-23 14:00:00"
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

    assert_eq!(payload.timestamp_hour, "2026-02-23 14:00:00");
    assert_eq!(payload.node_count_bucket, "101-500");
    assert_eq!(payload.edge_count_bucket, "1k-5k");
    assert_eq!(payload.command, "map");
    assert!(!payload.project_hash.is_empty());
    assert_eq!(payload.version, env!("CARGO_PKG_VERSION"));
}
