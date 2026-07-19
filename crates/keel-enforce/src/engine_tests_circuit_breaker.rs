//! Engine-level circuit breaker reset test: a violation on a session-modified
//! function persists across compiles and escalates as before, but once
//! actually resolved its breaker state must clear rather than staying
//! auto-downgraded forever.
//!
//! The function is MODIFIED (stored hash differs from the definition) so that
//! progressive adoption keeps E002 at ERROR — pre-existing, untouched debt is
//! progressive adoption's territory (see engine_tests_economy.rs), while the
//! breaker governs what the agent is actively working on.
use super::*;

#[test]
fn test_circuit_breaker_resets_on_resolved_violation() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Seed the store with a STALE hash — the agent has modified this function
    // this session, so its E002 stays ERROR under progressive adoption.
    let node = make_node(1, "staleHash000", "process", "def process(x)", "app.py");
    store.insert_node(&node).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut bad = make_definition("process", "def process(x)", "pass", "app.py");
    bad.type_hints_present = false;
    // The hash the store will hold after the first compile syncs it.
    let synced_hash = keel_core::hash::compute_hash("def process(x)", "pass", "Doc for process");

    let make_file = |def: Definition| FileIndex {
        file_path: "app.py".to_string(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    // Compile 1: hash mismatch (stale stored hash) — ERROR, breaker keyed to
    // the stale hash; the store syncs to `synced_hash` + previous_hashes.
    let first = engine.compile(&[make_file(bad.clone())]);
    assert!(
        first.errors.iter().any(|v| v.code == "E002"),
        "modified function's E002 must be an ERROR: {:?}",
        first.errors
    );

    // Compiles 2-4: hash now stable; previous_hashes marks the function as
    // session-modified, so E002 stays ERROR and the breaker escalates on the
    // synced-hash key: fix_hint → wider context → auto-downgrade.
    for _ in 0..2 {
        let result = engine.compile(&[make_file(bad.clone())]);
        let fired = result
            .errors
            .iter()
            .chain(result.warnings.iter())
            .any(|v| v.code == "E002");
        assert!(fired, "E002 should keep firing while unresolved");
    }
    let downgraded = engine.compile(&[make_file(bad.clone())]);
    assert_eq!(
        engine.circuit_breaker_failures("E002", &synced_hash, "app.py"),
        3
    );
    assert!(
        downgraded.warnings.iter().any(|v| v.code == "E002"),
        "E002 should be auto-downgraded to WARNING after 3 unresolved failures: {:?}",
        downgraded.warnings
    );

    // Now actually fix it: add type hints.
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
        engine.circuit_breaker_failures("E002", &synced_hash, "app.py"),
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
