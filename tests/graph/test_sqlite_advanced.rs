// Tests for SqliteGraphStore advanced features (Spec 000 - Graph Schema)
//
// Module profiles, resolution cache, circuit breaker, bulk atomicity,
// concurrent reads, and auto-create schema.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphError, GraphNode, NodeChange, NodeKind};

fn make_node(id: u64, hash: &str, name: &str, kind: NodeKind) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind,
        name: name.into(),
        signature: format!("{name}()"),
        file_path: "test.rs".into(),
        line_start: 1,
        line_end: 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

#[test]
/// Storing and retrieving a ModuleProfile via raw SQL insertion +
/// public get_module_profile API should preserve all profile data.
fn test_sqlite_module_profile_storage() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("module_profile.db");
    let db_str = db_path.to_str().unwrap();

    // Create store and insert a module node
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let module_node = GraphNode {
        id: 42,
        hash: "mod_hash_42".into(),
        kind: NodeKind::Module,
        name: "utils".into(),
        signature: "utils".into(),
        file_path: "src/utils.ts".into(),
        line_start: 1,
        line_end: 100,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    };
    store
        .update_nodes(vec![NodeChange::Add(module_node)])
        .unwrap();
    drop(store);

    // Insert module profile via raw SQL (no public insert API)
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute(
            "INSERT INTO module_profiles (module_id, path, function_count, class_count, line_count, function_name_prefixes, primary_types, import_sources, export_targets, external_endpoint_count, responsibility_keywords) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                42i64,
                "src/utils.ts",
                3i32,
                2i32,
                100i32,
                r#"["parse","format"]"#,
                r#"["Parser"]"#,
                r#"["fs"]"#,
                r#"[]"#,
                0i32,
                r#"["parsing"]"#,
            ],
        ).unwrap();
    }

    // Re-open store and verify via public get_module_profile
    let store = SqliteGraphStore::open(db_str).unwrap();
    let profile = store.get_module_profile(42);
    assert!(profile.is_some(), "should read module profile for id 42");
    let profile = profile.unwrap();
    assert_eq!(profile.module_id, 42);
    assert_eq!(profile.path, "src/utils.ts");
    assert_eq!(profile.function_count, 3);
    assert_eq!(profile.class_count, 2);
    assert_eq!(profile.line_count, 100);
    assert!(profile.function_name_prefixes.contains(&"parse".into()));
    assert!(profile.function_name_prefixes.contains(&"format".into()));
    assert!(profile.primary_types.contains(&"Parser".into()));
    assert!(profile.import_sources.contains(&"fs".into()));
    assert_eq!(profile.external_endpoint_count, 0);
    assert!(profile.responsibility_keywords.contains(&"parsing".into()));
}

#[test]
/// The resolution cache table should store and retrieve cached resolution results.
/// Verified via raw SQL insertion + read (no public API exposed).
/// Also verifies clear_all removes cached entries.
fn test_sqlite_resolution_cache() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("res_cache.db");
    let db_str = db_path.to_str().unwrap();

    // Create store and insert a node (needed for FK reference)
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let node = make_node(1, "hash_target", "target_fn", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(node)]).unwrap();
    drop(store);

    // Insert resolution_cache entries via raw SQL
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        conn.execute(
            "INSERT INTO resolution_cache (call_site_hash, resolved_node_id, confidence, resolution_tier) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["call_hash_001", 1i64, 0.95f64, "tier1"],
        ).unwrap();
        conn.execute(
            "INSERT INTO resolution_cache (call_site_hash, resolved_node_id, confidence, resolution_tier) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["call_hash_002", 1i64, 0.70f64, "tier2"],
        ).unwrap();
    }

    // Read back via raw SQL and verify
    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        let mut stmt = conn
            .prepare("SELECT call_site_hash, resolved_node_id, confidence, resolution_tier FROM resolution_cache ORDER BY call_site_hash")
            .unwrap();
        let rows: Vec<(String, i64, f64, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(rows.len(), 2, "should have 2 cached entries");
        assert_eq!(rows[0].0, "call_hash_001");
        assert_eq!(rows[0].1, 1);
        assert!((rows[0].2 - 0.95).abs() < 0.001);
        assert_eq!(rows[0].3, "tier1");
        assert_eq!(rows[1].0, "call_hash_002");
        assert!((rows[1].2 - 0.70).abs() < 0.001);
        assert_eq!(rows[1].3, "tier2");
    }

    // Verify clear_all removes resolution_cache entries
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    store.clear_all().unwrap();

    {
        let conn = rusqlite::Connection::open(db_str).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM resolution_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "clear_all should remove all resolution_cache entries");
    }
}

