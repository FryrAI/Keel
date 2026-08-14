//! Calibrated graph fixtures for review-time reuse advisories.

use std::collections::BTreeMap;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

use super::*;
use crate::parse_util::BlobParser;
use crate::review::diff::DiffScan;
use crate::review::{ChangeKind, ContractChange};

fn reuse_test_node(id: u64, module_id: u64, name: &str, signature: &str, file: &str) -> GraphNode {
    GraphNode {
        id,
        hash: format!("hash{id:07}"),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: signature.to_string(),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 4,
        docstring: None,
        is_public: false,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        complexity: 1,
        is_trivial_wrapper: false,
        in_test_context: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

fn insert_file(store: &SqliteGraphStore, module_id: u64, function: GraphNode) {
    let mut module = reuse_test_node(
        module_id,
        module_id,
        "module",
        "module",
        &function.file_path,
    );
    module.kind = NodeKind::Module;
    store.insert_node(&module).unwrap();
    store.insert_node(&function).unwrap();
}

fn edge(id: u64, source_id: u64, target_id: u64, file: &str, line: u32) -> GraphEdge {
    GraphEdge {
        id,
        source_id,
        target_id,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line,
        confidence: 1.0,
    }
}

fn change(name: &str, file: &str, kind: ChangeKind) -> ContractChange {
    ContractChange {
        name: name.to_string(),
        symbol_kind: NodeKind::Function,
        file: file.to_string(),
        kind,
        sig_base: None,
        sig_head: None,
        hash_base: None,
        hash_head: None,
        is_public: false,
        callers_outside_diff: vec![],
        callers_outside_diff_count: 0,
    }
}

fn scan(
    changes: Vec<ContractChange>,
    base_indices: Vec<keel_parsers::resolver::FileIndex>,
    head_indices: Vec<keel_parsers::resolver::FileIndex>,
) -> DiffScan {
    let diff_files = base_indices
        .iter()
        .chain(&head_indices)
        .map(|index| index.file_path.clone())
        .collect();
    DiffScan {
        changes,
        unanalyzed: vec![],
        diff_files,
        files_analyzed: head_indices.len(),
        base_indices,
        head_indices,
        renames: BTreeMap::new(),
    }
}

#[test]
fn detects_a_call_site_replacement_even_when_names_and_bodies_differ() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let caller = reuse_test_node(
        2,
        1,
        "caller",
        "fn caller(value: &str) -> i64",
        "src/app.rs",
    );
    let existing = reuse_test_node(
        4,
        3,
        "parse_timestamp",
        "fn parse_timestamp(value: &str) -> i64",
        "src/time.rs",
    );
    insert_file(&store, 1, caller);
    insert_file(&store, 3, existing);

    let mut parser = BlobParser::new();
    let base = parser
        .parse(
            "src/app.rs",
            "fn caller(value: &str) -> i64 {\n    parse_timestamp(value)\n}\n",
        )
        .unwrap();
    let head = parser
        .parse(
            "src/app.rs",
            "fn caller(value: &str) -> i64 {\n    to_unix_seconds(value)\n}\n\
             fn to_unix_seconds(value: &str) -> i64 { value.len() as i64 }\n",
        )
        .unwrap();
    let call_line = head
        .references
        .iter()
        .find(|reference| reference.name == "to_unix_seconds")
        .unwrap()
        .line;
    store
        .insert_node(&reuse_test_node(
            6,
            1,
            "to_unix_seconds",
            "fn to_unix_seconds(value: &str) -> i64",
            "src/app.rs",
        ))
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(edge(
            1,
            2,
            6,
            "src/app.rs",
            call_line,
        ))])
        .unwrap();
    let scan = scan(
        vec![
            change("caller", "src/app.rs", ChangeKind::BodyOnly),
            change("to_unix_seconds", "src/app.rs", ChangeKind::Added),
        ],
        vec![base],
        vec![head],
    );

    let advisories = detect(&store, &scan);

    assert_eq!(advisories.len(), 1);
    assert_eq!(advisories[0].kind, ReuseEvidenceKind::Replacement);
    assert_eq!(advisories[0].existing_symbol, "parse_timestamp");
    assert_eq!(advisories[0].confidence, 0.92);
}

