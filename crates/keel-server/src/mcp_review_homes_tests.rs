//! Homes configuration reaches MCP review, while server compile remains graph-only.

use std::fs;
use std::process::Command;
use std::sync::{Arc, Mutex};

use keel_core::config::KeelConfig;
use keel_core::sqlite::SqliteGraphStore;
use keel_enforce::engine::EnforcementEngine;
use serde_json::json;

use crate::writer::SharedEngine;

fn git(root: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=keel test",
            "-c",
            "user.email=test@keel.dev",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn homes_mcp_review_loads_rules_and_escalation_from_project_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".keel")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn query() {}\n").unwrap();
    fs::write(
        root.join(".keel/keel.json"),
        json!({
            "version": "test", "languages": ["rust"],
            "homes": [{"name": "civil-day", "patterns": ["CURRENT_DATE"], "home": "src/time.rs"}],
            "enforce": {"homes": "error", "docstrings": false, "dead_code": false}
        })
        .to_string(),
    )
    .unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    let content = "fn query() { let _ = \"CURRENT_DATE\"; }\n";
    fs::write(root.join("src/lib.rs"), content).unwrap();
    let store = Arc::new(Mutex::new(SqliteGraphStore::in_memory().unwrap()));
    let result = super::handle_review(&store, root, Some(json!({"base": "HEAD"}))).unwrap();
    let hits: Vec<_> = result["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["code"] == "E007")
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], "src/lib.rs");
    assert!(hits[0]["message"].as_str().unwrap().contains("civil-day"));

    let config = KeelConfig::load(&root.join(".keel"));
    let engine = SharedEngine::new(
        EnforcementEngine::with_config(Box::new(SqliteGraphStore::in_memory().unwrap()), &config),
        None,
    );
    let file = root.join("src/lib.rs").to_string_lossy().to_string();
    let result =
        crate::mcp_compile::handle_compile(&engine, Some(json!({"files": [file]}))).unwrap();
    assert_eq!(result["files_analyzed"], json!([file]));
    assert!(result["errors"]
        .as_array()
        .unwrap()
        .iter()
        .chain(result["warnings"].as_array().unwrap())
        .all(|v| v["code"] != "E007" && v["code"] != "W011"));
}
