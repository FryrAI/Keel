//! W005/W006/W007 economy checks + progressive adoption, through the engine.
use super::*;
use crate::test_fixtures::{definition, file_index, node_for_definition, ECON_BODY};

#[test]
fn w005_fires_through_compile_and_config_gates_it() {
    // W005 needs a STORED node (graph evidence); seed one matching the def.
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("orphan", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[file_index("src/a.rs", vec![def.clone()])]);
    assert!(
        result.warnings.iter().any(|v| v.code == "W005"),
        "dead private function should warn: {:?}",
        result.warnings
    );

    // Gated off via config
    let store2 = SqliteGraphStore::in_memory().unwrap();
    store2.insert_node(&node_for_definition(1, &def)).unwrap();
    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.dead_code = false;
    let mut engine2 = EnforcementEngine::with_config(Box::new(store2), &config);
    let result2 = engine2.compile(&[file_index("src/a.rs", vec![def])]);
    assert!(!result2.warnings.iter().any(|v| v.code == "W005"));
}

#[test]
fn w006_fires_across_files_in_one_compile() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[
        file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]),
        file_index("src/b.rs", vec![definition("calc_b", "src/b.rs", true)]),
    ]);
    let w006: Vec<_> = result
        .warnings
        .iter()
        .filter(|v| v.code == "W006")
        .collect();
    assert_eq!(
        w006.len(),
        1,
        "second copy warns once: {:?}",
        result.warnings
    );
    assert!(w006[0].message.contains("calc_a"));
}

#[test]
fn w007_fires_through_compile() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    let mut def = definition("big", "src/huge.rs", true);
    def.line_end = 450;
    let result = engine.compile(&[file_index("src/huge.rs", vec![def])]);
    assert!(result.warnings.iter().any(|v| v.code == "W007"));
}

#[test]
fn economy_warnings_defer_in_batch_mode() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("orphan", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    engine.batch_start();

    let result = engine.compile(&[file_index("src/a.rs", vec![def])]);
    assert!(
        !result.warnings.iter().any(|v| v.code == "W005"),
        "W005 must defer during batch (scaffolding in progress)"
    );

    let flushed = engine.batch_end();
    assert!(
        flushed.warnings.iter().any(|v| v.code == "W005"),
        "deferred W005 fires at batch end: {:?}",
        flushed.warnings
    );
}

#[test]
fn progressive_downgrades_untouched_e003_through_compile() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Stored node identical to what the parser will report (untouched),
    // but PUBLIC and undocumented so E003 fires.
    let mut def = definition("legacy", "src/old.rs", true);
    def.body_text = "do_work()".to_string();
    def.docstring = None;
    store.insert_node(&node_for_definition(1, &def)).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[file_index("src/old.rs", vec![def])]);

    assert!(
        !result.errors.iter().any(|v| v.code == "E003"),
        "untouched E003 must not be an ERROR: {:?}",
        result.errors
    );
    let downgraded = result
        .warnings
        .iter()
        .find(|v| v.code == "E003")
        .expect("E003 present as WARNING");
    assert!(downgraded
        .fix_hint
        .as_deref()
        .unwrap()
        .contains("pre-existing"));
}

#[test]
fn progressive_downgrades_second_same_named_untouched_function() {
    // Two same-named functions in one file (impl-block style). BOTH are
    // untouched and undocumented — both E003s must downgrade; first-name-
    // wins matching would leave the second one as a spurious ERROR.
    let store = SqliteGraphStore::in_memory().unwrap();

    let mut first = definition("run", "src/old.rs", true);
    first.body_text = "reader_impl()".to_string();
    let mut second = definition("run", "src/old.rs", true);
    second.body_text = "writer_impl()".to_string();
    second.line_start = 30;
    second.line_end = 40;

    store.insert_node(&node_for_definition(1, &first)).unwrap();
    let mut n2 = node_for_definition(2, &second);
    n2.line_start = 30;
    n2.line_end = 40;
    store.insert_node(&n2).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[file_index("src/old.rs", vec![first, second])]);

    assert!(
        result.errors.is_empty(),
        "both untouched same-named functions must downgrade: {:?}",
        result.errors
    );
    assert_eq!(
        result.warnings.iter().filter(|v| v.code == "E003").count(),
        2
    );
}

// Keep the shared fixture honest: engine-level W006 depends on this body
// clearing the duplicate threshold.
#[test]
fn econ_body_clears_duplicate_threshold() {
    assert!(keel_core::hash::normalize_body(ECON_BODY).len() >= 60);
}

/// W005 must not report a trait method as dead: it is reached through
/// static/dynamic dispatch that the call graph resolves to the trait
/// declaration, never to the individual implementor.
#[test]
fn w005_skips_trait_context_definitions() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&make_node(
            1,
            "hash1111111",
            "is_available",
            "fn is_available(&self) -> bool",
            "src/p.rs",
        ))
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition(
        "is_available",
        "fn is_available(&self) -> bool",
        "{ true }",
        "src/p.rs",
    );
    def.is_public = false;
    def.in_trait_context = true;

    let result = engine.compile(&[FileIndex {
        file_path: "src/p.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }]);
    assert!(
        !result.warnings.iter().any(|v| v.code == "W005"),
        "trait method must not be reported dead: {:?}",
        result.warnings
    );
}

/// W002 must not ask you to rename a trait method — every implementor is
/// REQUIRED to use the same name, so a rename is not a legal fix.
#[test]
fn w002_skips_trait_context_definitions() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&make_node(
            1,
            "hash1111111",
            "resolve",
            "fn resolve(&self) -> bool",
            "src/other.rs",
        ))
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition(
        "resolve",
        "fn resolve(&self) -> bool",
        "{ true }",
        "src/p.rs",
    );
    def.in_trait_context = true;

    let result = engine.compile(&[FileIndex {
        file_path: "src/p.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }]);
    assert!(
        !result.warnings.iter().any(|v| v.code == "W002"),
        "trait method name is a contract, not a collision: {:?}",
        result.warnings
    );
}

/// W005 must not report a function that is only ever handed around as a VALUE
/// (`.map(render_file)`, `get(health)`, `#[serde(default = "default_true")]`).
/// Before value references existed these looked uncalled, and following the
/// fix_hint would have deleted live code.
#[test]
fn w005_counts_value_references_as_usage() {
    use keel_parsers::resolver::{Reference, ReferenceKind};

    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&make_node(
            1,
            "hash1111111",
            "render_file",
            "fn render_file(f: &F) -> String",
            "src/r.rs",
        ))
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut def = make_definition(
        "render_file",
        "fn render_file(f: &F) -> String",
        "{ String::new() }",
        "src/r.rs",
    );
    def.is_public = false;

    let result = engine.compile(&[FileIndex {
        file_path: "src/r.rs".to_string(),
        content_hash: 0,
        definitions: vec![def],
        // `entries.iter().map(render_file)` — a usage, but not a call.
        references: vec![Reference {
            name: "render_file".to_string(),
            file_path: "src/r.rs".to_string(),
            line: 21,
            kind: ReferenceKind::Value,
            resolved_to: None,
            call_arity: None,
        }],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }]);
    assert!(
        !result.warnings.iter().any(|v| v.code == "W005"),
        "a function passed as a value is not dead: {:?}",
        result.warnings
    );
}
