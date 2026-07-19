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

/// Build the fingerprints for a synthetic `src/lib.rs` holding `defs`.
fn fingerprints(defs: &[Definition]) -> FileFingerprints {
    let hashes: Vec<(String, String)> = defs
        .iter()
        .map(|d| crate::violations_util::definition_hashes(d, "src/lib.rs"))
        .collect();
    FileFingerprints::new("src/lib.rs", defs, &hashes)
}

fn def_at(name: &str, body: &str, line_start: u32, line_end: u32) -> Definition {
    let mut d = make_definition(name, &format!("fn {}()", name), body, "src/lib.rs");
    d.line_start = line_start;
    d.line_end = line_end;
    d
}

/// `(name, body, line_start, line_end)` tuples -> definitions.
fn defs_at(specs: &[(&str, &str, u32, u32)]) -> Vec<Definition> {
    specs
        .iter()
        .map(|&(name, body, start, end)| def_at(name, body, start, end))
        .collect()
}

/// One E004 violation, boxed in the single-element vec the breaker takes.
fn e004_batch() -> Vec<Violation> {
    Vec::from([e004()])
}

fn e004() -> Violation {
    Violation {
        code: "E004".to_string(),
        severity: "ERROR".to_string(),
        category: "function_removed".to_string(),
        message: "function `gone` was removed".to_string(),
        file: "src/lib.rs".to_string(),
        line: 400, // the REMOVED node's old line — nothing is defined there now
        hash: "removedhash".to_string(),
        confidence: 0.95,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("restore or update callers".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

fn advanced(v: &Violation) -> bool {
    v.fix_hint.as_deref().unwrap_or("").contains("2nd attempt")
}

/// E004 blames a node that no longer exists, so no def can sit at — or contain
/// — its line. Its breaker fingerprint is the file's def NAME SET: only a
/// structural edit (restoring, renaming, or deleting a definition) is a
/// plausible fix attempt.
///
/// The whole-file CONTENT fingerprint used previously was far too wide: three
/// saves while working on an unrelated function in the same file auto-
/// downgraded a live E004 to WARNING, flipping exit 1 to 0 and shipping the
/// broken callers.
#[test]
fn test_e004_breaker_advances_on_name_set_changes_only() {
    let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let unrelated = |body: &str| defs_at(&[("unrelated", body, 10, 20)]);

    // Attempt 1: first sighting.
    let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&unrelated("v1")));
    assert_eq!(out[0].severity, "ERROR");

    // Body edits to an UNRELATED function in the same file: the name set is
    // unchanged, so none of these is a fix attempt for the removed function.
    for body in ["v2", "v3", "v4", "v5"] {
        let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&unrelated(body)));
        let did_advance = advanced(&out[0]);
        assert_eq!(
            out[0].severity, "ERROR",
            "unrelated body edits must never downgrade a live E004"
        );
        assert!(
            !did_advance,
            "editing a different function is not a fix attempt for E004, got: {:?}",
            out[0].fix_hint
        );
    }

    // A structural edit — a definition appears — IS a plausible fix attempt.
    let with_helper = defs_at(&[("unrelated", "v5", 10, 20), ("helper", "h", 30, 40)]);
    let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&with_helper));
    let did_advance = advanced(&out[0]);
    assert!(
        did_advance,
        "a change to the file's definition NAME SET must advance the breaker, got: {:?}",
        out[0].fix_hint
    );

    // A third distinct name set, still not resolving it: auto-downgrade.
    let renamed = defs_at(&[("unrelated", "v5", 10, 20), ("helper2", "h", 30, 40)]);
    let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&renamed));
    assert_eq!(
        out[0].severity, "WARNING",
        "third failed structural attempt must auto-downgrade E004"
    );
}