#[test]
fn detects_matching_caller_and_callee_roles_without_textual_similarity() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let caller_a = reuse_test_node(
        2,
        1,
        "from_api",
        "fn from_api(value: &str) -> i64",
        "src/api.rs",
    );
    let caller_b = reuse_test_node(
        4,
        3,
        "from_job",
        "fn from_job(value: &str) -> i64",
        "src/job.rs",
    );
    let existing = reuse_test_node(
        6,
        5,
        "parse_timestamp",
        "fn parse_timestamp(value: &str) -> i64",
        "src/time.rs",
    );
    let normalize = reuse_test_node(
        8,
        7,
        "normalize",
        "fn normalize(value: &str) -> i64",
        "src/normalize.rs",
    );
    insert_file(&store, 1, caller_a);
    insert_file(&store, 3, caller_b);
    insert_file(&store, 5, existing);
    insert_file(&store, 7, normalize);
    insert_file(
        &store,
        9,
        reuse_test_node(
            10,
            9,
            "to_unix_seconds",
            "fn to_unix_seconds(value: &str) -> i64",
            "src/new_time.rs",
        ),
    );
    store
        .update_edges(vec![
            EdgeChange::Add(edge(1, 2, 10, "src/api.rs", 1)),
            EdgeChange::Add(edge(2, 4, 10, "src/job.rs", 1)),
            EdgeChange::Add(edge(3, 6, 8, "src/time.rs", 2)),
        ])
        .unwrap();

    let mut parser = BlobParser::new();
    let base_api = parser
        .parse(
            "src/api.rs",
            "fn from_api(value: &str) -> i64 {\n    let _a = value;\n    let _b = value;\n    let _c = value;\n    let _d = value;\n    parse_timestamp(value)\n}",
        )
        .unwrap();
    let base_job = parser
        .parse(
            "src/job.rs",
            "fn from_job(value: &str) -> i64 {\n    let _a = value;\n    let _b = value;\n    let _c = value;\n    let _d = value;\n    parse_timestamp(value)\n}",
        )
        .unwrap();
    let api = parser
        .parse(
            "src/api.rs",
            "fn from_api(value: &str) -> i64 { to_unix_seconds(value) }",
        )
        .unwrap();
    let job = parser
        .parse(
            "src/job.rs",
            "fn from_job(value: &str) -> i64 { to_unix_seconds(value) }",
        )
        .unwrap();
    let new = parser
        .parse(
            "src/new_time.rs",
            "fn to_unix_seconds(value: &str) -> i64 { normalize(value) }",
        )
        .unwrap();
    let scan = scan(
        vec![
            change("from_api", "src/api.rs", ChangeKind::BodyOnly),
            change("from_job", "src/job.rs", ChangeKind::BodyOnly),
            change("to_unix_seconds", "src/new_time.rs", ChangeKind::Added),
        ],
        vec![base_api, base_job],
        vec![api, job, new],
    );

    let advisories = detect(&store, &scan);

    let advisory = advisories
        .iter()
        .find(|advisory| advisory.existing_symbol == "parse_timestamp")
        .expect("same-role function should be nominated");
    assert_eq!(advisory.kind, ReuseEvidenceKind::RoleOverlap);
    assert!(advisory.confidence >= 0.85, "{advisory:?}");
}

#[test]
fn unresolved_runtime_calls_do_not_supply_graph_role_evidence() {
    let store = SqliteGraphStore::in_memory().unwrap();
    insert_file(
        &store,
        1,
        reuse_test_node(
            2,
            1,
            "render",
            "fn render(value: &str) -> bool",
            "src/render.rs",
        ),
    );
    store
        .insert_node(&reuse_test_node(
            3,
            1,
            "existing",
            "fn existing(value: &str) -> bool",
            "src/render.rs",
        ))
        .unwrap();
    store
        .insert_node(&reuse_test_node(
            4,
            1,
            "fresh",
            "fn fresh(value: &str) -> bool",
            "src/render.rs",
        ))
        .unwrap();
    insert_file(
        &store,
        5,
        reuse_test_node(
            6,
            5,
            "is_empty",
            "fn is_empty(value: &str) -> bool",
            "src/predicates.rs",
        ),
    );

    let mut parser = BlobParser::new();
    let base = parser
        .parse(
            "src/render.rs",
            "fn render(value: &str) -> bool { existing(value) }\n\
             fn existing(value: &str) -> bool { value.is_empty() }\n",
        )
        .unwrap();
    let head = parser
        .parse(
            "src/render.rs",
            "fn render(value: &str) -> bool { existing(value) || fresh(value) }\n\
             fn existing(value: &str) -> bool { value.is_empty() }\n\
             fn fresh(value: &str) -> bool { value.is_empty() }\n",
        )
        .unwrap();
    let scan = scan(
        vec![
            change("render", "src/render.rs", ChangeKind::BodyOnly),
            change("fresh", "src/render.rs", ChangeKind::Added),
        ],
        vec![base],
        vec![head],
    );

    let advisories = detect(&store, &scan);

    assert!(
        advisories.is_empty(),
        "unresolved `is_empty` calls are runtime syntax, not shared project-graph position: {advisories:?}"
    );
}

#[test]
fn calibrated_helper_extractions_with_a_new_contract_are_not_equivalence_claims() {
    let existing = reuse_test_node(
        2,
        1,
        "waf_bypass_via_requests",
        "fn waf_bypass_via_requests(url: &str) -> Option<String>",
        "tools/scrape.py",
    );
    let mut parser = BlobParser::new();
    let head = parser
        .parse(
            "tools/scrape.py",
            "def requests_waf_rescue(url: str, started: float, prefix: str) -> tuple[str, str] | None:\n    return None\n",
        )
        .unwrap();
    let added = head
        .definitions
        .iter()
        .find(|definition| definition.name == "requests_waf_rescue")
        .unwrap();

    assert!(
        !signature_compatible(added, &existing),
        "Bonago's legitimate three-argument rescue helper must not be equated with its one-argument primitive"
    );
}
