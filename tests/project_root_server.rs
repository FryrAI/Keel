//! Real stdio and server roots must stay in a linked worktree, with shared storage.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use common::{git, keel, keel_bin};

fn mcp(root: &Path, tool: &str, arguments: Value) -> Value {
    let mut child = Command::new(keel_bin())
        .args(["serve", "--mcp", "--no-telemetry"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":tool, "arguments":arguments}});
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let response: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(response["result"]["isError"], false, "{response}");
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn linked_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main");
    let worktree = dir.path().join("worktree");
    fs::create_dir_all(main.join("src")).unwrap();
    for file in ["a.rs", "b.rs"] {
        fs::write(
            main.join("src").join(file),
            "/// Original.\npub fn original() {}\n",
        )
        .unwrap();
    }
    git(&main, &["init", "-q"]);
    keel(&main, &["init"]);
    git(&main, &["add", "."]);
    git(&main, &["commit", "-q", "-m", "base"]);
    assert!(keel(&main, &["map"]).status.success());
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "linked",
            worktree.to_str().unwrap(),
        ],
    );
    fs::write(main.join("src/a.rs"), "pub fn main_only() {}\n").unwrap();
    fs::write(
        worktree.join("src/b.rs"),
        "pub fn original(value: i32) -> i32 { value + 86 }\npub fn worktree_only() {}\n",
    )
    .unwrap();

    (dir, main, worktree)
}

#[test]
fn linked_worktree_mcp_reads_its_checkpoint_skeleton_and_compile() {
    let (_dir, main, worktree) = linked_fixture();
    let subdir = worktree.join("src");
    let checkpoint = mcp(&subdir, "keel/checkpoint", json!({}));
    assert_eq!(
        checkpoint["files"].as_array().unwrap().len(),
        1,
        "{checkpoint}"
    );
    assert_eq!(checkpoint["files"][0]["file"], "src/b.rs");
    assert!(checkpoint.to_string().contains("worktree_only"));
    assert!(!checkpoint.to_string().contains("main_only"));

    let skeleton = mcp(&subdir, "keel/skeleton", json!({"file":"src/b.rs"}));
    assert!(skeleton.to_string().contains("worktree_only"), "{skeleton}");
    assert!(!skeleton.to_string().contains("main_only"));

    let compiled = mcp(&subdir, "keel/compile", json!({"files":["src/b.rs"]}));
    assert!(
        compiled["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "E003" && v["file"] == "src/b.rs"),
        "{compiled}"
    );
    assert_eq!(keel_core::paths::keel_dir(&subdir), main.join(".keel"));
    assert!(!worktree.join(".keel/graph.db").exists());

    let server =
        keel_server::KeelServer::open(main.join(".keel/graph.db").to_str().unwrap(), subdir)
            .unwrap();
    assert_eq!(
        server.root_dir,
        keel_core::paths::canonicalize_portable(&worktree).unwrap()
    );
}

#[tokio::test]
async fn linked_worktree_http_and_watcher_use_root_relative_graph_paths() {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use keel_core::store::GraphStore;
    use keel_server::watcher::{apply_batch, WatchBatch};
    use tower::ServiceExt;

    let (_dir, main, worktree) = linked_fixture();
    let db = main.join(".keel/graph.db");
    let server = keel_server::KeelServer::open(db.to_str().unwrap(), worktree.join("src")).unwrap();
    let app = keel_server::http::router(server.engine.clone(), worktree.join("src"));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/compile")
                .header("content-type", "application/json")
                .body(Body::from(json!({"files":["src/b.rs"]}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["files_analyzed"], json!(["src/b.rs"]));
    assert!(
        result["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "E003" && v["file"] == "src/b.rs"),
        "{result}"
    );

    let outcome = apply_batch(
        &server.engine,
        &server.root_dir,
        &WatchBatch {
            changed: vec![worktree.join("src/b.rs")],
            removed: vec![],
        },
    )
    .unwrap();
    assert_eq!(outcome.compiled, 1);
    let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    let nodes = store.get_nodes_in_file("src/b.rs");
    assert!(nodes
        .iter()
        .any(|node| node.name == "original" && node.signature.contains("value: i32")));
    assert!(store.get_nodes_in_file("b.rs").is_empty());
    assert!(store
        .get_nodes_in_file(worktree.join("src/b.rs").to_str().unwrap())
        .is_empty());
    assert!(!store
        .get_nodes_in_file("src/a.rs")
        .iter()
        .any(|node| node.name == "main_only"));
}

#[cfg(unix)]
#[tokio::test]
async fn aliased_checkout_http_compile_and_mcp_skeleton_are_confined() {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let (dir, main, worktree) = linked_fixture();
    let alias = dir.path().join("alias");
    let subalias = dir.path().join("subalias");
    std::os::unix::fs::symlink(&worktree, &alias).unwrap();
    std::os::unix::fs::symlink(worktree.join("src"), &subalias).unwrap();
    let db = main.join(".keel/graph.db");
    for start in [&alias, &subalias] {
        let server = keel_server::KeelServer::open(db.to_str().unwrap(), start.clone()).unwrap();
        assert_eq!(
            server.root_dir,
            keel_core::paths::canonicalize_portable(&worktree).unwrap()
        );
        let app = keel_server::http::router(server.engine.clone(), start.clone());
        for file in [
            "src/b.rs".to_string(),
            alias.join("src/b.rs").to_string_lossy().to_string(),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/compile")
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"files":[file]}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let result: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(result["files_analyzed"], json!(["src/b.rs"]));
            assert!(
                result["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["code"] == "E003"),
                "{result}"
            );
        }
        let directory_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/compile")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"path":alias}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(directory_response.status(), StatusCode::OK);
        let bytes = to_bytes(directory_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let directory_result: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            directory_result["files_analyzed"]
                .as_array()
                .unwrap()
                .contains(&json!("src/b.rs")),
            "{directory_result}"
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/compile")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"files":[main.join("src/a.rs")]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let skeleton = mcp(
        &subalias,
        "keel/skeleton",
        json!({"file":alias.join("src/b.rs")}),
    );
    assert!(skeleton.to_string().contains("worktree_only"), "{skeleton}");
}
