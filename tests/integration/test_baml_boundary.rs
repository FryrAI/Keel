// Integration test: BAML boundary awareness (issue #32).
//
// A repo that declares LLM functions in `baml_src/*.baml` and calls them from
// Python via the generated `baml_client` should NOT read every such call as a
// silent unresolved edge. `keel map` must materialise the `.baml`
// function/class declarations as boundary nodes and resolve calls into them.

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

/// Create a repo with a `.baml` surface and a Python caller (no generated client).
fn setup_baml_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("baml_src")).unwrap();
    fs::write(
        root.join("baml_src/resume.baml"),
        r##"class Resume {
  name string
  skills string[]
}

function ExtractResume(resume: string) -> Resume {
  client GPT4
  prompt #"
    Extract the resume from {{ resume }}.
    {{ ctx.output_format }}
  "#
}
"##,
    )
    .unwrap();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.py"),
        r#"from baml_client import b


def process(text: str) -> str:
    result = b.ExtractResume(text)
    return result.name
"#,
    )
    .unwrap();

    dir
}

/// Create a repo whose `.baml` surface is driven from Rust by *string
/// literal* — the CLI-subprocess shape keel used to see as 31 dead schemas.
fn setup_literal_dispatch_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("baml_src")).unwrap();
    fs::write(
        root.join("baml_src/plan.baml"),
        r##"function PlanBerichtSection(input: string) -> string {
  client GPT4
  prompt #"
    Plan the section for {{ input }}.
  "#
}
"##,
    )
    .unwrap();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("llm_impl.rs"),
        r#"/// Dispatch a BAML function through the baml CLI subprocess.
pub fn run_baml(function_name: &str, input: &str) -> String {
    format!("{function_name}:{input}")
}

/// Plan one report section via the BAML boundary.
pub fn plan_section(input: &str) -> String {
    run_baml("PlanBerichtSection", input)
}

/// Route a dispatch key to the handler family that serves it.
pub fn route(kind: &str) -> &'static str {
    match kind {
        "PlanBerichtSection" => "planner",
        _ => "unknown",
    }
}

/// A literal that names nothing in the boundary index.
pub fn plan_unknown(input: &str) -> String {
    run_baml("NotABamlFunction", input)
}
"#,
    )
    .unwrap();

    dir
}

/// Run `keel init` then `keel map` in `dir`, asserting both succeed.
fn init_and_map(dir: &TempDir) -> std::process::Output {
    let keel = keel_bin();

    let init = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("keel init failed");
    assert!(init.status.success(), "init failed: {:?}", init.stderr);

    let map = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .expect("keel map failed");
    assert!(
        map.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    map
}

#[test]
fn test_baml_function_becomes_boundary_node() {
    let dir = setup_baml_project();
    init_and_map(&dir);

    let db_path = dir.path().join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap();

    let nodes = store.get_nodes_in_file("baml_src/resume.baml");
    assert!(
        !nodes.is_empty(),
        "expected boundary nodes for baml_src/resume.baml"
    );

    let func = nodes
        .iter()
        .find(|n| n.name == "ExtractResume" && n.kind == NodeKind::Function);
    assert!(
        func.is_some(),
        "expected an ExtractResume boundary Function node, got: {:?}",
        nodes.iter().map(|n| (&n.name, &n.kind)).collect::<Vec<_>>()
    );

    let class = nodes
        .iter()
        .find(|n| n.name == "Resume" && n.kind == NodeKind::Class);
    assert!(class.is_some(), "expected a Resume boundary Class node");
}

#[test]
fn test_python_call_resolves_to_baml_boundary() {
    let dir = setup_baml_project();
    init_and_map(&dir);

    let db_path = dir.path().join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap();

    let boundary_fn = store
        .get_nodes_in_file("baml_src/resume.baml")
        .into_iter()
        .find(|n| n.name == "ExtractResume" && n.kind == NodeKind::Function)
        .expect("ExtractResume boundary node must exist");

    // The `b.ExtractResume(text)` call in app.py should produce an incoming
    // Calls edge on the boundary node — i.e. it is no longer an unresolved edge.
    let incoming = store.get_edges(boundary_fn.id, EdgeDirection::Incoming);
    let call_edges: Vec<_> = incoming
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert!(
        !call_edges.is_empty(),
        "expected a resolved Calls edge into the BAML boundary function"
    );

    // The resolved call must originate from `process` in src/app.py — the
    // function whose body contains `b.ExtractResume(text)`. This proves the
    // reference resolved into the boundary AND is attributed to the enclosing
    // function: before the graph-attribution fix, in-function calls were
    // mis-attributed to the whole-file module node.
    let from_process = call_edges.iter().any(|e| {
        store
            .get_node_by_id(e.source_id)
            .map(|n| n.name == "process" && n.file_path == "src/app.py")
            .unwrap_or(false)
    });
    assert!(
        from_process,
        "the resolved BAML call should originate from the `process` function in src/app.py"
    );

    // Boundary edges are intentionally sub-0.80 confidence so they never
    // escalate to hard errors across the language boundary.
    assert!(
        call_edges.iter().all(|e| e.confidence < 0.80),
        "BAML boundary call edges must be low-confidence (warning-tier)"
    );
}

/// Open the graph a `keel map`/`keel compile` left in `dir`.
fn open_graph(dir: &TempDir) -> keel_core::sqlite::SqliteGraphStore {
    let db_path = dir.path().join(".keel/graph.db");
    keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap()
}

/// `(node, incoming uses-edge source names)` for the boundary function.
fn boundary_callers(store: &keel_core::sqlite::SqliteGraphStore) -> (u64, Vec<String>) {
    let node = store
        .get_nodes_in_file("baml_src/plan.baml")
        .into_iter()
        .find(|n| n.name == "PlanBerichtSection" && n.kind == NodeKind::Function)
        .expect("PlanBerichtSection boundary node must exist");
    let callers = store
        .get_edges(node.id, EdgeDirection::Incoming)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Uses)
        .filter_map(|e| store.get_node_by_id(e.source_id).map(|n| n.name))
        .collect();
    (node.id, callers)
}

