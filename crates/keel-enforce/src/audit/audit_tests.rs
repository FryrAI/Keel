//! Purpose: regression tests for T1.5 — the audit's noise-removal rules.
//!
//! Related: audit/mod.rs (wiring), audit/rank.rs (dedup + ranking),
//! file_class.rs (who gets graded), audit/navigation.rs (cycle policy).

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeKind};

use crate::types::{AuditOptions, AuditResult};

fn function_node(id: u64, name: &str, file: &str, line: u32) -> GraphNode {
    GraphNode {
        complexity: 0,
        id,
        hash: format!("h{id:010}"),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + 1,
        docstring: Some("doc".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn module_node(id: u64, file: &str, lines: u32) -> GraphNode {
    let mut n = function_node(id, "module", file, 1);
    n.kind = NodeKind::Module;
    n.line_end = lines;
    n
}

fn call_edge(id: u64, source_id: u64, target_id: u64, file: &str) -> EdgeChange {
    EdgeChange::Add(GraphEdge {
        id,
        source_id,
        target_id,
        kind: EdgeKind::Calls,
        file_path: file.to_string(),
        line: 1,
        confidence: 1.0,
    })
}

/// A file holding `symbols` public one-line functions, plus its module node.
fn file_with_symbols(store: &mut SqliteGraphStore, next_id: &mut u64, file: &str, symbols: u32) {
    store
        .insert_node(&module_node(*next_id, file, 100))
        .unwrap();
    *next_id += 1;
    for i in 0..symbols {
        store
            .insert_node(&function_node(
                *next_id,
                &format!("fn_{}_{}", file.len(), i),
                file,
                10 + i,
            ))
            .unwrap();
        *next_id += 1;
    }
}

fn audit(store: &SqliteGraphStore, dimension: &str) -> AuditResult {
    audit_with(store, dimension, false)
}

fn audit_with(store: &SqliteGraphStore, dimension: &str, strict_cycles: bool) -> AuditResult {
    let options = AuditOptions {
        dimension: Some(dimension.to_string()),
        strict_cycles,
        ..Default::default()
    };
    crate::audit::audit_repo(store, std::path::Path::new("."), &options, None)
}

fn checks_on(result: &AuditResult, check: &str) -> Vec<String> {
    result
        .dimensions
        .iter()
        .flat_map(|d| &d.findings)
        .filter(|f| f.check == check)
        .map(|f| f.file.clone().unwrap_or_default())
        .collect()
}

#[test]
fn god_file_skips_tests_boundaries_and_generated_code() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut id = 1;
    file_with_symbols(&mut store, &mut id, "src/big.rs", 40);
    file_with_symbols(&mut store, &mut id, "tests/big_test.rs", 40);
    file_with_symbols(&mut store, &mut id, "baml_src/big.baml", 40);
    file_with_symbols(&mut store, &mut id, "baml_client/big.py", 40);
    file_with_symbols(&mut store, &mut id, "migrations/big.sql", 40);

    let result = audit(&store, "structure");
    assert_eq!(checks_on(&result, "god_file"), vec!["src/big.rs"]);
}

#[test]
fn function_size_skips_test_files() {
    let store = SqliteGraphStore::in_memory().unwrap();
    for (id, file) in [(1u64, "src/long.rs"), (3, "tests/long_test.rs")] {
        store.insert_node(&module_node(id, file, 400)).unwrap();
        let mut f = function_node(id + 1, "long_function", file, 10);
        f.line_end = 300; // 291 lines — a FAIL on source
        store.insert_node(&f).unwrap();
    }

    let result = audit(&store, "structure");
    assert_eq!(checks_on(&result, "function_size"), vec!["src/long.rs"]);
}

#[test]
fn public_ratio_and_cryptic_name_skip_non_source_files() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut id = 1;
    // 6+ symbols, all public: a public_ratio FAIL on hand-written source.
    file_with_symbols(&mut store, &mut id, "src/api.rs", 8);
    file_with_symbols(&mut store, &mut id, "baml_src/api.baml", 8);
    file_with_symbols(&mut store, &mut id, "tests/api_test.rs", 8);

    // A cryptic (<3 char) name in each file.
    for (file, node_id) in [
        ("src/api.rs", 900u64),
        ("baml_src/api.baml", 901),
        ("tests/api_test.rs", 902),
    ] {
        store
            .insert_node(&function_node(node_id, "f", file, 50))
            .unwrap();
    }

    let result = audit(&store, "discoverability");
    assert_eq!(checks_on(&result, "public_ratio"), vec!["src/api.rs"]);
    assert_eq!(checks_on(&result, "cryptic_name"), vec!["src/api.rs"]);
}

#[test]
fn duplicate_findings_are_collapsed_before_scoring() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut id = 1;
    file_with_symbols(&mut store, &mut id, "src/big.rs", 40);
    // A second module node for the same file — what makes the walk visit it
    // twice and emit the identical finding twice.
    store
        .insert_node(&module_node(500, "src/big.rs", 100))
        .unwrap();

    let result = audit(&store, "structure");
    assert_eq!(
        checks_on(&result, "god_file").len(),
        1,
        "the same rule+file+symbol must be reported once"
    );

    let lines: Vec<String> = result
        .dimensions
        .iter()
        .flat_map(|d| &d.findings)
        .map(|f| format!("{}{}", f.check, f.message))
        .collect();
    let mut unique = lines.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(lines.len(), unique.len(), "audit emitted duplicate lines");
}

