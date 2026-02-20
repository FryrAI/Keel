use super::*;

fn make_event(command: &str, duration_ms: u64, exit_code: i32) -> TelemetryEvent {
    let mut event = new_event(command, duration_ms, exit_code);
    event.error_count = 2;
    event.warning_count = 5;
    event.node_count = 100;
    event.edge_count = 200;
    event.language_mix.insert("typescript".to_string(), 60);
    event.language_mix.insert("python".to_string(), 40);
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
}

#[test]
fn test_prune_old_events() {
    let store = TelemetryStore::in_memory().unwrap();
    // Insert event with old timestamp
    store
        .conn
        .execute(
            "INSERT INTO events (timestamp, command, duration_ms, exit_code)
         VALUES ('2020-01-01T00:00:00Z', 'compile', 100, 0)",
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
    // Should match ISO 8601: YYYY-MM-DDTHH:MM:SSZ
    assert!(ts.ends_with('Z'));
    assert_eq!(ts.len(), 20);
    assert_eq!(&ts[4..5], "-");
    assert_eq!(&ts[7..8], "-");
    assert_eq!(&ts[10..11], "T");
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
