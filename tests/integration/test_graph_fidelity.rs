// Integration tests: graph fidelity (issue #30).
//
// 1. The map pipeline routes ambiguous references through the per-language
//    `resolve_call_edge` (TS barrel re-exports, Python star-imports), so edges
//    carry the resolver's own confidence instead of the flat 0.80 heuristic.
// 2. `keel compile` keeps the graph fresh: new definitions are persisted,
//    vanished ones removed, and the file's call edges re-resolved — so E001
//    broken-caller checks fire right after an interface break with no re-map.

use std::fs;
use std::process::Command;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use tempfile::TempDir;

/// Path to the keel binary built by cargo.
fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    fallback
}

fn run(dir: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir.path())
        .output()
        .unwrap_or_else(|e| panic!("keel {:?} failed to spawn: {e}", args))
}

fn init(dir: &TempDir) {
    let out = run(dir, &["init"]);
    assert!(out.status.success(), "init failed: {:?}", out.stderr);
}

fn map(dir: &TempDir) {
    let out = run(dir, &["map"]);
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn open_db(dir: &TempDir) -> keel_core::sqlite::SqliteGraphStore {
    let db_path = dir.path().join(".keel/graph.db");
    keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap()
}

fn write(dir: &TempDir, rel: &str, content: &str) {
    let full = dir.path().join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(full, content).unwrap();
}

/// Incoming `calls` edges on a node named `name` in `file`.
fn incoming_calls(
    store: &keel_core::sqlite::SqliteGraphStore,
    file: &str,
    name: &str,
) -> Vec<keel_core::types::GraphEdge> {
    let node = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name && n.kind == NodeKind::Function)
        .unwrap_or_else(|| panic!("no function `{name}` in {file}"));
    store
        .get_edges(node.id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect()
}

#[test]
fn test_ts_barrel_reexport_edge_uses_resolver_confidence() {
    let dir = TempDir::new().unwrap();
    // impl.ts owns the real definition; index.ts is a barrel re-export; app.ts
    // imports through the barrel and calls it. The path heuristics would tag
    // this at 0.80 — only `resolve_call_edge` traces the barrel to 0.95.
    write(
        &dir,
        "src/impl.ts",
        "export function doThing(): number {\n  return 1;\n}\n",
    );
    write(&dir, "src/index.ts", "export { doThing } from './impl';\n");
    write(
        &dir,
        "src/app.ts",
        "import { doThing } from './index';\n\nexport function run(): number {\n  return doThing();\n}\n",
    );
    init(&dir);
    map(&dir);

    let store = open_db(&dir);
    let edges = incoming_calls(&store, "src/impl.ts", "doThing");
    assert!(
        !edges.is_empty(),
        "the barrel-imported call should resolve to an edge into impl.ts:doThing"
    );
    assert!(
        edges.iter().any(|e| e.confidence >= 0.90),
        "barrel edge must carry the resolver's confidence (>=0.90), got {:?}",
        edges.iter().map(|e| e.confidence).collect::<Vec<_>>()
    );
}

#[test]
fn test_python_star_import_edge_uses_resolver_confidence() {
    let dir = TempDir::new().unwrap();
    // A star import resolves via `resolve_call_edge`'s star handling, which
    // reports sub-0.80 confidence — distinct from the flat 0.80 heuristic.
    write(&dir, "src/utils.py", "def compute():\n    return 42\n");
    write(
        &dir,
        "src/app.py",
        "from utils import *\n\n\ndef main():\n    return compute()\n",
    );
    init(&dir);
    map(&dir);

    let store = open_db(&dir);
    let edges = incoming_calls(&store, "src/utils.py", "compute");
    assert!(
        !edges.is_empty(),
        "the star-imported call should resolve to an edge into utils.py:compute"
    );
    assert!(
        edges.iter().any(|e| e.confidence < 0.80),
        "star-import edge must carry the resolver's sub-0.80 confidence, got {:?}",
        edges.iter().map(|e| e.confidence).collect::<Vec<_>>()
    );
}

#[test]
fn test_compile_persists_new_and_removed_definitions() {
    let dir = TempDir::new().unwrap();
    write(
        &dir,
        "src/lib.py",
        "def existing() -> int:\n    return 1\n\n\ndef doomed() -> int:\n    return 2\n",
    );
    init(&dir);
    map(&dir);

    // Add `added`, drop `doomed`.
    write(
        &dir,
        "src/lib.py",
        "def existing() -> int:\n    return 1\n\n\ndef added() -> int:\n    return 3\n",
    );
    let out = run(&dir, &["compile", "src/lib.py"]);
    assert!(
        out.status.code() != Some(2),
        "compile hit an internal error: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let store = open_db(&dir);
    let names: Vec<String> = store
        .get_nodes_in_file("src/lib.py")
        .into_iter()
        .filter(|n| n.kind == NodeKind::Function)
        .map(|n| n.name)
        .collect();
    assert!(
        names.iter().any(|n| n == "added"),
        "compile must persist the new `added` definition, got {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "doomed"),
        "compile must remove the vanished `doomed` definition, got {names:?}"
    );

    // `keel discover` should find the newly-persisted node.
    let disc = run(&dir, &["discover", "src/lib.py"]);
    assert!(
        String::from_utf8_lossy(&disc.stdout).contains("added"),
        "discover should list the new `added` symbol"
    );
}

#[test]
fn test_compile_refreshes_edges_and_fires_e001_without_remap() {
    let dir = TempDir::new().unwrap();
    // Map with only the callee present — foo has no callers yet.
    write(
        &dir,
        "a.py",
        "def foo(a: int, b: int) -> int:\n    return a + b\n",
    );
    init(&dir);
    map(&dir);

    // Add a caller AFTER the map and compile it — the incremental sync must
    // persist the bar -> foo call edge even though foo lives in another file.
    write(
        &dir,
        "b.py",
        "from a import foo\n\n\ndef bar() -> int:\n    return foo(1, 2)\n",
    );
    let compile_b = run(&dir, &["compile", "b.py"]);
    assert!(
        compile_b.status.code() != Some(2),
        "compile b.py internal error: {}",
        String::from_utf8_lossy(&compile_b.stderr)
    );

    let store = open_db(&dir);
    let edges = incoming_calls(&store, "a.py", "foo");
    assert!(
        !edges.is_empty(),
        "compiling the new caller must persist a foo <- bar edge without a re-map"
    );
    drop(store);

    // Break foo's signature (arity change) and compile ONLY a.py — with no
    // fresh map. E001 must fire against the caller persisted at compile time.
    write(&dir, "a.py", "def foo(a: int) -> int:\n    return a\n");
    let compile_a = run(&dir, &["compile", "a.py"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&compile_a.stdout),
        String::from_utf8_lossy(&compile_a.stderr)
    );
    assert_eq!(
        compile_a.status.code(),
        Some(1),
        "interface-breaking edit must fail compile (E001), output: {combined}"
    );
    assert!(
        combined.contains("E001") || combined.to_lowercase().contains("caller"),
        "expected an E001 broken-caller violation, output: {combined}"
    );
}
