//! Compile regressions for local module calls and cross-language candidates.

use std::fs;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind};

use super::test_bare_call_binding::{fixture, incoming_calls};
use crate::common::{assert_no_violation, compile_json, keel};

const RUST_SYMBOLS: &str = "/// Build a symbol row.\npub fn sym(name: &str, sig: &str, line: u32, doc: &str) -> String { name.to_string() }\n/// Another Rust-only symbol.\npub fn rust_only(a: i32, b: i32) {}\n";
const PYTHON_SYM: &str =
    "def sym(name: str) -> str:\n    \"\"\"Build a Python symbol.\"\"\"\n    return name\n";

fn write(root: &Path, file: &str, source: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn clean_compile(root: &Path, file: &str) {
    let out = keel(root, &["compile", file, "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let result = compile_json(root, file);
    assert_no_violation(&result, "E005");
}

fn assert_no_rust_edges(root: &Path, caller: &str) {
    let store = SqliteGraphStore::open(root.join(".keel/graph.db").to_str().unwrap()).unwrap();
    for node in store.get_nodes_in_file(caller) {
        for edge in store.get_edges(node.id, EdgeDirection::Outgoing) {
            if matches!(edge.kind, EdgeKind::Calls | EdgeKind::Uses) {
                let target = store.get_node_by_id(edge.target_id).unwrap();
                assert!(!target.file_path.ends_with(".rs"), "{caller}: {target:?}");
            }
        }
    }
}

#[test]
fn new_python_module_call_binds_parsed_local_instead_of_rust() {
    let dir = fixture(&[("src/lib.rs", RUST_SYMBOLS)]);
    let file = "tools/m1.py";
    write(
        dir.path(),
        file,
        &format!("{PYTHON_SYM}\nX = sym(\"run\")\n"),
    );
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
    // Compile sync attributes edges only to containing definitions. Module
    // calls may therefore have no edge; any stored edge must stay in Python.
    assert!(incoming_calls(dir.path(), file, "sym", false) <= 1);
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_eq!(incoming_calls(dir.path(), file, "sym", false), 1);
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
}

#[test]
fn new_python_module_and_function_calls_never_bind_unique_rust_names() {
    let dir = fixture(&[("src/lib.rs", RUST_SYMBOLS)]);
    let file = "tools/caller.py";
    write(dir.path(), file, "from unavailable_package import rust_only\nX = sym(\"run\")\ndef main() -> None:\n    \"\"\"Call an unrelated unique name.\"\"\"\n    rust_only(1)\n");
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "sym", false), 0);
    assert_eq!(
        incoming_calls(dir.path(), "src/lib.rs", "rust_only", false),
        0
    );
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_no_rust_edges(dir.path(), file);
}

#[test]
fn same_directory_and_imported_python_calls_reject_rust_targets() {
    for source in [
        "def main() -> None:\n    \"\"\"Call.\"\"\"\n    sym(\"run\")\n",
        "from lib import sym\ndef main() -> None:\n    \"\"\"Call.\"\"\"\n    sym(\"run\")\n",
    ] {
        let dir = fixture(&[("src/lib.rs", RUST_SYMBOLS)]);
        write(dir.path(), "src/caller.py", source);
        clean_compile(dir.path(), "src/caller.py");
        assert_no_rust_edges(dir.path(), "src/caller.py");
        assert!(keel(dir.path(), &["map"]).status.success());
        assert_no_rust_edges(dir.path(), "src/caller.py");
    }
}

#[test]
fn in_function_local_and_after_map_module_calls_keep_python_target() {
    let dir = fixture(&[("src/lib.rs", RUST_SYMBOLS), ("src/local.py", PYTHON_SYM)]);
    let file = "src/local.py";
    write(dir.path(), file, &format!("{PYTHON_SYM}\ndef main() -> str:\n    \"\"\"Call locally.\"\"\"\n    return sym(\"run\")\n"));
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
    assert_eq!(incoming_calls(dir.path(), file, "sym", false), 1);
    write(
        dir.path(),
        file,
        &format!("{PYTHON_SYM}\nX = sym(\"run\")\n"),
    );
    assert!(keel(dir.path(), &["map"]).status.success());
    assert_eq!(incoming_calls(dir.path(), file, "sym", false), 1);
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
}

#[test]
fn typescript_calls_keep_tsx_svelte_and_astro_family_targets() {
    for (file, source) in [
        ("src/target.tsx", "export function render(x: number): number { return x; }\n"),
        ("src/target.svelte", "<script lang=\"ts\">\nexport function render(x: number): number { return x; }\n</script>\n"),
        ("src/target.astro", "---\nexport function render(x: number): number { return x; }\n---\n"),
    ] {
        let dir = fixture(&[(file, source), ("src/caller.ts", "import { render } from './target';\n/** Call. */\nexport function wire(): number { return render(1); }\n")]);
        assert_eq!(incoming_calls(dir.path(), file, "render", false), 1);
        clean_compile(dir.path(), "src/caller.ts");
        assert_eq!(incoming_calls(dir.path(), file, "render", false), 1);
    }
}

#[test]
fn python_calls_keep_baml_boundary_targets() {
    let dir = fixture(&[
        (
            "src/caller.py",
            "def main() -> None:\n    \"\"\"Call the model.\"\"\"\n    b.ExtractResume(\"text\")\n",
        ),
        (
            "baml_src/main.baml",
            "function ExtractResume(text: string) -> string {\n  client GPT4\n}\n",
        ),
    ]);
    assert_eq!(
        incoming_calls(dir.path(), "baml_src/main.baml", "ExtractResume", false),
        1
    );
    clean_compile(dir.path(), "src/caller.py");
    assert_eq!(
        incoming_calls(dir.path(), "baml_src/main.baml", "ExtractResume", false),
        1
    );
}

#[test]
fn fresh_in_function_local_keeps_sync_binding_after_foreign_arity_rejection() {
    let dir = fixture(&[("src/lib.rs", RUST_SYMBOLS)]);
    let file = "tools/inner.py";
    write(dir.path(), file, &format!("{PYTHON_SYM}\ndef main() -> str:\n    \"\"\"Call the fresh local.\"\"\"\n    return sym(\"run\")\n"));
    clean_compile(dir.path(), file);
    assert_no_rust_edges(dir.path(), file);
    assert_eq!(incoming_calls(dir.path(), file, "sym", false), 1);
}
