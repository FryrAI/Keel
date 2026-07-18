//! Engine-level circuit breaker reset test: a violation that persists across
//! several compiles should escalate as before, but once actually resolved its
//! breaker state must clear rather than staying auto-downgraded forever.
use super::*;

#[test]
fn test_circuit_breaker_resets_on_resolved_violation() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Seed the store with a node matching the (still-failing) definition below —
    // mirrors `keel map` having already registered this function, so the
    // circuit breaker's hash-based identifier is in scope across compiles.
    let hash = keel_core::hash::compute_hash("def process(x)", "pass", "Doc for process");
    let node = make_node(1, &hash, "process", "def process(x)", "app.py");
    store.insert_node(&node).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut bad = make_definition("process", "def process(x)", "pass", "app.py");
    bad.type_hints_present = false;

    let make_file = |def: Definition| FileIndex {
        file_path: "app.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    // Compile the unresolved violation 3 times in a row — it escalates all
    // the way to auto-downgrade even though nobody has fixed anything yet.
    for i in 0..3 {
        let result = engine.compile(&[make_file(bad.clone())]);
        let fired = result
            .errors
            .iter()
            .chain(result.warnings.iter())
            .any(|v| v.code == "E002");
        assert!(fired, "E002 should fire on attempt {}", i + 1);
    }
    assert_eq!(engine.circuit_breaker_failures("E002", &hash, "app.py"), 3);
    // Third compile should have auto-downgraded it to WARNING.
    let pre_fix = engine.compile(&[make_file(bad.clone())]);
    assert!(
        pre_fix.warnings.iter().any(|v| v.code == "E002"),
        "E002 should be auto-downgraded to WARNING after 3 unresolved failures"
    );

    // Now actually fix it: add type hints (signature/body/docstring unchanged
    // so this test isolates the E002 reset from hash-change bookkeeping).
    let mut fixed = bad.clone();
    fixed.type_hints_present = true;
    let fixed_result = engine.compile(&[make_file(fixed)]);
    assert!(
        fixed_result
            .errors
            .iter()
            .chain(fixed_result.warnings.iter())
            .all(|v| v.code != "E002"),
        "E002 should not fire once type hints are added"
    );
    assert_eq!(
        engine.circuit_breaker_failures("E002", &hash, "app.py"),
        0,
        "circuit breaker should reset once the violation is resolved"
    );

    // Regression: reintroduce the exact same unresolved violation. If the
    // reset above hadn't happened, the breaker would still be in its
    // downgraded state and this would come back as WARNING, not ERROR.
    let regressed = engine.compile(&[make_file(bad)]);
    assert!(
        regressed.errors.iter().any(|v| v.code == "E002"),
        "E002 should re-fire as a fresh ERROR after reset, not stay downgraded"
    );
}
