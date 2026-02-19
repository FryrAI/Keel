use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};
use keel_parsers::resolver::Definition;

fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.to_string(),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn make_call_edge(id: u64, src: u64, tgt: u64, file: &str) -> GraphEdge {
    GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line: 15,
        confidence: 1.0,
    }
}

fn make_definition(name: &str, sig: &str, body: &str, file: &str) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: sig.to_string(),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: Some(format!("Doc for {}", name)),
        is_public: true,
        type_hints_present: true,
        body_text: body.to_string(),
    }
}

#[test]
fn test_engine_new() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let _engine = EnforcementEngine::new(Box::new(store));
}

#[test]
fn test_compile_empty() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[]);
    assert_eq!(result.status, "ok");
    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn test_batch_mode() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    engine.batch_start();
    let result = engine.compile(&[]);
    assert_eq!(result.status, "ok");
    let batch_result = engine.batch_end();
    assert_eq!(batch_result.status, "ok");
}

#[test]
fn test_e001_broken_caller_fires() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Store a function with old hash
    let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    // Store a caller
    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller).unwrap();

    // Edge: caller -> foo
    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile with a changed signature for foo
    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32, y: i32)",
            "{ x + y }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");
    assert!(!result.errors.is_empty());
    let e001 = result.errors.iter().find(|v| v.code == "E001");
    assert!(e001.is_some(), "E001 broken_caller should fire");
    let v = e001.unwrap();
    assert_eq!(v.category, "broken_caller");
    assert_eq!(v.affected.len(), 1);
    assert_eq!(v.affected[0].name, "bar");
}

#[test]
fn test_e002_missing_type_hints() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

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
    assert_eq!(result.status, "error");
    let e002 = result.errors.iter().find(|v| v.code == "E002");
    assert!(e002.is_some(), "E002 missing_type_hints should fire");
    assert!(e002.unwrap().message.contains("process"));
}

#[test]
fn test_e003_missing_docstring() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition("handle", "fn handle()", "{}", "src/h.rs");
    def.docstring = None;

    let file = FileIndex {
        file_path: "src/h.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");
    let e003 = result.errors.iter().find(|v| v.code == "E003");
    assert!(e003.is_some(), "E003 missing_docstring should fire");
    assert!(e003.unwrap().message.contains("handle"));
}

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
fn test_batch_defers_e002_e003() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    engine.batch_start();

    let mut def = make_definition("process", "def process(x)", "pass", "app.py");
    def.type_hints_present = false;
    def.docstring = None;

    let file = FileIndex {
        file_path: "app.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    // During batch mode, E002/E003 should be deferred
    let result = engine.compile(&[file]);
    assert_eq!(
        result.status, "ok",
        "Deferred violations should not appear yet"
    );
    assert!(result.errors.is_empty());

    // batch_end should fire the deferred violations
    let batch_result = engine.batch_end();
    assert!(
        !batch_result.errors.is_empty(),
        "Deferred violations should fire on batch_end"
    );
    let codes: Vec<&str> = batch_result
        .errors
        .iter()
        .map(|v| v.code.as_str())
        .collect();
    assert!(codes.contains(&"E002") || codes.contains(&"E003"));
}

#[test]
fn test_suppression() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    engine.suppress("E002");

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
    // E002 should be suppressed to S001/INFO which goes to warnings, not errors
    let e002_errors = result.errors.iter().filter(|v| v.code == "E002").count();
    assert_eq!(e002_errors, 0, "E002 should be suppressed");

    // Should appear as S001 in warnings
    let s001 = result.warnings.iter().find(|v| v.code == "S001");
    assert!(s001.is_some(), "Suppressed E002 should become S001");
    assert!(s001.unwrap().suppressed);
}

#[test]
fn test_suppression_prevents_circuit_breaker_escalation() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    // Suppress E002 before compiling
    engine.suppress("E002");

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

    // Compile 3 times -- suppressed violations should become S001/INFO
    for _ in 0..3 {
        let result = engine.compile(std::slice::from_ref(&file));
        let e002_errors = result.errors.iter().filter(|v| v.code == "E002").count();
        assert_eq!(
            e002_errors, 0,
            "E002 should be suppressed in every iteration"
        );

        let s001 = result.warnings.iter().filter(|v| v.code == "S001").count();
        assert!(s001 > 0, "Suppressed E002 should appear as S001");
    }
}

#[test]
fn test_batch_expired_flushes_deferred() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    // Set batch state to already expired
    engine.batch_state = Some(crate::batch::BatchState::new_expired());

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

    // Compile with expired batch -- should flush and include E002 immediately
    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");
    let e002 = result.errors.iter().filter(|v| v.code == "E002").count();
    assert!(
        e002 > 0,
        "E002 should fire immediately when batch is expired"
    );
    // Batch state should be consumed
    assert!(
        engine.batch_state.is_none(),
        "Expired batch should be consumed"
    );
}

#[test]
fn test_e003_and_e002_both_fire_for_same_function() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition("handler", "def handler(x)", "pass", "app.py");
    def.type_hints_present = false;
    def.docstring = None;

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
    assert_eq!(result.status, "error");
    let codes: Vec<&str> = result.errors.iter().map(|v| v.code.as_str()).collect();
    assert!(
        codes.contains(&"E002"),
        "E002 should fire for missing type hints"
    );
    assert!(
        codes.contains(&"E003"),
        "E003 should fire for missing docstring"
    );
}

#[test]
fn test_config_disables_type_hints() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.type_hints = false;
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
        e002.is_none(),
        "E002 should NOT fire when type_hints config is false"
    );
}

#[test]
fn test_config_disables_docstrings() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.docstrings = false;
    let mut engine = EnforcementEngine::with_config(Box::new(store), &config);

    let mut def = make_definition("handle", "fn handle()", "{}", "src/h.rs");
    def.docstring = None;

    let file = FileIndex {
        file_path: "src/h.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    let e003 = result.errors.iter().find(|v| v.code == "E003");
    assert!(
        e003.is_none(),
        "E003 should NOT fire when docstrings config is false"
    );
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