/// T1.4: a `.baml` function named only by a string literal in Rust gains real
/// callers — the call-argument form and the match-arm form — and they are
/// `uses` edges, never `calls`.
#[test]
fn test_rust_string_literal_resolves_to_baml_boundary() {
    let dir = setup_literal_dispatch_project();
    init_and_map(&dir);
    let store = open_graph(&dir);

    let (node_id, mut callers) = boundary_callers(&store);
    callers.sort();
    assert_eq!(
        callers,
        vec!["plan_section".to_string(), "route".to_string()],
        "both literal positions must produce a caller on the .baml node"
    );

    let incoming = store.get_edges(node_id, EdgeDirection::Incoming);
    assert!(
        incoming.iter().all(|e| e.kind != EdgeKind::Calls),
        "a dispatch literal is never a call site — no `calls` edge may exist"
    );
    assert!(
        incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Uses)
            .all(|e| e.confidence < 0.80),
        "literal boundary edges stay warning-tier"
    );
}

/// `keel discover <hash of the Rust caller>` lists the `.baml` node as a
/// callee — the working path from a Rust handler to the contract it drives.
#[test]
fn test_discover_from_rust_caller_lists_baml_callee() {
    let dir = setup_literal_dispatch_project();
    init_and_map(&dir);
    let store = open_graph(&dir);

    let caller = store
        .get_nodes_in_file("src/llm_impl.rs")
        .into_iter()
        .find(|n| n.name == "plan_section")
        .expect("plan_section node must exist");

    let out = Command::new(keel_bin())
        .args(["discover", &caller.hash, "--json"])
        .current_dir(dir.path())
        .output()
        .expect("keel discover failed");
    assert!(out.status.success(), "discover exited non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PlanBerichtSection") && stdout.contains("baml_src/plan.baml"),
        "discover on the Rust caller must list the .baml node as a callee:\n{stdout}"
    );
}

/// A literal that matches no boundary name produces no reference and therefore
/// no edge: `plan_unknown` has no boundary callee, and no node is invented for
/// the string.
#[test]
fn test_unmatched_literal_produces_no_edge() {
    let dir = setup_literal_dispatch_project();
    init_and_map(&dir);
    let store = open_graph(&dir);

    let unknown = store
        .get_nodes_in_file("src/llm_impl.rs")
        .into_iter()
        .find(|n| n.name == "plan_unknown")
        .expect("plan_unknown node must exist");

    let baml_ids: Vec<u64> = store
        .get_nodes_in_file("baml_src/plan.baml")
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert!(
        store
            .get_edges(unknown.id, EdgeDirection::Outgoing)
            .iter()
            .all(|e| !baml_ids.contains(&e.target_id)),
        "an unknown literal must not reach the boundary surface"
    );
    assert!(
        store
            .find_nodes_by_name("NotABamlFunction", "", "")
            .is_empty(),
        "an unknown literal must not create a node"
    );
}

/// `keel compile` prunes and re-resolves a file's outgoing edges, so it must
/// reproduce the literal edges the map built — otherwise the first compile
/// after a map silently deletes the boundary surface's only callers.
#[test]
fn test_compile_preserves_literal_boundary_edges() {
    let dir = setup_literal_dispatch_project();
    init_and_map(&dir);

    let compile = Command::new(keel_bin())
        .args(["compile", "src/llm_impl.rs"])
        .current_dir(dir.path())
        .output()
        .expect("keel compile failed");
    assert_ne!(
        compile.status.code(),
        Some(2),
        "compile hit an internal error: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let store = open_graph(&dir);
    let (_, mut callers) = boundary_callers(&store);
    callers.sort();
    assert_eq!(
        callers,
        vec!["plan_section".to_string(), "route".to_string()],
        "compile must re-resolve the literal edges it pruned"
    );
}
