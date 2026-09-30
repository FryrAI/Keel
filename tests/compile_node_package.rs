//! Issue #89: nodes `keel compile` adds carry the package `keel map` would give them.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use common::{compile_json, git, keel, violations_with_code};

const BASE_X: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n";
const FRESH_X: &str =
    "/// Existing.\npub fn existing() -> i32 { 1 }\n\n/// Fresh.\npub fn fresh() -> i32 { 2 }\n";
const CALLER_Y: &str = "/// Call fresh.\npub fn caller() -> i32 { fresh() }\n";
/// Holds a stored in-package call edge, so W009's `is_mapped()` guard is live in `crates/a/src`.
const BASE_Y: &str =
    "use crate::x::existing;\n\n/// Other.\npub fn other() -> i32 { existing() }\n";

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

    write(
        root,
        "crates/a/src/y.rs",
        &format!(
            "use crate::x::{{existing, fresh}};\n\n{}\n{CALLER_Y}",
            &BASE_Y[BASE_Y.find("///").unwrap()..]
        ),
    );
    let result = compile_json(root, "crates/a/src/y.rs");
    assert!(
        violations_with_code(&result, "W009").is_empty(),
        "false W009: {result}"
    );
    let out = keel(root, &["compile", "crates/a/src/y.rs"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(package_of(root, "fresh"), map_pkg);

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
    assert!(
        w009[0]["message"].as_str().unwrap().contains("`b` -> `a`"),
        "wrong boundary pair: {result}"
    );
}

#[test]
fn non_monorepo_compile_added_nodes_keep_null_package() {
    let dir = fixture(false);
    let root = dir.path();
    assert_eq!(package_of(root, "existing"), None);
    write(root, "crates/a/src/x.rs", FRESH_X);
    let out = keel(root, &["compile", "crates/a/src/x.rs"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "clean compile must print nothing");
    assert_eq!(package_of(root, "fresh"), None);
}

/// A NULL package stored by an older binary is healed by compiling its file, and
/// a same-crate caller then draws no W009.
#[test]
fn compile_heals_null_package_stored_by_an_older_binary() {
    let dir = fixture(true);
    let root = dir.path();
    // y.rs starts with ONLY an in-file edge, so its first call to `existing`
    // is what W009 judges.
    write(
        root,
        "crates/a/src/y.rs",
        "/// Local.\npub fn local() -> i32 { 1 }\n\n/// Other.\npub fn other() -> i32 { local() }\n",
    );
    assert!(keel(root, &["map"]).status.success());
    {
        let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
        db.execute(
            "UPDATE nodes SET package = NULL WHERE file_path = 'crates/a/src/x.rs'",
            [],
        )
        .unwrap();
    }
    assert_eq!(package_of(root, "existing"), None);
    let out = keel(root, &["compile", "crates/a/src/x.rs"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(package_of(root, "existing").as_deref(), Some("a"));

    write(
        root,
        "crates/a/src/y.rs",
        "use crate::x::existing;\n\n/// Local.\npub fn local() -> i32 { 1 }\n\n/// Other.\npub fn other() -> i32 { local() + existing() }\n",
    );
    let result = compile_json(root, "crates/a/src/y.rs");
    assert!(violations_with_code(&result, "W009").is_empty(), "{result}");
}

/// A malformed keel.json warns the same number of times as before the fix
/// (two loads) — sync loads nothing.
#[test]
fn malformed_keel_json_warning_count_is_unchanged() {
    let dir = fixture(true);
    let root = dir.path();
    fs::write(root.join(".keel/keel.json"), "{ not json").unwrap();
    write(root, "crates/a/src/x.rs", FRESH_X);
    let out = keel(root, &["compile", "crates/a/src/x.rs"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let count = stderr.matches("failed to parse").count();
    assert_eq!(count, 2, "stderr: {stderr}");
}
