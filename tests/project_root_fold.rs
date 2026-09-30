//! Round-one regressions for map identities and cwd-relative file commands.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use common::{git, keel};
use serde_json::{json, Value};

const TARGET: &str = "/// Double a value.\npub fn target(value: i32) -> i32 { value * 2 }\n";
const CALLER: &str = "use crate::target;\n/// Call target.\npub fn caller() -> i32 { target(7) }\n";

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg/src")).unwrap();
    fs::write(dir.path().join("pkg/src/lib.rs"), TARGET).unwrap();
    fs::write(dir.path().join("pkg/src/caller.rs"), CALLER).unwrap();
    // Package maps rebuild the shared graph from the whole project root.
    fs::write(dir.path().join("outside.rs"), "pub fn outside() {}\n").unwrap();
    git(dir.path(), &["init", "-q"]);
    assert!(keel(&dir.path().join("pkg"), &["init", "--yes"])
        .status
        .success());
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    assert!(keel(&dir.path().join("pkg"), &["map"]).status.success());
    dir
}

fn graph_paths(root: &Path) -> Vec<String> {
    let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    let mut query = db
        .prepare("SELECT DISTINCT file_path FROM nodes ORDER BY file_path")
        .unwrap();
    query
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn package_init_map_removed_function_explicit_and_changed() {
    for args in [
        vec!["compile", "src/lib.rs", "--json"],
        vec!["compile", "--changed", "--json"],
    ] {
        let dir = fixture();
        let root = dir.path();
        assert_eq!(
            graph_paths(root),
            ["outside.rs", "pkg/src/caller.rs", "pkg/src/lib.rs"]
        );
        assert!(!root.join("pkg/.keel").exists());
        fs::write(
            root.join("pkg/src/lib.rs"),
            "/// Replacement.\npub fn replacement() -> i32 { 21 }\n",
        )
        .unwrap();
        let out = keel(&root.join("pkg"), &args);
        assert_eq!(out.status.code(), Some(1));
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            !common::violations_with_code(&result, "E004").is_empty(),
            "{result}"
        );
        assert!(
            common::violations_with_code(&result, "W002").is_empty(),
            "{result}"
        );
        assert_eq!(
            graph_paths(root),
            ["outside.rs", "pkg/src/caller.rs", "pkg/src/lib.rs"]
        );
        let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
        let modules: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE kind = 'module'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(modules, 3, "compile created a duplicate module");
    }
}

#[test]
fn package_file_commands_use_root_keys() {
    let dir = fixture();
    for command in ["discover", "focus", "context", "analyze", "skeleton"] {
        let from_root = keel(dir.path(), &[command, "pkg/src/lib.rs", "--json"]);
        let from_package = keel(&dir.path().join("pkg"), &[command, "src/lib.rs", "--json"]);
        assert!(from_package.status.success(), "{command}");
        assert_eq!(from_package.stdout, from_root.stdout, "{command}");
        assert!(String::from_utf8_lossy(&from_package.stdout).contains("pkg/src/lib.rs"));
    }
    // Hash-mode focus must not turn a hash into pkg/<hash>.
    let db = rusqlite::Connection::open(dir.path().join(".keel/graph.db")).unwrap();
    let hash: String = db
        .query_row("SELECT hash FROM nodes WHERE name = 'target'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        keel(dir.path(), &["focus", &hash, "--json"]).stdout,
        keel(&dir.path().join("pkg"), &["focus", &hash, "--json"]).stdout
    );
}

#[test]
fn package_fix_checkpoint_and_review_use_root_keys() {
    let dir = fixture();
    fs::write(
        dir.path().join("pkg/src/lib.rs"),
        "pub fn target(value: i32) -> i32 { value * 3 }\n",
    )
    .unwrap();
    // fix compiles against the graph, so restore its baseline between invocations.
    let db = dir.path().join(".keel/graph.db");
    let baseline = dir.path().join("baseline.db");
    fs::copy(&db, &baseline).unwrap();
    let root_fix = keel(dir.path(), &["fix", "--file", "pkg/src/lib.rs", "--json"]);
    fs::copy(&baseline, &db).unwrap();
    let pkg_fix = keel(
        &dir.path().join("pkg"),
        &["fix", "--file", "src/lib.rs", "--json"],
    );
    assert_eq!(root_fix.stdout, pkg_fix.stdout);
    assert!(String::from_utf8_lossy(&pkg_fix.stdout).contains("pkg/src/lib.rs"));
    for args in [
        vec!["checkpoint", "--json"],
        vec!["review", "--base", "HEAD", "--json"],
    ] {
        fs::copy(&baseline, &db).unwrap();
        let root = keel(dir.path(), &args);
        fs::copy(&baseline, &db).unwrap();
        let package = keel(&dir.path().join("pkg"), &args);
        assert_eq!(root.stdout, package.stdout, "{}", args[0]);
        assert!(String::from_utf8_lossy(&package.stdout).contains("pkg/src/lib.rs"));
    }
}

#[test]
fn root_map_keeps_base_path_spelling_including_boundaries() {
    let dir = fixture();
    fs::create_dir(dir.path().join("pkg/baml_src")).unwrap();
    fs::write(
        dir.path().join("pkg/baml_src/main.baml"),
        "function ExtractThing(input: string) -> string {\n}\n",
    )
    .unwrap();
    assert!(keel(&dir.path().join("pkg"), &["map"]).status.success());
    assert_eq!(
        graph_paths(dir.path()),
        [
            "outside.rs",
            "pkg/baml_src/main.baml",
            "pkg/src/caller.rs",
            "pkg/src/lib.rs"
        ]
    );
    assert!(keel(dir.path(), &["map"]).status.success());
    // These are the plain strip_prefix spellings stored by the base map.
    assert_eq!(
        graph_paths(dir.path()).join("\n").as_bytes(),
        b"outside.rs\npkg/baml_src/main.baml\npkg/src/caller.rs\npkg/src/lib.rs"
    );
}

#[test]
fn compile_reversed_explicit_order_preserves_first_occurrence() {
    let dir = fixture();
    for file in ["z.rs", "a.rs"] {
        fs::write(
            dir.path().join(file),
            format!("pub fn undocumented_{}() {{}}\n", &file[..1]),
        )
        .unwrap();
    }
    let out = keel(dir.path(), &["compile", "z.rs", "a.rs", "./z.rs", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["files_analyzed"], json!(["z.rs", "a.rs"]));
    let hits = common::violations_with_code(&result, "E003");
    assert_eq!(
        hits.iter()
            .map(|hit| hit["file"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["z.rs", "a.rs"]
    );
}
