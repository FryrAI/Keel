use crate::validate_plan::validate_plan;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

fn node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind: NodeKind::Function,
        name: name.into(),
        signature: sig.into(),
        file_path: file.into(),
        line_start: 40 + id as u32,
        line_end: 60 + id as u32,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// `execute(&self, sql: &str) -> Result<()>` in src/db.rs with one caller.
fn store_with_execute() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut exec = node(
        1,
        "EXECHASH001",
        "execute",
        "execute(&self, sql: &str) -> Result<()>",
        "src/db.rs",
    );
    exec.is_associated = true;
    store.insert_node(&exec).unwrap();
    store
        .insert_node(&node(
            2,
            "CALLERHASH1",
            "run_query",
            "run_query(sql: &str) -> Result<()>",
            "src/api.rs",
        ))
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: "src/api.rs".into(),
            line: 12,
            confidence: 1.0,
        })])
        .unwrap();
    store
}

#[test]
fn wrong_arity_produces_one_p002_with_real_signature() {
    let store = store_with_execute();
    let result = validate_plan(&store, "Step 1: call execute(cmd, params) from run_query.");

    let p002: Vec<_> = result
        .findings
        .iter()
        .filter(|f| f.code == "P002")
        .collect();
    assert_eq!(
        p002.len(),
        1,
        "expected exactly one P002: {:?}",
        result.findings
    );
    let f = p002[0];
    assert_eq!(f.symbol, "execute");
    assert_eq!(f.hash, "EXECHASH001");
    assert_eq!(f.file, "src/db.rs");
    assert_eq!(f.line, 41);
    assert_eq!(
        f.actual.as_deref(),
        Some("execute(&self, sql: &str) -> Result<()>")
    );
    assert!(
        f.fix_hint.contains("src/db.rs:41"),
        "fix_hint: {}",
        f.fix_hint
    );
    assert!(f.fix_hint.contains("EXECHASH001"));
    assert_eq!(f.severity, "WARNING");
    assert!(result.has_live_findings());
}

#[test]
fn receiver_is_normalized_out_on_both_sides() {
    let store = store_with_execute();
    // One non-self argument matches `execute(&self, sql: &str)`.
    let result = validate_plan(&store, "Step 1: call execute(sql) from run_query.");
    assert!(
        result.findings.is_empty(),
        "receiver-normalized match must be silent: {:?}",
        result.findings
    );
}

#[test]
fn qualified_call_on_a_method_is_checked() {
    let store = store_with_execute();
    let result = validate_plan(&store, "Step 1: use db.execute(cmd, params) in run_query.");
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].code, "P002");
}

#[test]
fn nonexistent_call_target_produces_p001() {
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: run_query should call computeTotals(rows) before returning.",
    );
    let p001: Vec<_> = result
        .findings
        .iter()
        .filter(|f| f.code == "P001")
        .collect();
    assert_eq!(p001.len(), 1, "findings: {:?}", result.findings);
    assert_eq!(p001[0].symbol, "computeTotals");
    assert!(p001[0].hash.is_empty());
    assert!(p001[0].fix_hint.contains("keel search computeTotals"));
}

#[test]
fn short_nonexistent_call_target_produces_p001() {
    // Regression: the plan tokenizer drops identifiers shorter than three
    // chars, so `gc` never reached the lookup cache and P001 read the absent
    // key as "resolved". Every call claim is resolved now, whatever its length.
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: run_query should call gc(rows) before returning.",
    );
    let p001: Vec<_> = result
        .findings
        .iter()
        .filter(|f| f.code == "P001")
        .collect();
    assert_eq!(p001.len(), 1, "findings: {:?}", result.findings);
    assert_eq!(p001[0].symbol, "gc");
}

#[test]
fn short_real_symbol_is_not_p001() {
    // The other half of the same fix: a two-char name the graph DOES know must
    // resolve, not fire.
    let store = store_with_execute();
    store
        .insert_node(&node(3, "GCHASH00001", "gc", "gc(rows)", "src/db.rs"))
        .unwrap();
    let result = validate_plan(
        &store,
        "Step 1: run_query should call gc(rows) before returning.",
    );
    assert!(
        result.findings.is_empty(),
        "a known short symbol must stay silent: {:?}",
        result.findings
    );
}

