//! E004 (function_removed) engine tests, plus a few misc single-shot checks
//! (clean compile, config-disabled placement/defaults) that didn't warrant
//! their own file.
use super::*;

#[test]
fn test_e004_function_removed() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Store a function that will be "removed"
    let node = make_node(
        1,
        "old11111111",
        "deprecated_fn",
        "fn deprecated_fn()",
        "src/lib.rs",
    );
    store.insert_node(&node).unwrap();

    // Store a caller
    let caller = make_node(2, "cal11111111", "consumer", "fn consumer()", "src/main.rs");
    store.insert_node(&caller).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(
            1,
            2,
            1,
            "src/main.rs",
        ))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile with an empty definitions list (function removed)
    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");
    let e004 = result.errors.iter().find(|v| v.code == "E004");
    assert!(e004.is_some(), "E004 function_removed should fire");
    let v = e004.unwrap();
    assert!(v.message.contains("deprecated_fn"));
    assert_eq!(v.affected.len(), 1);
    assert_eq!(v.affected[0].name, "consumer");
}

#[test]
fn test_e004_skipped_when_caller_in_batch() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Store a function that will be "removed"
    let node = make_node(
        1,
        "old11111111",
        "deprecated_fn",
        "fn deprecated_fn()",
        "src/lib.rs",
    );
    store.insert_node(&node).unwrap();

    // Store a caller in src/main.rs
    let caller = make_node(2, "cal11111111", "consumer", "fn consumer()", "src/main.rs");
    store.insert_node(&caller).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(
            1,
            2,
            1,
            "src/main.rs",
        ))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile BOTH files — caller is in the batch
    let file_lib = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    let file_main = FileIndex {
        file_path: "src/main.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "consumer",
            "fn consumer()",
            "{ /* no longer calls deprecated_fn */ }",
            "src/main.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file_lib, file_main]);
    let e004 = result.errors.iter().find(|v| v.code == "E004");
    assert!(
        e004.is_none(),
        "E004 should NOT fire when caller is in the same compile batch"
    );
}

#[test]
fn test_clean_compile_no_violations() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Compute hash matching the definition exactly
    let hash =
        keel_core::hash::compute_hash("fn clean(x: i32) -> bool", "{ x > 0 }", "Doc for clean");
    let mut node = make_node(1, &hash, "clean", "fn clean(x: i32) -> bool", "src/lib.rs");
    node.docstring = Some("Doc for clean".to_string());
    store.insert_node(&node).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));

    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "clean",
            "fn clean(x: i32) -> bool",
            "{ x > 0 }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "ok");
    assert!(result.errors.is_empty());
}

#[test]
fn test_config_disables_placement() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.placement = false;
    let mut engine = EnforcementEngine::with_config(Box::new(store), &config);

    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "db_connect",
            "fn db_connect()",
            "{}",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    let w001 = result.warnings.iter().find(|v| v.code == "W001");
    assert!(
        w001.is_none(),
        "W001 should NOT fire when placement config is false"
    );
}

#[test]
fn test_config_defaults_enable_all() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let config = keel_core::config::KeelConfig::default();
    let mut engine = EnforcementEngine::with_config(Box::new(store), &config);

    let mut def = make_definition("process", "def process(x)", "pass", "app.py");
    def.type_hints_present = false;

    let file = FileIndex {
        file_path: "app.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    let e002 = result.errors.iter().find(|v| v.code == "E002");
    assert!(
        e002.is_some(),
        "E002 should fire with default config (backward compat)"
    );
}

/// Two identical `fn default() -> Self { Self::new() }` in DIFFERENT files
/// normalize to the same content hash. `keel map` stores the second one under
/// its file-salted (disambiguated) identity; the engine's re-baseline pass must
/// do the same, or it hands both nodes one hash and `update_nodes` aborts the
/// entire persist with `HashCollision`.
///
/// Regression: `keel compile` on this repo printed
/// "failed to persist node updates: Hash collision detected for hash
/// G3B2feLlDMW between functions 'default' and 'default'".
#[test]
fn identical_defs_in_different_files_persist_with_distinct_hashes() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Both nodes start out stale (some earlier body), so BOTH re-baseline in
    // one compile pass — the exact situation that collided.
    store
        .insert_node(&make_node(
            1,
            "stale111111",
            "default",
            "fn default() -> Self",
            "src/a.rs",
        ))
        .unwrap();
    store
        .insert_node(&make_node(
            2,
            "stale222222",
            "default",
            "fn default() -> Self",
            "src/b.rs",
        ))
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));
    let body = "{ Self::new() }";
    let files: Vec<FileIndex> = ["src/a.rs", "src/b.rs"]
        .iter()
        .map(|path| FileIndex {
            file_path: path.to_string(),
            content_hash: 0,
            definitions: vec![make_definition(
                "default",
                "fn default() -> Self",
                body,
                path,
            )],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        })
        .collect();

    let result = engine.compile(&files);
    assert_eq!(
        result.info.hashes_changed.len(),
        2,
        "both nodes re-baseline"
    );

    // The persist must have succeeded: both nodes keep their identity and now
    // carry DISTINCT, non-stale hashes.
    let a = engine.store.get_node_by_id(1).expect("node a survives");
    let b = engine.store.get_node_by_id(2).expect("node b survives");
    assert_ne!(a.hash, "stale111111", "node a re-baselined");
    assert_ne!(b.hash, "stale222222", "node b re-baselined");
    assert_ne!(
        a.hash, b.hash,
        "identical defs in different files must not share a hash"
    );
    assert_eq!(result.errors.len(), 0);
}

/// Two same-named defs in ONE file (e.g. the two `fn is_available(&self)`
/// impls in `python/ty.rs`) must bind to two DISTINCT stored nodes. Matching
/// stored nodes by name alone gave both defs the same node, so one node took
/// two conflicting hash updates while its sibling was orphaned — and the
/// second update tried to claim the hash the sibling legitimately owned.
#[test]
fn same_named_defs_in_one_file_bind_to_distinct_nodes() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let sig = "fn is_available(&self) -> bool";
    for (id, hash, line) in [(1u64, "stale111111", 10u32), (2, "stale222222", 40)] {
        let mut node = make_node(id, hash, "is_available", sig, "src/ty.rs");
        node.line_start = line;
        store.insert_node(&node).unwrap();
    }

    let mut engine = EnforcementEngine::new(Box::new(store));
    let mut first = make_definition("is_available", sig, "{ self.a }", "src/ty.rs");
    first.line_start = 10;
    let mut second = make_definition("is_available", sig, "{ self.b }", "src/ty.rs");
    second.line_start = 40;

    let result = engine.compile(&[FileIndex {
        file_path: "src/ty.rs".to_string(),
        content_hash: 0,
        definitions: vec![first, second],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }]);

    let a = engine.store.get_node_by_id(1).expect("node 1 survives");
    let b = engine.store.get_node_by_id(2).expect("node 2 survives");
    assert_ne!(
        a.hash, "stale111111",
        "line-10 def re-baselined its own node"
    );
    assert_ne!(
        b.hash, "stale222222",
        "line-40 def re-baselined its own node"
    );
    assert_ne!(a.hash, b.hash, "distinct bodies keep distinct hashes");
    assert_eq!(result.errors.len(), 0);
}
