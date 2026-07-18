//! E002 (missing_type_hints) / E003 (missing_docstring) engine tests.
use super::*;

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
