//! Issue #89: nodes `keel compile` adds carry the package `keel map` would give them.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use common::{compile_json, git, keel, violations_with_code};

const BASE_X: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n";
const FRESH_X: &str =
    "/// Existing.\npub fn existing() -> i32 { 1 }\n\n/// Fresh.\npub fn fresh() -> i32 { 2 }\n";
const CALLER_Y: &str =
    "use crate::x::fresh;\n\n/// Call fresh.\npub fn caller() -> i32 { fresh() }\n";
const BASE_Y: &str = "/// Other.\npub fn other() -> i32 { 3 }\n";

/// A module with a stored in-package call edge (W009's bootstrap guard needs one).
const BASE_Z: &str =
    "/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() }\n";

fn write(root: &Path, rel: &str, body: &str) {
    let full = root.join(rel);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, body).unwrap();
}

/// A two-crate cargo workspace, initialised and mapped. `monorepo` toggles the
/// workspace manifest (a plain non-workspace tree has no packages).
fn fixture(monorepo: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    if monorepo {
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        );
        for c in ["a", "b"] {
            let manifest = format!("[package]\nname = \"{c}\"\nversion = \"0.1.0\"\n");
            write(root, &format!("crates/{c}/Cargo.toml"), &manifest);
        }
    }
    write(root, "crates/a/src/x.rs", BASE_X);
    write(root, "crates/a/src/y.rs", BASE_Y);
    write(root, "crates/b/src/z.rs", BASE_Z);
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init", "--yes"]).status.success());
    assert!(keel(root, &["map"]).status.success());
    dir
}

fn package_of(root: &Path, name: &str) -> Option<String> {
    let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    db.query_row(
        "SELECT package FROM nodes WHERE name = ?1 AND kind = 'function'",
        [name],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn compile_added_node_gets_map_package_and_no_false_w009() {
    let dir = fixture(true);
    let root = dir.path();
    let map_pkg = package_of(root, "existing");
    assert_eq!(map_pkg.as_deref(), Some("a"));

    write(root, "crates/a/src/x.rs", FRESH_X);
    let out = keel(root, &["compile", "crates/a/src/x.rs"]);
    assert!(out.status.success());
    assert_eq!(package_of(root, "fresh"), map_pkg);

    write(root, "crates/a/src/y.rs", &format!("{BASE_Y}\n{CALLER_Y}"));
    let result = compile_json(root, "crates/a/src/y.rs");
    assert!(
        violations_with_code(&result, "W009").is_empty(),
        "false W009: {result}"
    );
    let out = keel(root, &["compile", "crates/a/src/y.rs"]);
    assert_eq!(out.status.code(), Some(0));

    // Parity: a fresh map stores the same package.
    assert!(keel(root, &["map"]).status.success());
    assert_eq!(package_of(root, "fresh"), map_pkg);
}

#[test]
fn genuine_cross_package_dependency_still_reports_w009() {
    let dir = fixture(true);
    let root = dir.path();
    write(root, "crates/a/src/x.rs", FRESH_X);
    assert!(keel(root, &["compile", "crates/a/src/x.rs"])
        .status
        .success());

    write(
        root,
        "crates/b/src/z.rs",
        &format!(
            "use a::x::fresh;\n{BASE_Z}\n/// Reach into a.\npub fn reach() -> i32 {{ fresh() }}\n"
        ),
    );
    let result = compile_json(root, "crates/b/src/z.rs");
    let w009 = violations_with_code(&result, "W009");
    assert!(!w009.is_empty(), "W009 must stay live: {result}");
    assert!(w009[0]["message"].as_str().unwrap().contains("b"));
}

#[test]
fn non_monorepo_compile_added_nodes_keep_null_package() {
    let dir = fixture(false);
    let root = dir.path();
    assert_eq!(package_of(root, "existing"), None);
    write(root, "crates/a/src/x.rs", FRESH_X);
    assert!(keel(root, &["compile", "crates/a/src/x.rs"])
        .status
        .success());
    assert_eq!(package_of(root, "fresh"), None);
}
