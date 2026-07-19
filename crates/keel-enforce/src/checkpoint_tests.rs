use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, GraphEdge, GraphNode};
use keel_parsers::resolver::Definition;

use crate::types::{CompileInfo, CompileResult, Violation};

fn node(id: u64, hash: &str, name: &str, file: &str, kind: NodeKind) -> GraphNode {
    GraphNode {
        id,
        hash: hash.into(),
        kind,
        name: name.into(),
        signature: format!("fn {name}()"),
        file_path: file.into(),
        line_start: id as u32,
        line_end: id as u32 + 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn def(name: &str, signature: &str, body: &str) -> Definition {
    Definition {
        name: name.into(),
        kind: NodeKind::Function,
        signature: signature.into(),
        file_path: "src/lib.rs".into(),
        line_start: 1,
        line_end: 5,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: body.into(),
        in_test_context: false,
    }
}

fn file_index(defs: Vec<Definition>) -> FileIndex {
    FileIndex {
        file_path: "src/lib.rs".into(),
        content_hash: 0,
        definitions: defs,
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

fn empty_compile() -> CompileResult {
    CompileResult {
        version: "test".into(),
        command: "compile".into(),
        status: "ok".into(),
        files_analyzed: vec!["src/lib.rs".into()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    }
}

/// Store: module + `foo` (in src/lib.rs) called by `bar` (in src/other.rs).
fn fixture_store() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&node(
            1,
            "modhash00001",
            "lib",
            "src/lib.rs",
            NodeKind::Module,
        ))
        .unwrap();
    // `foo` gets a fixed stored hash so any fresh parse produces a different one.
    store
        .insert_node(&node(
            2,
            "STOREDFOO01",
            "foo",
            "src/lib.rs",
            NodeKind::Function,
        ))
        .unwrap();
    store
        .insert_node(&node(
            3,
            "BARHASH0001",
            "bar",
            "src/other.rs",
            NodeKind::Function,
        ))
        .unwrap();
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 3, // bar
            target_id: 2, // foo
            kind: EdgeKind::Calls,
            file_path: "src/other.rs".into(),
            line: 4,
            confidence: 1.0,
        })])
        .unwrap();
    store
}

#[test]
fn added_changed_and_callers() {
    let store = fixture_store();
    // Current parse: `foo` (body differs from stored hash → changed) + `baz` (new → added).
    let fi = file_index(vec![
        def("foo", "fn foo()", "return 1"),
        def("baz", "fn baz()", "return 2"),
    ]);
    let diff = diff_changed_files(&store, &[fi]);
    let result = build_checkpoint(
        diff,
        &empty_compile(),
        "since HEAD (working tree)".into(),
        vec!["abc123 do a thing".into()],
    );

    assert_eq!(result.command, "checkpoint");
    assert_eq!(result.files.len(), 1);
    let fd = &result.files[0];
    assert_eq!(fd.file, "src/lib.rs");
    assert!(fd.added.iter().any(|s| s.name == "baz"));
    assert!(fd.changed.iter().any(|s| s.name == "foo"));
    // `foo` changed and has a caller `bar` → structural impact reported.
    let impact = result
        .affected_callers
        .iter()
        .find(|a| a.symbol == "foo")
        .expect("foo should have affected callers");
    assert_eq!(impact.callers.len(), 1);
    assert_eq!(impact.callers[0].name, "bar");
    assert_eq!(impact.callers[0].file, "src/other.rs");
    assert_eq!(result.commits, vec!["abc123 do a thing".to_string()]);
}

#[test]
fn removed_symbol_lists_callers() {
    let store = fixture_store();
    // Current parse omits `foo` entirely → removed.
    let fi = file_index(vec![def("baz", "fn baz()", "x")]);
    let diff = diff_changed_files(&store, &[fi]);
    let result = build_checkpoint(diff, &empty_compile(), "staged".into(), vec![]);

    let fd = &result.files[0];
    assert!(fd.removed.iter().any(|s| s.name == "foo"));
    assert!(result.affected_callers.iter().any(|a| a.symbol == "foo"));
}

#[test]
fn violations_are_included() {
    let store = fixture_store();
    let fi = file_index(vec![def("foo", "fn foo()", "return 1")]);
    let mut compile = empty_compile();
    compile.errors.push(Violation {
        code: "E003".into(),
        severity: "ERROR".into(),
        category: "missing_docstring".into(),
        message: "missing docstring".into(),
        file: "src/lib.rs".into(),
        line: 1,
        hash: "h".into(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".into(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    });
    let diff = diff_changed_files(&store, &[fi]);
    let result = build_checkpoint(diff, &compile, "staged".into(), vec![]);
    assert_eq!(result.error_count, 1);
    assert_eq!(result.violations.len(), 1);
    assert_eq!(result.violations[0].code, "E003");
}

#[test]
fn no_changes_yields_empty_files() {
    let store = fixture_store();
    let diff = diff_changed_files(&store, &[]);
    let result = build_checkpoint(diff, &empty_compile(), "staged".into(), vec![]);
    assert!(result.files.is_empty());
    assert!(result.affected_callers.is_empty());
}
