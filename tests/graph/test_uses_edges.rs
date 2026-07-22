//! End-to-end proof that the cross-file references W005 depends on actually
//! reach the graph.
//!
//! The false positive these pin down: `keel map` only ever stored `calls`
//! edges from *bare* names, so a callback passed from another file — and a
//! call written as a scoped path (`crate::m::f()`) — left its definition with
//! zero incoming edges. Compiling the defining file alone (the post-edit hook
//! path, where the referencing file is not in the batch) then reported W005 on
//! a function that is very much alive.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};
use tempfile::TempDir;

use crate::common::{compile_json, mapped_project, violations_with_code};

/// `mod.rs`: defines the callback and the function that takes one.
const OWNER_SRC: &str = "\
mod child;

fn cross_file_cb(x: i32) -> i32 {
    x + 1
}

pub fn apply(f: fn(i32) -> i32, v: i32) -> i32 {
    f(v)
}
";

/// `child.rs`: the only usage of `cross_file_cb` — passed as a value, never
/// called by name.
const CHILD_SRC: &str = "\
use super::{apply, cross_file_cb};

pub fn wire() -> i32 {
    apply(cross_file_cb, 1)
}
";

/// The two-file cross-file-callback fixture, mapped.
fn mapped_fixture() -> TempDir {
    mapped_project(&[
        ("Cargo.toml", "[package]\nname = \"fixture\"\n"),
        ("src/thing/mod.rs", OWNER_SRC),
        ("src/thing/child.rs", CHILD_SRC),
    ])
}

/// Open the mapped project's graph.
fn open_graph(dir: &TempDir) -> SqliteGraphStore {
    SqliteGraphStore::open(
        dir.path()
            .join(".keel/graph.db")
            .to_str()
            .expect("db path is utf-8"),
    )
    .expect("graph.db opens after map")
}

/// Incoming edges of the single node named `name` in `file`.
fn incoming_of(
    store: &SqliteGraphStore,
    file: &str,
    name: &str,
) -> Vec<keel_core::types::GraphEdge> {
    let node = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("`{name}` is in the graph after map"));
    store.get_edges(node.id, EdgeDirection::Incoming)
}

#[test]
/// `keel map` must record the cross-file value reference as a `uses` edge.
fn test_map_records_cross_file_value_reference_as_uses_edge() {
    let dir = mapped_fixture();
    let store = open_graph(&dir);

    let incoming = incoming_of(&store, "src/thing/mod.rs", "cross_file_cb");
    assert!(
        incoming.iter().any(|e| e.kind == EdgeKind::Uses),
        "value reference from child.rs must be stored as a uses edge: {incoming:?}"
    );
    assert!(
        !incoming.iter().any(|e| e.kind == EdgeKind::Calls),
        "a value reference must never be stored as a calls edge: {incoming:?}"
    );
}

#[test]
/// A cross-file call written as a scoped path (`crate::core_mod::helper()`,
/// the shape `#[cfg(test)] mod tests` uses) must store a real `calls` edge, so
/// compiling the callee's file alone does not report it dead.
fn test_scoped_crate_path_call_is_stored_and_not_dead() {
    let dir = mapped_project(&[
        ("Cargo.toml", "[package]\nname = \"fixture\"\n"),
        ("src/lib.rs", "mod core_mod;\nmod other;\n"),
        (
            "src/core_mod.rs",
            "fn test_only_helper() -> i32 {\n    42\n}\n",
        ),
        (
            "src/other.rs",
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        \
             let v = crate::core_mod::test_only_helper();\n        \
             assert_eq!(v, 42);\n    }\n}\n",
        ),
    ]);

    let incoming = incoming_of(&open_graph(&dir), "src/core_mod.rs", "test_only_helper");
    assert!(
        incoming.iter().any(|e| e.kind == EdgeKind::Calls),
        "scoped-path call must be stored as a calls edge: {incoming:?}"
    );

    assert_no_w005(&dir, "src/core_mod.rs");
}

#[test]
/// Compiling the defining file ALONE (the post-edit hook path) must not report
/// the callback as dead code.
fn test_w005_silent_for_cross_file_callback() {
    let dir = mapped_fixture();
    assert_no_w005(&dir, "src/thing/mod.rs");
}

/// Compile `file` on its own and assert no W005 came back.
fn assert_no_w005(dir: &TempDir, file: &str) {
    let result = compile_json(dir.path(), file);
    let dead = violations_with_code(&result, "W005");
    assert!(
        dead.is_empty(),
        "cross-file callback must not be reported dead: {dead:?}"
    );
}