/// Two modules that call each other, in one language.
fn cyclic_pair(ext: &str) -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let a = format!("src/a.{ext}");
    let b = format!("src/b.{ext}");
    store.insert_node(&module_node(1, &a, 50)).unwrap();
    store
        .insert_node(&function_node(2, "a_fn", &a, 10))
        .unwrap();
    store.insert_node(&module_node(3, &b, 50)).unwrap();
    store
        .insert_node(&function_node(4, "b_fn", &b, 10))
        .unwrap();
    store
        .update_edges(vec![call_edge(1, 2, 4, &a), call_edge(2, 4, 2, &b)])
        .unwrap();
    store
}

#[test]
fn circular_dep_is_off_for_rust_but_on_for_other_languages() {
    let rust = cyclic_pair("rs");
    assert!(
        checks_on(&audit(&rust, "navigation"), "circular_dep").is_empty(),
        "intra-crate Rust module cycles are legal and must not be reported"
    );

    let ts = cyclic_pair("ts");
    assert!(
        !checks_on(&audit(&ts, "navigation"), "circular_dep").is_empty(),
        "TS import cycles are still a genuine load-order hazard"
    );
}

#[test]
fn strict_cycles_restores_rust_cycle_reporting() {
    let rust = cyclic_pair("rs");
    let result = audit_with(&rust, "navigation", true);
    assert!(!checks_on(&result, "circular_dep").is_empty());
}

// --- trivial_wrapper (#60) ---------------------------------------------
//
// The only audit check that needs a syntactically valid re-parse rather than
// pure graph data — these tests write real source to a `tempfile::TempDir`
// and call `check_structure` directly (not `audit_with`, whose helper
// hardcodes `Path::new(".")` and so can never see a tempdir's files).

/// Writes `content` to `rel` inside `dir`, creating parent directories.
fn write_source(dir: &std::path::Path, rel: &str, content: &str) {
    let full = dir.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(full, content).unwrap();
}

/// A store with one module node and one function node per `names` entry, all
/// attributed to `file`. Names must be unique in `file` — hashes are left
/// arbitrary, relying on `node_hash_matches`'s unique-name fallback, same
/// tolerance `check_dead_code` already accepts. Returns each function's node
/// id, in the order given.
fn wrapper_fixture(store: &mut SqliteGraphStore, file: &str, names: &[&str]) -> Vec<u64> {
    let mut id = 1u64;
    store.insert_node(&module_node(id, file, 100)).unwrap();
    id += 1;
    let mut ids = Vec::new();
    for name in names {
        store
            .insert_node(&function_node(id, name, file, 1))
            .unwrap();
        ids.push(id);
        id += 1;
    }
    ids
}