/// E005 blames a CALL SITE, which lives inside some definition's line range
/// (the violation is keyed by the callee's hash, so no def starts at that
/// line). A fix attempt edits the def that contains the call — and only that.
#[test]
fn test_e005_breaker_advances_only_on_the_containing_def() {
    let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    let e005 = || Violation {
        code: "E005".to_string(),
        severity: "ERROR".to_string(),
        category: "arity_mismatch".to_string(),
        message: "call to `target` passes 2 args, expects 3".to_string(),
        file: "src/lib.rs".to_string(),
        line: 15, // inside `caller` (10..20), not at any def's first line
        hash: "calleehash".to_string(),
        confidence: 0.95,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("pass the missing argument".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    };
    // `caller` holds the call site at line 15; `other` is elsewhere in the file.
    let file = |caller_body: &str, other_body: &str| {
        defs_at(&[
            ("caller", caller_body, 10, 20),
            ("other", other_body, 30, 40),
        ])
    };

    // Attempt 1: first sighting.
    let out = engine.apply_circuit_breaker(Vec::from([e005()]), &fingerprints(&file("c1", "o1")));
    assert_eq!(out[0].severity, "ERROR");

    // Editing a DIFFERENT function is not an attempt at this call site.
    for other in ["o2", "o3", "o4"] {
        let out =
            engine.apply_circuit_breaker(Vec::from([e005()]), &fingerprints(&file("c1", other)));
        let did_advance = advanced(&out[0]);
        assert_eq!(out[0].severity, "ERROR");
        assert!(
            !did_advance,
            "editing a def that does not contain the call site is not a fix attempt"
        );
    }

    // Editing the CONTAINING def with E005 still firing IS a failed attempt.
    let out = engine.apply_circuit_breaker(Vec::from([e005()]), &fingerprints(&file("c2", "o4")));
    let did_advance = advanced(&out[0]);
    assert!(
        did_advance,
        "editing the def containing the call site must advance the breaker, got: {:?}",
        out[0].fix_hint
    );

    let out = engine.apply_circuit_breaker(Vec::from([e005()]), &fingerprints(&file("c3", "o4")));
    assert_eq!(
        out[0].severity, "WARNING",
        "third failed attempt on the containing def must auto-downgrade E005"
    );
}

/// The hardest E004 shape: deleting a function shifts its NEIGHBOR up into the
/// removed node's old line, so a line-geometry fingerprint would charge every
/// body edit of that neighbor as an E004 fix attempt — three saves while
/// iterating on the neighbor then auto-downgraded a live E004 to WARNING
/// (exit 1 → 0, broken callers ship). E004 must key on the NAME SET only,
/// even when a def now sits exactly at (or spans) the blamed line.
#[test]
fn test_e004_ignores_the_def_that_slid_into_the_removed_line() {
    let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));

    // `neighbor` slid up and now starts EXACTLY at the E004's blamed line 400
    // (and its range spans it) — the removed `gone` fn used to live there.
    let neighbor = |body: &str| defs_at(&[("neighbor", body, 400, 450)]);

    let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&neighbor("v1")));
    assert_eq!(out[0].severity, "ERROR");

    // Three body edits to the neighbor: name set unchanged → never advances.
    for body in ["v2", "v3", "v4"] {
        let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&neighbor(body)));
        assert_eq!(
            out[0].severity, "ERROR",
            "body edits to the def at the removed line must not advance E004"
        );
        assert!(
            !out[0]
                .fix_hint
                .as_deref()
                .unwrap_or("")
                .contains("2nd attempt"),
            "body edit wrongly counted as an E004 fix attempt"
        );
    }

    // A structural edit (a def added — name set changes) IS an attempt.
    let structural = defs_at(&[("neighbor", "v4", 400, 450), ("gone", "restored", 460, 470)]);
    let out = engine.apply_circuit_breaker(e004_batch(), &fingerprints(&structural));
    assert!(
        out[0]
            .fix_hint
            .as_deref()
            .unwrap_or("")
            .contains("2nd attempt"),
        "a name-set change must advance the E004 breaker, got: {:?}",
        out[0].fix_hint
    );
}
