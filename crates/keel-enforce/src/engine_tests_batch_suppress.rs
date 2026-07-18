//! Core engine plumbing: construction, empty compiles, batch mode, and
//! suppression (including its interaction with circuit-breaker escalation).
use super::*;

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
