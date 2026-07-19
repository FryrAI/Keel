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

    // The resolved call must originate from the calling source file (app.py),
    // proving the `b.ExtractResume(...)` reference resolved into the boundary
    // instead of remaining a silent unresolved edge. (keel attributes the
    // caller to the containing top-level definition, which for the whole-file
    // scope is the module node of `src/app.py`.)
    let from_app = call_edges.iter().any(|e| {
        store
            .get_node_by_id(e.source_id)
            .map(|n| n.file_path == "src/app.py")
            .unwrap_or(false)
    });
    assert!(
        from_app,
        "the resolved BAML call should originate from src/app.py"
    );

    // Boundary edges are intentionally sub-0.80 confidence so they never
    // escalate to hard errors across the language boundary.
    assert!(
        call_edges.iter().all(|e| e.confidence < 0.80),
        "BAML boundary call edges must be low-confidence (warning-tier)"
    );
}
