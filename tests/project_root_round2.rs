//! Round-two regressions for canonical identities, full maps, init and display paths.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const TARGET: &str = "/// Double a value.\npub fn target(value: i32) -> i32 { value * 2 }\n";

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg/src")).unwrap();
    fs::write(dir.path().join("pkg/src/lib.rs"), TARGET).unwrap();
    git(dir.path(), &["init", "-q"]);
    assert!(keel(dir.path(), &["init", "--yes"]).status.success());
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "base"]);
    assert!(keel(dir.path(), &["map"]).status.success());
    dir
}

fn graph_counts(root: &Path) -> (i64, i64) {
    let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    db.query_row(
        "SELECT (SELECT COUNT(*) FROM nodes), (SELECT COUNT(*) FROM nodes WHERE kind = 'module')",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

/// A literal comparator output with its recorded version replaced by this build's.
fn base_output(base: &Value, command: &str) -> String {
    base[command]
        .as_str()
        .unwrap()
        .replace("\"0.6.2\"", &format!("\"{}\"", env!("CARGO_PKG_VERSION")))
}

#[cfg(unix)]
#[test]
fn directory_alias_compile_preserves_e004_and_discovery() {
    let dir = fixture();
    let root = dir.path();
    fs::write(
        root.join("pkg/src/caller.rs"),
        "use crate::target;\n/// Call target.\npub fn caller() -> i32 { target(7) }\n",
    )
    .unwrap();
    std::os::unix::fs::symlink("pkg/src", root.join("alias")).unwrap();
    assert!(keel(root, &["map"]).status.success());
    let before = graph_counts(root);
    fs::write(
        root.join("pkg/src/lib.rs"),
        "/// Replacement.\npub fn replacement() -> i32 { 21 }\n",
    )
    .unwrap();
    let out = keel(root, &["compile", "alias/lib.rs", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !common::violations_with_code(&result, "E004").is_empty(),
        "{result}"
    );
    assert_eq!(result["files_analyzed"], json!(["pkg/src/lib.rs"]));
    assert_eq!(
        graph_counts(root).1,
        before.1,
        "alias created a phantom module"
    );
    let db = rusqlite::Connection::open(root.join(".keel/graph.db")).unwrap();
    let aliases: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE file_path LIKE 'alias/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(aliases, 0);
    let discovered = keel(root, &["discover", "alias/lib.rs", "--json"]);
    assert!(discovered.status.success());
    let symbols: Value = serde_json::from_slice(&discovered.stdout).unwrap();
    assert!(symbols["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|symbol| symbol["name"] == "replacement"));
    assert!(symbols["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .all(|symbol| symbol["file"] == "pkg/src/lib.rs"));
}

#[test]
fn package_remap_keeps_root_nodes_and_removed_function_error() {
    let dir = fixture();
    let root = dir.path();
    fs::write(root.join("outside.rs"), TARGET.replace("target", "outside")).unwrap();
    fs::write(
        root.join("caller.rs"),
        "use crate::outside;\n/// Call outside.\npub fn caller() -> i32 { outside(7) }\n",
    )
    .unwrap();
    assert!(keel(root, &["map"]).status.success());
    let before = graph_counts(root);
    assert!(keel(&root.join("pkg"), &["map"]).status.success());
    assert_eq!(graph_counts(root), before);
    fs::write(
        root.join("outside.rs"),
        "/// Replacement.\npub fn replacement() -> i32 { 21 }\n",
    )
    .unwrap();
    let out = keel(root, &["compile", "outside.rs", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !common::violations_with_code(&result, "E004").is_empty(),
        "{result}"
    );
}

#[test]
fn package_init_and_update_docs_preserve_local_agent_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join(".claude")).unwrap();
    fs::write(pkg.join("lib.rs"), TARGET).unwrap();
    fs::write(root.join("outside.py"), "def outside(): pass\n").unwrap();
    git(root, &["init", "-q"]);
    let init = keel(&pkg, &["init", "--yes"]);
    assert!(init.status.success());
    assert!(String::from_utf8_lossy(&init.stderr).contains("1 language(s) detected"));
    for file in [
        "CLAUDE.md",
        "AGENTS.md",
        ".claude/settings.json",
        ".keelignore",
        ".gitignore",
    ] {
        assert!(pkg.join(file).exists(), "missing {file}");
        assert!(!root.join(file).exists(), "init moved {file} to root");
    }
    assert!(root.join(".keel/graph.db").exists());
    let settings = fs::read(pkg.join(".claude/settings.json")).unwrap();
    let marker = format!("<!-- keel:version {} -->", env!("CARGO_PKG_VERSION"));
    for file in ["CLAUDE.md", "AGENTS.md"] {
        let path = pkg.join(file);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text.replace(&marker, "<!-- keel:version 0.0.0 -->")).unwrap();
    }
    let update = keel(&pkg, &["init", "--update-docs"]);
    assert!(update.status.success());
    assert!(String::from_utf8_lossy(&update.stderr).contains("refreshed 2 doc file(s)"));
    for file in ["CLAUDE.md", "AGENTS.md"] {
        assert!(fs::read_to_string(pkg.join(file))
            .unwrap()
            .contains(&marker));
    }
    assert_eq!(
        fs::read(pkg.join(".claude/settings.json")).unwrap(),
        settings
    );
}

#[test]
fn package_audit_reads_graph_files_from_project_root() {
    let dir = fixture();
    fs::write(
        dir.path().join("pkg/src/lib.rs"),
        format!("//! Purpose: double values.\n//! Related: caller.rs\n{TARGET}"),
    )
    .unwrap();
    assert!(keel(dir.path(), &["map"]).status.success());
    let args = ["audit", "--dimension", "discoverability", "--json"];
    let root = keel(dir.path(), &args);
    let pkg = keel(&dir.path().join("pkg"), &args);
    assert!(root.status.success());
    assert!(pkg.status.success());
    assert_eq!(root.stdout, pkg.stdout);
    assert!(!String::from_utf8_lossy(&pkg.stdout).contains("missing_file_header"));
}

#[test]
fn root_file_command_display_matches_literal_base_spelling() {
    let dir = fixture();
    let absolute = dir
        .path()
        .join("pkg/src/lib.rs")
        .to_string_lossy()
        .to_string();
    let base: Value =
        serde_json::from_str(include_str!("fixtures/project_root_round2_base.json")).unwrap();
    // Literal path fields captured with the de1620c comparator (its display code
    // matches 441111c). At base, dotted analyze/context/discover failed lookup;
    // their diagnostic spelling is retained while normalized lookup now succeeds.
    for (argument, graph_display, skeleton_display) in [
        ("pkg/src/lib.rs", "pkg/src/lib.rs", "pkg/src/lib.rs"),
        ("./pkg/src/lib.rs", "./pkg/src/lib.rs", "./pkg/src/lib.rs"),
        (absolute.as_str(), "pkg/src/lib.rs", absolute.as_str()),
    ] {
        for (command, field, expected) in [
            ("analyze", "file", graph_display),
            ("context", "file", graph_display),
            ("discover", "path", graph_display),
            ("focus", "target", graph_display),
            ("skeleton", "file", skeleton_display),
        ] {
            let out = keel(dir.path(), &[command, argument, "--json"]);
            assert!(out.status.success(), "{command} {argument}");
            let result: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(result[field], expected, "{command} {argument}");
            let literal = base_output(&base, command);
            let (original, replacement) = if command == "context" {
                (
                    format!("\"{field}\":\"pkg/src/lib.rs\""),
                    format!("\"{field}\":{}", serde_json::to_string(expected).unwrap()),
                )
            } else {
                (
                    format!("\"{field}\": \"pkg/src/lib.rs\""),
                    format!("\"{field}\": {}", serde_json::to_string(expected).unwrap()),
                )
            };
            assert_eq!(
                String::from_utf8(out.stdout).unwrap(),
                literal.replacen(&original, &replacement, 1),
                "{command} {argument}"
            );
        }
    }
}

/// A mapped baseline with a working-tree addition that must print a file path.
fn changed_fixture() -> tempfile::TempDir {
    let dir = fixture();
    fs::write(
        dir.path().join("pkg/src/lib.rs"),
        format!("{TARGET}\npub fn added() {{}}\n"),
    )
    .unwrap();
    dir
}

fn changed_base() -> Value {
    // Captured independently from the de1620c 0.6.2 comparator, with the
    // undocumented public addition present before each command.
    serde_json::from_str(include_str!("fixtures/project_root_round3_base.json")).unwrap()
}

#[test]
fn root_fix_matches_literal_base_output_with_a_violation() {
    let base = changed_base();
    for spelling in ["plain", "dotted", "absolute"] {
        let dir = changed_fixture();
        let argument = match spelling {
            "plain" => "pkg/src/lib.rs".to_string(),
            "dotted" => "./pkg/src/lib.rs".to_string(),
            _ => dir
                .path()
                .join("pkg/src/lib.rs")
                .to_string_lossy()
                .to_string(),
        };
        let out = keel(dir.path(), &["fix", "--file", &argument, "--json"]);
        assert!(out.status.success());
        let result: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["violations_addressed"], 1);
        assert_eq!(result["plans"][0]["actions"][0]["file"], "pkg/src/lib.rs");
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            base_output(&base, "fix"),
            "{spelling}"
        );
    }
}

#[test]
fn root_checkpoint_matches_literal_base_output_with_a_change() {
    let dir = changed_fixture();
    let out = keel(dir.path(), &["checkpoint", "--since", "HEAD", "--json"]);
    assert!(out.status.success());
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["files"][0]["file"], "pkg/src/lib.rs");
    assert_eq!(result["violations"][0]["code"], "E003");
    assert_eq!(result["violations"][0]["file"], "pkg/src/lib.rs");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        base_output(&changed_base(), "checkpoint")
    );
}

#[test]
fn root_review_matches_literal_base_output_with_a_change() {
    let dir = changed_fixture();
    let out = keel(dir.path(), &["review", "--base", "HEAD", "--json"]);
    assert!(out.status.success());
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["files_changed"], 1);
    assert_eq!(result["changes"][0]["file"], "pkg/src/lib.rs");
    assert_eq!(result["new_violations"][0]["code"], "E003");
    assert_eq!(result["new_violations"][0]["file"], "pkg/src/lib.rs");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        base_output(&changed_base(), "review")
    );
}