#[test]
fn correct_plan_produces_no_findings() {
    let store = store_with_execute();
    let result = validate_plan(&store, "Step 1: keep execute(sql) as is; run_query stays.");
    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert!(!result.has_live_findings());
}

#[test]
fn proposed_new_function_is_not_p001() {
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: add computeTotals(rows) to src/db.rs.\nStep 2: run_query calls computeTotals(rows).",
    );
    assert!(
        result.findings.iter().all(|f| f.symbol != "computeTotals"),
        "a function the plan proposes must not be P001: {:?}",
        result.findings
    );
}

#[test]
fn definition_syntax_marks_a_name_as_proposed() {
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: `fn computeTotals(rows)` lives in src/db.rs.\nStep 2: call computeTotals(rows).",
    );
    assert!(result.findings.iter().all(|f| f.symbol != "computeTotals"));
}

#[test]
fn stdlib_and_macro_calls_are_not_p001() {
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: in run_query use println!(\"x\"), format!(\"y\"), rows.iter().map(f), \
         serde_json::from_str(text), and Some(value).",
    );
    assert!(
        result.findings.is_empty(),
        "stdlib/macro/method calls must stay silent: {:?}",
        result.findings
    );
}

#[test]
fn elided_arguments_do_not_fire_p002() {
    let store = store_with_execute();
    let result = validate_plan(&store, "Step 1: run_query should call execute(...) once.");
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn a_plan_declaring_a_signature_change_is_not_p002() {
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: change the signature of execute to execute(sql, params).",
    );
    assert!(
        result.findings.iter().all(|f| f.code != "P002"),
        "intent to change the signature must not read as a wrong claim: {:?}",
        result.findings
    );
}

#[test]
fn empty_graph_match_produces_nothing() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let result = validate_plan(&store, "Step 1: call computeTotals(rows).");
    assert!(result.findings.is_empty());
}

#[test]
fn generic_type_commas_do_not_inflate_arity() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "GENERICHASH",
            "index_rows",
            "index_rows(map: HashMap<String, u32>) -> usize",
            "src/idx.rs",
        ))
        .unwrap();
    let result = validate_plan(&store, "Step 1: call index_rows(map) in the loop.");
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn defaulted_parameters_make_the_signature_uncomparable() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "DEFAULTHASH",
            "render_page",
            "render_page(path: str, depth: int = 3)",
            "src/render.py",
        ))
        .unwrap();
    let result = validate_plan(&store, "Step 1: call render_page(path) in the handler.");
    assert!(
        result.findings.is_empty(),
        "defaulted params must be skipped, not guessed: {:?}",
        result.findings
    );
}

#[test]
fn explicit_return_claim_mismatch_fires() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "NORETHASH01",
            "log_event",
            "log_event(msg)",
            "src/log.ts",
        ))
        .unwrap();
    let result = validate_plan(
        &store,
        "Step 1: call log_event(msg) -> string in the handler.",
    );
    let p002: Vec<_> = result
        .findings
        .iter()
        .filter(|f| f.code == "P002")
        .collect();
    assert_eq!(p002.len(), 1, "{:?}", result.findings);
    assert!(p002[0].message.contains("return type"));
}

#[test]
fn ambiguous_same_named_symbols_are_skipped() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(1, "AMBIGHASH01", "handle", "handle(a)", "src/a.rs"))
        .unwrap();
    store
        .insert_node(&node(
            2,
            "AMBIGHASH02",
            "handle",
            "handle(a, b)",
            "src/b.rs",
        ))
        .unwrap();
    let result = validate_plan(&store, "Step 1: call handle(a, b, c) in the router.");
    assert!(
        result.findings.is_empty(),
        "disagreeing candidates must be skipped: {:?}",
        result.findings
    );
}
