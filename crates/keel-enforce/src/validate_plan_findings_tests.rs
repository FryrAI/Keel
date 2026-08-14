use crate::validate_plan::validate_plan;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

fn node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
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
fn short_call_target_is_never_p001() {
    // The length floor: a two-char callee the graph does not know stays silent.
    // Recall is given up here on purpose — see `prose_call_shapes_are_silent`
    // for what accepting short names actually costs.
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: run_query should call gc(rows) before returning.",
    );
    assert!(
        result.findings.is_empty(),
        "a short name must stay silent whether or not it exists: {:?}",
        result.findings
    );
}

#[test]
fn short_real_symbol_is_never_p002() {
    // The floor covers P002 too. `of` is a real symbol in plenty of graphs, and
    // "…moved out of(pkg)" in prose is not a call to it — a 0.9-confidence
    // arity finding there is exactly the false signal keel exists to remove.
    let store = store_with_execute();
    store
        .insert_node(&node(
            3,
            "OFHASH00001",
            "of",
            "of(a, b) -> Item",
            "src/db.rs",
        ))
        .unwrap();
    let result = validate_plan(
        &store,
        "Step 1: run_query pulls the row out of(pkg) before returning.",
    );
    assert!(
        result.findings.is_empty(),
        "a short name must stay silent whether or not it exists: {:?}",
        result.findings
    );
}

#[test]
fn prose_call_shapes_are_silent() {
    // Pin against re-lifting the floor: every one of these fired live. `t(...)`
    // and `cb(...)` and `f(...)` are prose or callback shorthand, `ok(value)`
    // is Rust prose (`Ok` is a builtin, `ok` is not) — none is a claim about a
    // repo symbol, and all three are indistinguishable from one.
    let store = store_with_execute();
    let result = validate_plan(
        &store,
        "Step 1: run_query renders t(\"nav.home\"), then calls cb(err, res).\n\
         Step 2: apply f(x) to each row and return ok(value).",
    );
    assert!(
        result.findings.is_empty(),
        "short prose call shapes must produce no P-findings: {:?}",
        result.findings
    );
}

#[test]
fn the_floor_holds_even_when_the_context_resolved_the_short_name() {
    // The two tests above go through `validate_plan`, whose tokenizer never
    // hands a short name to the lookup — so they would also pass with the floor
    // removed. This one closes that: it builds a context that HAS resolved `of`
    // and knows `gc` is absent, exactly what a widened lookup would produce,
    // and the floor must still gate both codes.
    use crate::checkpoint::CallerRef;
    use crate::validate_plan_findings::{detect_plan_findings, PlanContext};
    use std::collections::HashMap;

    let of = node(3, "OFHASH00001", "of", "of(a, b) -> Item", "src/db.rs");
    let anchor = node(
        2,
        "CALLERHASH1",
        "run_query",
        "run_query(sql)",
        "src/api.rs",
    );
    let symbol_node: HashMap<String, GraphNode> = HashMap::from([
        ("of".to_string(), of.clone()),
        ("run_query".to_string(), anchor),
    ]);
    let nodes_by_name: HashMap<String, Vec<GraphNode>> =
        HashMap::from([("of".to_string(), vec![of]), ("gc".to_string(), vec![])]);
    let symbol_callers: HashMap<String, Vec<CallerRef>> = HashMap::new();
    let actions: HashMap<String, (&'static str, u8)> = HashMap::new();

    let findings = detect_plan_findings(
        "Step 1: run_query pulls the row out of(pkg), then calls gc(rows).",
        &PlanContext {
            symbol_node: &symbol_node,
            nodes_by_name: &nodes_by_name,
            symbol_callers: &symbol_callers,
            actions: &actions,
        },
    );
    assert!(
        findings.is_empty(),
        "the length floor must gate P001 and P002 on its own: {findings:?}"
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

#[test]
fn proposed_function_gets_advisory_only_p003_for_a_strong_reuse_candidate() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut module = node(1, "MODULEHASH1", "time", "module", "src/time.rs");
    module.kind = NodeKind::Module;
    store.insert_node(&module).unwrap();
    store
        .insert_node(&node(
            2,
            "TIMEHASH001",
            "parse_timestamp",
            "fn parse_timestamp(value: &str) -> i64",
            "src/time.rs",
        ))
        .unwrap();

    let result = validate_plan(&store, "Add a new parse_time(value) helper.");

    let finding = result
        .findings
        .iter()
        .find(|finding| finding.code == "P003")
        .expect("strong lexical candidate should be surfaced");
    assert_eq!(finding.category, "reuse_candidate");
    assert_eq!(
        finding.actual.as_deref(),
        Some("fn parse_timestamp(value: &str) -> i64")
    );
    assert!(finding.fix_hint.contains("TIMEHASH001"));
    assert!(
        !result.has_live_findings(),
        "P003 must never participate in --strict"
    );
}

#[test]
fn semantic_only_similarity_cannot_create_p003() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut module = node(1, "MODULEHASH1", "time", "module", "src/time.rs");
    module.kind = NodeKind::Module;
    store.insert_node(&module).unwrap();
    let mut existing = node(
        2,
        "TIMEHASH001",
        "parse_timestamp",
        "fn parse_timestamp(value: &str) -> i64",
        "src/time.rs",
    );
    existing.docstring = None;
    store.insert_node(&existing).unwrap();

    let result = validate_plan(&store, "Add a new convert_unix_seconds(value) helper.");

    assert!(
        result.findings.iter().all(|finding| finding.code != "P003"),
        "semantic candidate generation is opt-in display only: {:?}",
        result.findings
    );
}
