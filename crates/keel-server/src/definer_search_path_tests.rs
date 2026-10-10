//! W013 reads SQL files from disk, so every server surface that compiles must
//! hand the engine its project root. Each case pairs a routine that must be
//! flagged with one that must not.

use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::Request;
use keel_core::sqlite::SqliteGraphStore;
use serde_json::json;
use tower::ServiceExt;

use crate::http::router;
use crate::mcp::{create_shared_engine, process_line_with_root};

const MIGRATION: &str = "migrations/0001_items.sql";
const UNSAFE: &str = "SECURITY DEFINER";
const SAFE: &str = "SECURITY DEFINER SET search_path = public, pg_temp";

/// A project whose one migration creates `touch_item` with `clauses`.
fn project(clauses: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".keel")).unwrap();
    std::fs::create_dir(dir.path().join("migrations")).unwrap();
    write_migration(dir.path(), clauses);
    dir
}

fn write_migration(root: &Path, clauses: &str) {
    let sql = format!(
        "CREATE TABLE items (id int PRIMARY KEY, seen timestamptz);\n\n\
         CREATE FUNCTION touch_item() RETURNS void\n    LANGUAGE sql {clauses}\n    \
         AS $$ UPDATE items SET seen = now() $$;\n"
    );
    std::fs::write(root.join(MIGRATION), sql).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// One MCP tool call against a disk-backed engine rooted at `root`.
fn mcp_call(root: &Path, tool: &str, arguments: serde_json::Value) -> String {
    let db = root.join(".keel/graph.db");
    let engine = create_shared_engine(Some(db.to_str().unwrap())).unwrap();
    let store = Arc::new(Mutex::new(
        SqliteGraphStore::open(db.to_str().unwrap()).unwrap(),
    ));
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}});
    process_line_with_root(&store, &engine, root, &request.to_string())
}

#[tokio::test]
async fn http_compile_reports_w013() {
    for (clauses, flagged) in [(UNSAFE, true), (SAFE, false)] {
        let dir = project(clauses);
        let db = dir.path().join(".keel/graph.db");
        let server = crate::KeelServer::open(db.to_str().unwrap(), dir.path().into()).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/compile")
            .header("content-type", "application/json")
            .body(Body::from(json!({"files": [MIGRATION]}).to_string()))
            .unwrap();
        let response = router(server.engine, server.root_dir)
            .oneshot(request)
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let body = String::from_utf8_lossy(&body);
        assert_eq!(body.contains("W013"), flagged, "{clauses}: {body}");
    }
}

#[test]
fn mcp_compile_reports_w013() {
    for (clauses, flagged) in [(UNSAFE, true), (SAFE, false)] {
        let dir = project(clauses);
        let response = mcp_call(dir.path(), "keel/compile", json!({"files": [MIGRATION]}));
        assert_eq!(response.contains("W013"), flagged, "{clauses}: {response}");
    }
}

#[test]
fn mcp_checkpoint_reports_w013() {
    for (clauses, flagged) in [(UNSAFE, true), (SAFE, false)] {
        // The committed migration has no routine; the edit under review adds one.
        let dir = project(clauses);
        let edited = std::fs::read(dir.path().join(MIGRATION)).unwrap();
        std::fs::write(dir.path().join(MIGRATION), "CREATE TABLE items (id int);\n").unwrap();
        git(dir.path(), &["init"]);
        git(dir.path(), &["add", MIGRATION]);
        git(dir.path(), &["commit", "-m", "baseline"]);
        std::fs::write(dir.path().join(MIGRATION), edited).unwrap();
        let response = mcp_call(dir.path(), "keel/checkpoint", json!({}));
        assert!(response.contains(MIGRATION), "{response}");
        assert_eq!(response.contains("W013"), flagged, "{clauses}: {response}");
    }
}
