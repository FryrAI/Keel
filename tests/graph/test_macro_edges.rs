//! End-to-end proof that Rust macro invocations never resolve to same-named
//! functions, and that a name-only cross-file match refuses to guess.
//!
//! The false positive these pin down: `format!("{x}")` used to resolve through
//! the bare name `format`, so any repo with a `fn format` collected a `calls`
//! edge from every file that formats a string. On one 25k-edge repo that made
//! a small currency formatter the graph's #1 hotspot at 1,385 phantom callers,
//! feeding E001/E004/E005 and masking genuinely dead helpers.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphEdge};
use tempfile::TempDir;

use crate::common::mapped_project;

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

/// Incoming *reference* edges of the single node named `name` in `file` — the
/// file's own `contains` edge is structural, not a caller, so it is excluded.
fn incoming_of(dir: &TempDir, file: &str, name: &str) -> Vec<GraphEdge> {
    let store = open_graph(dir);
    let node = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("`{name}` in `{file}` is in the graph after map"));
    store
        .get_edges(node.id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind != EdgeKind::Contains)
        .collect()
}

#[test]
/// A `format!` call site must contribute NO edge to a repo function named
/// `format` — the edge count between them is zero, not merely "some other
/// pair also exists".
fn test_prelude_macro_never_edges_to_same_named_function() {
    let dir = mapped_project(&[
        ("Cargo.toml", "[package]\nname = \"fixture\"\n"),
        ("src/lib.rs", "mod figures;\nmod report;\n"),
        (
            "src/figures.rs",
            "/// Formats a currency amount.\npub fn format(cents: i64) -> String {\n    \
             let whole = cents / 100;\n    whole.to_string()\n}\n",
        ),
        (
            // Formats strings constantly, never imports `figures::format`.
            "src/report.rs",
            "/// Renders a report line.\npub fn render(name: &str, n: i64) -> String {\n    \
             let head = format!(\"{name}: {n}\");\n    \
             let body = format!(\"{}\", n);\n    \
             format!(\"{head}\\n{body}\")\n}\n",
        ),
    ]);

    let incoming = incoming_of(&dir, "src/figures.rs", "format");
    assert!(
        incoming.is_empty(),
        "`fn format` must have zero incoming edges — `format!` is a prelude \
         macro, not a call into this repo: {incoming:?}"
    );
}

#[test]
/// With both `macro_rules! log` and `fn log` in the graph, a `log!()`
/// invocation produces exactly one edge, and it points at the macro.
///
/// The decoy `fn log` deliberately sits in the *caller's own* file, where the
/// same-file arm of the bang branch used to match it outright — so this fails
/// deterministically, not by whichever cross-file candidate came out of the
/// hash map first.
fn test_macro_invocation_prefers_macro_over_function() {
    let dir = mapped_project(&[
        ("Cargo.toml", "[package]\nname = \"fixture\"\n"),
        ("src/lib.rs", "mod macros;\nmod app;\n"),
        (
            "src/macros.rs",
            "macro_rules! log {\n    ($m:expr) => {\n        let _ = $m;\n    };\n}\n",
        ),
        (
            "src/app.rs",
            "/// Writes a line to the sink.\npub fn log(message: &str) -> usize {\n    \
             message.len()\n}\n\n\
             /// Runs the app.\npub fn run() {\n    log!(\"starting\");\n}\n",
        ),
    ]);

    let to_macro = incoming_of(&dir, "src/macros.rs", "log");
    assert_eq!(
        to_macro.len(),
        1,
        "`log!()` must produce exactly one edge into `macro_rules! log`: {to_macro:?}"
    );
    assert_eq!(to_macro[0].kind, EdgeKind::Calls);
    assert_eq!(to_macro[0].file_path, "src/app.rs");

    let to_function = incoming_of(&dir, "src/app.rs", "log");
    assert!(
        to_function.is_empty(),
        "`log!()` must not edge into the same-named function: {to_function:?}"
    );
}

#[test]
/// A name-only cross-file match spread across more than two files is a coin
/// flip, so keel emits no edge at all rather than picking the first hit.
fn test_name_only_match_across_many_files_emits_no_edge() {
    let macro_src = "macro_rules! shared_mac {\n    () => {\n        ()\n    };\n}\n";
    let dir = mapped_project(&[
        ("Cargo.toml", "[package]\nname = \"fixture\"\n"),
        ("src/lib.rs", "mod a;\nmod b;\nmod c;\nmod caller;\n"),
        ("src/a.rs", macro_src),
        ("src/b.rs", macro_src),
        ("src/c.rs", macro_src),
        (
            "src/caller.rs",
            "/// Uses the ambiguous macro.\npub fn go() {\n    shared_mac!();\n}\n",
        ),
    ]);

    for file in ["src/a.rs", "src/b.rs", "src/c.rs"] {
        let incoming = incoming_of(&dir, file, "shared_mac");
        assert!(
            incoming.is_empty(),
            "`shared_mac` is defined in 3 files, so the name-only match must \
             emit no edge; {file} got: {incoming:?}"
        );
    }
}