#[test]
/// The circuit breaker state should be stored and retrieved.
fn test_sqlite_circuit_breaker_state() {
    let store = SqliteGraphStore::in_memory().unwrap();

    let state = vec![
        ("E001".to_string(), "hash_abc".to_string(), 2u32, false),
        ("E002".to_string(), "hash_def".to_string(), 3u32, true),
    ];
    store.save_circuit_breaker(&state).unwrap();

    let loaded = store.load_circuit_breaker().unwrap();
    assert_eq!(loaded.len(), 2);

    let first = loaded.iter().find(|r| r.0 == "E001").unwrap();
    assert_eq!(first.1, "hash_abc");
    assert_eq!(first.2, 2);
    assert!(!first.3, "should not be downgraded");

    let second = loaded.iter().find(|r| r.0 == "E002").unwrap();
    assert_eq!(second.1, "hash_def");
    assert_eq!(second.2, 3);
    assert!(second.3, "should be downgraded");
}

#[test]
/// Bulk insertion with a duplicate hash (different name) should fail atomically.
fn test_sqlite_bulk_insert_atomicity() {
    let mut store = SqliteGraphStore::in_memory().unwrap();

    // Seed a node so the collision target exists
    let existing = make_node(1, "dup_hash", "original_fn", NodeKind::Function);
    store.update_nodes(vec![NodeChange::Add(existing)]).unwrap();

    // Batch with a hash collision: same hash "dup_hash", different name
    let batch = vec![
        NodeChange::Add(make_node(2, "unique_a", "fn_a", NodeKind::Function)),
        NodeChange::Add(make_node(3, "dup_hash", "collider_fn", NodeKind::Function)),
        NodeChange::Add(make_node(4, "unique_b", "fn_b", NodeKind::Function)),
    ];

    let result = store.update_nodes(batch);
    assert!(
        result.is_err(),
        "batch with hash collision should fail: {:?}",
        result
    );
    if let Err(GraphError::HashCollision { hash, .. }) = &result {
        assert_eq!(hash, "dup_hash");
    } else {
        panic!("expected HashCollision error, got: {:?}", result);
    }

    // None of the batch nodes should have been persisted (transaction rolled back)
    assert!(
        store.get_node("unique_a").is_none(),
        "transaction should have rolled back node unique_a"
    );
    assert!(
        store.get_node("unique_b").is_none(),
        "transaction should have rolled back node unique_b"
    );
    // Original node should still be intact
    assert!(store.get_node("dup_hash").is_some());
}

#[test]
/// SQLite store should handle concurrent reads without corruption.
fn test_sqlite_concurrent_reads() {
    // SQLite in-memory databases are per-connection, so we use a temp file
    // that multiple connections can open for concurrent reads.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("concurrent.db");
    let db_str = db_path.to_str().unwrap();

    // Populate the store
    let mut store = SqliteGraphStore::open(db_str).unwrap();
    let mut nodes = Vec::new();
    for i in 1..=10u64 {
        nodes.push(NodeChange::Add(make_node(
            i,
            &format!("conc_hash_{i}"),
            &format!("fn_{i}"),
            NodeKind::Function,
        )));
    }
    store.update_nodes(nodes).unwrap();
    drop(store);

    // Spawn readers each with their own connection
    let path_owned = db_str.to_string();
    let handles: Vec<_> = (1..=4)
        .map(|thread_id| {
            let p = path_owned.clone();
            std::thread::spawn(move || {
                let reader = SqliteGraphStore::open(&p).unwrap();
                for i in 1..=10u64 {
                    let node = reader.get_node(&format!("conc_hash_{i}"));
                    assert!(
                        node.is_some(),
                        "thread {thread_id} failed to read node {i}"
                    );
                    assert_eq!(node.unwrap().name, format!("fn_{i}"));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("reader thread panicked");
    }
}

#[test]
/// Opening a SQLite store on a new database should auto-create the schema.
fn test_sqlite_auto_create_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("fresh.db");
    let db_str = db_path.to_str().unwrap();

    assert!(!db_path.exists(), "db should not exist yet");

    let store = SqliteGraphStore::open(db_str).unwrap();
    assert!(db_path.exists(), "db file should have been created");

    let version = store.schema_version().unwrap();
    assert_eq!(version, 4, "schema version should be 4");

    // Verify we can perform basic operations on the fresh schema
    let modules = store.get_all_modules();
    assert!(modules.is_empty(), "fresh db should have no modules");
}
