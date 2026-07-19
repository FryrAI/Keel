//! Engine-level circuit breaker tests.
//!
//! The breaker counts FIX ATTEMPTS, not compiles: passively recompiling
//! unfixed code must never march a genuine ERROR toward auto-downgrade (that
//! would be a false all-clear). Once the violation is actually resolved, its
//! breaker state clears rather than staying stuck.
//!
//! The function is MODIFIED (stored hash differs from the definition) so that
//! progressive adoption keeps E002 at ERROR — pre-existing, untouched debt is
//! progressive adoption's territory (see engine_tests_economy.rs), while the
//! breaker governs what the agent is actively working on.
use super::*;

#[test]
fn test_passive_recompiles_never_downgrade_then_reset_on_fix() {
    let store = SqliteGraphStore::in_memory().unwrap();

    // Seed the store with a STALE hash — the agent has modified this function
    // this session, so its E002 stays ERROR under progressive adoption.
    let node = make_node(1, "staleHash000", "process", "def process(x)", "app.py");
    store.insert_node(&node).unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store));

    let mut bad = make_definition("process", "def process(x)", "pass", "app.py");
    bad.type_hints_present = false;
    // The hash the store holds after the first compile syncs it.
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

    // Compile 1: hash mismatch (stale stored hash) — ERROR; the store syncs to
    // `synced_hash` + previous_hashes so the function stays "session-modified".
    let first = engine.compile(&[make_file(bad.clone())]);
    assert!(
        first.errors.iter().any(|v| v.code == "E002"),
        "modified function's E002 must be an ERROR: {:?}",
        first.errors
    );

    // Compiles 2-5: byte-identical recompiles (no fix attempted). E002 must keep
    // firing as an ERROR every time and must NOT be auto-downgraded — the whole
    // point: an ignored ERROR does not silently become a warning that exits 0.
    for attempt in 2..=5 {
        let result = engine.compile(&[make_file(bad.clone())]);
        assert!(
            result.errors.iter().any(|v| v.code == "E002"),
            "E002 must stay an ERROR on passive recompile #{attempt}, not downgrade: {:?}",
            result
        );
        assert!(
            !result.warnings.iter().any(|v| v.code == "E002"),
            "E002 must NOT be downgraded to WARNING on passive recompile #{attempt}"
        );
    }
    assert_eq!(
        engine.circuit_breaker_failures("E002", &synced_hash, "app.py"),
        1,
        "passive recompiles must not advance the failure counter past the first sighting"
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

    // Regression: reintroduce the exact same unresolved violation. It must come
    // back as a fresh ERROR, not stay in some downgraded state.
    let regressed = engine.compile(&[make_file(bad)]);
    assert!(
        regressed.errors.iter().any(|v| v.code == "E002"),
        "E002 should re-fire as a fresh ERROR after reset"
    );
}

/// E004/E005-shaped violations have no definition at (file, line) — E004
/// points at a removed node's old line, E005 at a call site keyed by the
/// callee's hash. Their breaker fingerprint must fall back to the WHOLE-FILE
/// fingerprint so edits count as fix attempts; falling back to the violation's
/// own static hash froze the counter at one strike forever.
#[test]
fn test_e004_breaker_advances_on_file_edits_via_file_fingerprint() {
    use std::collections::HashMap;

    let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let v = || Violation {
        code: "E004".to_string(),
        severity: "ERROR".to_string(),
        category: "function_removed".to_string(),
        message: "function `gone` was removed".to_string(),
        file: "src/lib.rs".to_string(),
        line: 40, // the REMOVED node's old line — nothing is defined here now
        hash: "removedhash".to_string(),
        confidence: 0.95,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("restore or update callers".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    };
    let empty: HashMap<(String, u32), String> = HashMap::new();

    // Attempt 1: first sighting.
    let out = engine.apply_circuit_breaker(vec![v()], &empty, "file-fp-1");
    assert_eq!(out[0].severity, "ERROR");

    // Passive recompile (same file fingerprint): must NOT advance.
    let out = engine.apply_circuit_breaker(vec![v()], &empty, "file-fp-1");
    assert_eq!(out[0].severity, "ERROR");
    assert!(
        !out[0]
            .fix_hint
            .as_deref()
            .unwrap_or("")
            .contains("2nd attempt"),
        "passive recompile must not count as a fix attempt"
    );

    // Edit the file (fingerprint changes) with the violation still firing:
    // that IS a failed fix attempt — 2nd strike.
    let out = engine.apply_circuit_breaker(vec![v()], &empty, "file-fp-2");
    assert!(
        out[0]
            .fix_hint
            .as_deref()
            .unwrap_or("")
            .contains("2nd attempt"),
        "an edit that leaves E004 firing must advance the breaker, got: {:?}",
        out[0].fix_hint
    );

    // Third distinct edit: auto-downgrade.
    let out = engine.apply_circuit_breaker(vec![v()], &empty, "file-fp-3");
    assert_eq!(
        out[0].severity, "WARNING",
        "third failed attempt must auto-downgrade E004"
    );
}
