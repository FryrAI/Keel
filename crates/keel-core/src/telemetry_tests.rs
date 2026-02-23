use super::*;

fn make_event(command: &str, duration_ms: u64, exit_code: i32) -> TelemetryEvent {
    let mut event = new_event(command, duration_ms, exit_code);
    event.error_count = 2;
    event.warning_count = 5;
    event.node_count = 100;
    event.edge_count = 200;
    event.language_mix.insert("typescript".to_string(), 60);
    event.language_mix.insert("python".to_string(), 40);
    event.error_codes.insert("E001".to_string(), 1);
    event.error_codes.insert("W001".to_string(), 1);
    event
}

#[test]
fn test_open_in_memory() {
    let store = TelemetryStore::in_memory().unwrap();
    let events = store.recent_events(10).unwrap();
    assert!(events.is_empty());
}

#[test]
fn test_record_and_retrieve() {
    let store = TelemetryStore::in_memory().unwrap();
    let event = make_event("compile", 150, 0);
    store.record(&event).unwrap();

    let events = store.recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command, "compile");
    assert_eq!(events[0].duration_ms, 150);
    assert_eq!(events[0].exit_code, 0);
    assert_eq!(events[0].error_count, 2);
    assert_eq!(events[0].warning_count, 5);
    assert_eq!(events[0].node_count, 100);
    assert_eq!(events[0].edge_count, 200);
    assert_eq!(events[0].language_mix.get("typescript"), Some(&60));
    assert_eq!(events[0].error_codes.get("E001"), Some(&1));
    assert_eq!(events[0].error_codes.get("W001"), Some(&1));
    assert!(events[0].client_name.is_none());
}

#[test]
fn test_aggregate_empty() {
    let store = TelemetryStore::in_memory().unwrap();
    let agg = store.aggregate(30).unwrap();
    assert_eq!(agg.total_invocations, 0);
    assert!(agg.avg_compile_ms.is_none());
    assert!(agg.avg_map_ms.is_none());
    assert_eq!(agg.total_errors, 0);
    assert_eq!(agg.total_warnings, 0);
    assert!(agg.command_counts.is_empty());
}

#[test]
fn test_aggregate_with_data() {
    let store = TelemetryStore::in_memory().unwrap();
    store.record(&make_event("compile", 100, 0)).unwrap();
    store.record(&make_event("compile", 200, 0)).unwrap();
    store.record(&make_event("map", 3000, 0)).unwrap();

    let agg = store.aggregate(30).unwrap();
    assert_eq!(agg.total_invocations, 3);
    assert!((agg.avg_compile_ms.unwrap() - 150.0).abs() < 1.0);
    assert!((agg.avg_map_ms.unwrap() - 3000.0).abs() < 1.0);
    assert_eq!(agg.total_errors, 6); // 2 per event * 3
    assert_eq!(agg.total_warnings, 15); // 5 per event * 3
    assert_eq!(agg.command_counts.get("compile"), Some(&2));
    assert_eq!(agg.command_counts.get("map"), Some(&1));
    // error_codes: E001=1 per event * 3, W001=1 per event * 3
    assert_eq!(agg.top_error_codes.get("E001"), Some(&3));
    assert_eq!(agg.top_error_codes.get("W001"), Some(&3));
}

#[test]
fn test_client_name_roundtrip() {
    let store = TelemetryStore::in_memory().unwrap();
    let mut event = make_event("compile", 100, 0);
    event.client_name = Some("claude-code".to_string());
    store.record(&event).unwrap();

    let events = store.recent_events(10).unwrap();
    assert_eq!(events[0].client_name, Some("claude-code".to_string()));
}

#[test]
fn test_agent_stats_aggregation() {
    let store = TelemetryStore::in_memory().unwrap();

    // Simulate MCP tool call events
    let mut e1 = new_event("mcp:compile", 50, 0);
    e1.client_name = Some("claude-code".to_string());
    store.record(&e1).unwrap();

    let mut e2 = new_event("mcp:discover", 30, 0);
    e2.client_name = Some("claude-code".to_string());
    store.record(&e2).unwrap();

    // Session summary event: node_count = tool_call_count
    let mut session = new_event("mcp:session", 60000, 0);
    session.client_name = Some("claude-code".to_string());
    session.node_count = 2;
    store.record(&session).unwrap();

    let agg = store.aggregate(30).unwrap();
    let stats = agg.agent_stats.get("claude-code").unwrap();
    assert_eq!(stats.sessions, 1);
    assert_eq!(stats.total_tool_calls, 2);
    assert!((stats.avg_tool_calls_per_session - 2.0).abs() < 0.01);
    assert_eq!(stats.tool_usage.get("mcp:compile"), Some(&1));
    assert_eq!(stats.tool_usage.get("mcp:discover"), Some(&1));
}

#[test]
fn test_prune_old_events() {
    let store = TelemetryStore::in_memory().unwrap();
    // Insert event with old timestamp
    store
        .conn
        .execute(
            "INSERT INTO events (timestamp, command, duration_ms, exit_code)
         VALUES ('2020-01-01 00:00:00', 'compile', 100, 0)",
            [],
        )
        .unwrap();
    // Insert recent event
    store.record(&make_event("compile", 100, 0)).unwrap();

    assert_eq!(store.recent_events(10).unwrap().len(), 2);
    let pruned = store.prune(90).unwrap();
    assert_eq!(pruned, 1);
    assert_eq!(store.recent_events(10).unwrap().len(), 1);
}

#[test]
fn test_chrono_utc_now_format() {
    let ts = chrono_utc_now();
    // Should match SQLite native format: YYYY-MM-DD HH:MM:SS
    assert_eq!(ts.len(), 19);
    assert_eq!(&ts[4..5], "-");
    assert_eq!(&ts[7..8], "-");
    assert_eq!(&ts[10..11], " ");
}

#[test]
fn test_file_based_store() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("telemetry.db");

    // Write
    {
        let store = TelemetryStore::open(&db_path).unwrap();
        store.record(&make_event("map", 2000, 0)).unwrap();
    }

    // Re-open and read
    {
        let store = TelemetryStore::open(&db_path).unwrap();
        let events = store.recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].command, "map");
    }
}