fn trivial_wrapper_findings(dir: &std::path::Path, store: &SqliteGraphStore) -> usize {
    crate::audit::structure::check_structure(store, dir, None)
        .iter()
        .filter(|f| f.check == "trivial_wrapper")
        .count()
}

const WRAP_RS: &str =
    "fn helper(x: i32) -> i32 {\n    x + 1\n}\n\nfn wrapper(x: i32) -> i32 {\n    helper(x)\n}\n";

#[test]
fn trivial_wrapper_fires_with_zero_callers() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(dir.path(), "src/wrap.rs", WRAP_RS);
    let mut store = SqliteGraphStore::in_memory().unwrap();
    wrapper_fixture(&mut store, "src/wrap.rs", &["helper", "wrapper"]);

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 1);
}

#[test]
fn trivial_wrapper_fires_with_one_caller() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(dir.path(), "src/wrap.rs", WRAP_RS);
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let ids = wrapper_fixture(&mut store, "src/wrap.rs", &["helper", "wrapper"]);
    store
        .insert_node(&function_node(900, "caller_a", "src/wrap.rs", 50))
        .unwrap();
    store
        .update_edges(vec![call_edge(1, 900, ids[1], "src/wrap.rs")])
        .unwrap();

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 1);
}

#[test]
fn trivial_wrapper_silent_with_two_callers() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(dir.path(), "src/wrap.rs", WRAP_RS);
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let ids = wrapper_fixture(&mut store, "src/wrap.rs", &["helper", "wrapper"]);
    store
        .insert_node(&function_node(900, "caller_a", "src/wrap.rs", 50))
        .unwrap();
    store
        .insert_node(&function_node(901, "caller_b", "src/wrap.rs", 51))
        .unwrap();
    store
        .update_edges(vec![
            call_edge(1, 900, ids[1], "src/wrap.rs"),
            call_edge(2, 901, ids[1], "src/wrap.rs"),
        ])
        .unwrap();

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 0);
}

#[test]
fn trivial_wrapper_skips_test_files() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(dir.path(), "tests/wrap_test.rs", WRAP_RS);
    let mut store = SqliteGraphStore::in_memory().unwrap();
    wrapper_fixture(&mut store, "tests/wrap_test.rs", &["helper", "wrapper"]);

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 0);
}

#[test]
fn trivial_wrapper_skips_decorated_python() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(
        dir.path(),
        "src/dec.py",
        "def helper(x):\n    return x + 1\n\n\n@some_decorator\ndef wrapper(x):\n    return helper(x)\n",
    );
    let mut store = SqliteGraphStore::in_memory().unwrap();
    wrapper_fixture(&mut store, "src/dec.py", &["helper", "wrapper"]);

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 0);
}

#[test]
fn trivial_wrapper_skips_associated_impl_methods() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(
        dir.path(),
        "src/imp.rs",
        "struct Foo;\n\nimpl Foo {\n    fn helper(x: i32) -> i32 {\n        x + 1\n    }\n\n    fn wrapper(x: i32) -> i32 {\n        Self::helper(x)\n    }\n}\n",
    );
    let mut store = SqliteGraphStore::in_memory().unwrap();
    wrapper_fixture(&mut store, "src/imp.rs", &["helper", "wrapper"]);

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 0);
}

#[test]
fn trivial_wrapper_skips_keel_keep_marker() {
    let dir = tempfile::TempDir::new().unwrap();
    write_source(
        dir.path(),
        "src/keep.rs",
        "fn helper(x: i32) -> i32 {\n    x + 1\n}\n\n// keel:keep\nfn wrapper(x: i32) -> i32 {\n    helper(x)\n}\n",
    );
    let mut store = SqliteGraphStore::in_memory().unwrap();
    wrapper_fixture(&mut store, "src/keep.rs", &["helper", "wrapper"]);

    assert_eq!(trivial_wrapper_findings(dir.path(), &store), 0);
}
