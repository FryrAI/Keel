//! Real parsed calls exercise both compile resolution and stored edges.

use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};
use keel_parsers::resolver::{FileIndex, ReferenceKind};
use keel_parsers::treesitter::TreeSitterParser;

use super::super::compile_sync::{resolve_call_targets, sync_compiled_files};
use super::super::map_lang_resolve::ResolverSet;

fn parsed(language: &str, file: &str, source: &str) -> FileIndex {
    let result = TreeSitterParser::new()
        .parse_file(language, Path::new(file), source)
        .unwrap();
    FileIndex::from_parse(file, source, result)
}

fn check_bare_and_method(language: &str, file: &str, source: &str, method: &str) {
    let mut file = parsed(language, file, source);
    assert!(file
        .definitions
        .iter()
        .any(|d| d.name == method && d.is_associated));
    let resolvers = ResolverSet {
        ts: None,
        py: None,
        go: None,
        rs: None,
    };
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = Path::new("/repo");
    sync_compiled_files(
        &mut store,
        cwd,
        std::slice::from_ref(&file),
        &resolvers,
        false,
    );
    let target = store
        .get_nodes_in_file(&file.file_path)
        .into_iter()
        .find(|n| n.name == method)
        .unwrap();
    let calls: Vec<_> = store
        .get_edges(target.id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "only the qualified/method call binds ({language})"
    );
    let bare = file
        .references
        .iter()
        .find(|r| r.name == method && r.kind == ReferenceKind::Call)
        .unwrap();
    assert_ne!(calls[0].line, bare.line);
    resolve_call_targets(&store, cwd, std::slice::from_mut(&mut file), &resolvers);
    let bare = file
        .references
        .iter()
        .find(|r| r.name == method && r.kind == ReferenceKind::Call)
        .unwrap();
    assert!(
        bare.resolved_to.is_none(),
        "bare member call must not feed E005"
    );
}

#[test]
fn rust_bare_calls_exclude_associated_functions_and_methods() {
    check_bare_and_method("rust", "a.rs", "struct Guard;\nimpl Guard {\n fn run(&self, x: i32) {}\n}\nfn wire(g: Guard) {\n run(1);\n g.run(1);\n}\n", "run");
    check_bare_and_method("rust", "a.rs", "struct Guard;\nimpl Guard {\n fn new(x: i32) -> Self { Guard }\n}\nfn wire() {\n new(1);\n Guard::new(1);\n}\n", "new");
}

#[test]
fn python_bare_calls_exclude_class_methods() {
    check_bare_and_method(
        "python",
        "a.py",
        "class Guard:\n def run(self, x):\n  return x\ndef wire(g):\n run(1)\n g.run(1)\n",
        "run",
    );
}

#[test]
fn typescript_bare_calls_exclude_class_methods() {
    check_bare_and_method("typescript", "a.ts", "class Guard {\n run(x: number): number { return x; }\n}\nfunction wire(g: Guard) {\n run(1);\n g.run(1);\n}\n", "run");
}

#[test]
fn go_bare_calls_exclude_receiver_methods() {
    check_bare_and_method("go", "a.go", "package p\ntype Guard struct{}\nfunc (g Guard) Run(x int) {}\nfunc wire(g Guard) {\n Run(1)\n g.Run(1)\n}\n", "Run");
}

#[test]
fn javascript_bare_calls_exclude_class_methods() {
    check_bare_and_method(
        "javascript",
        "a.js",
        "class Guard {\n run(x) { return x; }\n}\nfunction wire(g) {\n run(1);\n g.run(1);\n}\n",
        "run",
    );
}

#[test]
fn python_class_body_keeps_its_local_function_calls() {
    let file = parsed("python", "a.py", "class Guard:\n def helper(x):\n  return x\n value = helper(1)\n def method(self):\n  return helper(1)\nclass Other:\n value = helper(1)\n");
    let helper = file
        .definitions
        .iter()
        .find(|d| d.name == "helper")
        .unwrap();
    let calls: Vec<_> = file
        .references
        .iter()
        .filter(|r| r.name == "helper" && r.kind == ReferenceKind::Call)
        .collect();
    assert_eq!(calls.len(), 3);
    assert!(super::allows_associated(
        calls[0],
        "a.py",
        "a.py",
        helper.line_start,
        &file.definitions
    ));
    assert!(!super::allows_associated(
        calls[1],
        "a.py",
        "a.py",
        helper.line_start,
        &file.definitions
    ));
    assert!(!super::allows_associated(
        calls[2],
        "a.py",
        "a.py",
        helper.line_start,
        &file.definitions
    ));
    let resolvers = ResolverSet {
        ts: None,
        py: None,
        go: None,
        rs: None,
    };
    let mut store = SqliteGraphStore::in_memory().unwrap();
    sync_compiled_files(&mut store, Path::new("/repo"), &[file], &resolvers, false);
    let helper = store
        .get_nodes_in_file("a.py")
        .into_iter()
        .find(|n| n.name == "helper")
        .unwrap();
    let calls: Vec<_> = store
        .get_edges(helper.id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].line, 4);
}

#[test]
fn bare_cross_file_calls_cannot_bind_receiver_methods() {
    let resolvers = ResolverSet {
        ts: None,
        py: None,
        go: None,
        rs: None,
    };
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let target = parsed(
        "rust",
        "src/target.rs",
        "struct Guard;\nimpl Guard { fn run(&self) {} }\n",
    );
    let mut caller = parsed(
        "rust",
        "src/caller.rs",
        "use crate::target::run;\nfn wire() { run(1); }\n",
    );
    let cwd = Path::new("/repo");
    sync_compiled_files(
        &mut store,
        cwd,
        &[target, caller.clone()],
        &resolvers,
        false,
    );
    sync_compiled_files(
        &mut store,
        cwd,
        std::slice::from_ref(&caller),
        &resolvers,
        false,
    );
    resolve_call_targets(&store, cwd, std::slice::from_mut(&mut caller), &resolvers);
    assert!(caller.references.iter().all(|r| r.resolved_to.is_none()));
    let target = store
        .get_nodes_in_file("src/target.rs")
        .into_iter()
        .find(|n| n.name == "run")
        .unwrap();
    assert!(!store
        .get_edges(target.id, EdgeDirection::Incoming)
        .iter()
        .any(|e| e.kind == EdgeKind::Calls));
}

#[test]
fn local_replacement_requires_exactly_one_free_definition_among_members() {
    let file = parsed("rust", "src/lib.rs", "fn wire() { run(); }");
    let call = file.references.iter().find(|r| r.name == "run").unwrap();
    for (candidates, expected) in [
        (vec![(1, true, false), (2, false, true)], Some(1)),
        (
            vec![(1, true, false), (3, true, false), (2, false, true)],
            None,
        ),
        (vec![(1, true, true), (2, false, true)], None),
        (vec![(1, true, false), (2, true, false)], Some(2)),
        (vec![(2, false, true)], None),
    ] {
        assert_eq!(
            super::select_local_target(call, 2, candidates.into_iter()),
            expected
        );
    }
}
