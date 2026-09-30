use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use keel_core::graph_lock::{self, GraphLockError};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeKind};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::http::router;
use crate::mcp::{create_shared_engine, process_line_with_root};
use crate::watcher::{apply_batch, WatchBatch};
use crate::writer::SharedEngine;
use crate::writer_test_support::state;

/// Seed a real disk graph whose function hash must change on compilation.
pub(super) fn disk_fixture(
    relative: bool,
) -> (tempfile::TempDir, SqliteGraphStore, SharedEngine, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir(&keel_dir).unwrap();
    std::fs::write(keel_dir.join("compile.lock"), "fixture").unwrap();
    let source = dir.path().join("hello.rs");
    std::fs::write(&source, "fn hello() -> i32 { 42 }\n").unwrap();
    let path = keel_dir.join("graph.db");
    let store = SqliteGraphStore::open(path.to_str().unwrap()).unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "seed_hash".into(),
            kind: NodeKind::Function,
            name: "hello".into(),
            signature: "fn hello() -> i32".into(),
            file_path: if relative {
                "hello.rs".into()
            } else {
                source.to_string_lossy().into()
            },
            line_start: 1,
            line_end: 1,
            docstring: None,
            is_public: false,
            type_hints_present: true,
            has_docstring: false,
            is_associated: false,
            complexity: 0,
            is_trivial_wrapper: false,
            in_test_context: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
            package: None,
        })
        .unwrap();
    let engine = create_shared_engine(Some(path.to_str().unwrap())).unwrap();
    (dir, store, engine, source)
}

fn compile_request(source: &std::path::Path) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/compile")
        .header("content-type", "application/json")
        .body(Body::from(json!({"files": [source]}).to_string()))
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn http_disk_compile_busy_then_writes_after_release_without_stalling_runtime() {
    // Map stores relative graph keys even when a client sends an absolute path.
    let (dir, store, engine, source) = disk_fixture(true);
    let before = state(dir.path());
    let app = router(engine, dir.path().to_path_buf());
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let start = Instant::now();
    let busy = app.clone().oneshot(compile_request(&source));
    let health = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "lock polling stalled HTTP"
        );
    };
    let (response, ()) = tokio::join!(busy, health);
    let response = response.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("graph busy"));
    assert_eq!(state(dir.path()), before);
    drop(held);
    let response = app.oneshot(compile_request(&source)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        store.get_node("seed_hash").unwrap().hash != "seed_hash",
        "released writer must update seeded hash"
    );
}

#[tokio::test]
async fn http_disk_lock_io_returns_500_without_writing() {
    let (dir, _store, engine, source) = disk_fixture(false);
    let before = state(dir.path());
    std::fs::remove_file(dir.path().join(".keel/compile.lock")).unwrap();
    std::fs::create_dir(dir.path().join(".keel/compile.lock")).unwrap();
    let response = router(engine, dir.path().to_path_buf())
        .oneshot(compile_request(&source))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(state(dir.path()), before);
}

fn mcp_busy_then_release(tool: &str) {
    let (dir, store, engine, source) = disk_fixture(true);
    let shared = Arc::new(Mutex::new(
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap(),
    ));
    let before = state(dir.path());
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":tool,"arguments":{"files":[source]}}}).to_string();
    let response: Value = serde_json::from_str(&process_line_with_root(
        &shared,
        &engine,
        dir.path(),
        &request,
    ))
    .unwrap();
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(response.to_string().contains("graph busy"));
    assert_eq!(state(dir.path()), before);
    drop(held);
    let response: Value = serde_json::from_str(&process_line_with_root(
        &shared,
        &engine,
        dir.path(),
        &request,
    ))
    .unwrap();
    assert_ne!(response["result"]["isError"], true, "{response}");
    assert!(response.get("error").is_none(), "{response}");
    assert!(store.get_node_by_id(1).unwrap().hash != "seed_hash");
}

#[test]
fn mcp_disk_compile_busy_then_writes_after_release() {
    mcp_busy_then_release("keel/compile");
}

#[test]
fn mcp_disk_fix_busy_then_writes_after_release() {
    mcp_busy_then_release("keel/fix");
}

#[test]
fn mcp_disk_lock_io_is_tool_error_without_writing() {
    let (dir, _store, engine, source) = disk_fixture(false);
    let shared = Arc::new(Mutex::new(
        SqliteGraphStore::open(dir.path().join(".keel/graph.db").to_str().unwrap()).unwrap(),
    ));
    let before = state(dir.path());
    std::fs::remove_file(dir.path().join(".keel/compile.lock")).unwrap();
    std::fs::create_dir(dir.path().join(".keel/compile.lock")).unwrap();
    for tool in ["keel/compile", "keel/fix", "keel/checkpoint"] {
        let request = json!({"jsonrpc":"2.0", "id":1, "method":tool, "params":{"files":[source]}})
            .to_string();
        let response = process_line_with_root(&shared, &engine, dir.path(), &request);
        assert!(response.contains("graph lock I/O error"), "{response}");
        assert_eq!(state(dir.path()), before);
    }
}

#[test]
fn watcher_disk_prune_and_compile_busy_then_apply_after_release() {
    let (dir, store, engine, source) = disk_fixture(true);
    // Both branches are exercised independently, and neither can write while busy.
    for prune in [false, true] {
        let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
        if prune {
            std::fs::remove_file(&source).unwrap();
        }
        let batch = if prune {
            WatchBatch {
                changed: vec![],
                removed: vec![source.clone()],
            }
        } else {
            WatchBatch {
                changed: vec![source.clone()],
                removed: vec![],
            }
        };
        let before = state(dir.path());
        assert!(matches!(
            apply_batch(&engine, dir.path(), &batch),
            Err(GraphLockError::Busy)
        ));
        assert_eq!(state(dir.path()), before);
        drop(held);
        let outcome = apply_batch(&engine, dir.path(), &batch).unwrap();
        if prune {
            assert_eq!(outcome.pruned, 1);
            assert!(store.get_nodes_in_file("hello.rs").is_empty());
        } else {
            assert_eq!(outcome.compiled, 1);
            assert!(store.get_node("seed_hash").unwrap().hash != "seed_hash");
        }
    }
}

#[tokio::test]
async fn disk_startup_succeeds_while_busy_then_first_write_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir(&keel_dir).unwrap();
    let db = keel_dir.join("graph.db");
    let held = graph_lock::try_acquire(&keel_dir).unwrap();
    let server = crate::KeelServer::open(db.to_str().unwrap(), dir.path().into()).unwrap();
    let mcp = create_shared_engine(Some(db.to_str().unwrap())).unwrap();
    assert!(db.exists());
    let before = state(dir.path());
    let response = router(server.engine, dir.path().into())
        .oneshot(compile_request(&dir.path().join("hello.rs")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(matches!(mcp.writer(), Err(GraphLockError::Busy)));
    assert_eq!(state(dir.path()), before);
    drop(held);
    assert!(mcp.try_writer().is_ok());
}

#[tokio::test]
async fn external_database_and_memory_use_project_docstring_config() {
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".keel")).unwrap();
    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.docstrings = false;
    std::fs::write(
        dir.path().join(".keel/keel.json"),
        serde_json::to_string(&config).unwrap(),
    )
    .unwrap();
    let source = dir.path().join("hello.rs");
    std::fs::write(&source, "pub fn hello() -> i32 { 42 }\n").unwrap();
    let external = cache.path().join("graph.db");
    for db in [external.to_str().unwrap(), ":memory:"] {
        let server = crate::KeelServer::open(db, dir.path().into()).unwrap();
        // The lock follows the database, while config follows the project.
        let project_lock = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
        let response = router(server.engine, dir.path().into())
            .oneshot(compile_request(&source))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("E003"), "{body:?}");
        drop(project_lock);
    }
}

#[test]
fn mcp_disk_open_fallback_records_memory_backend() {
    let dir = tempfile::tempdir().unwrap();
    // SQLite cannot open a directory as a database. The context must not lock
    // its candidate disk directory after falling back to a real memory store.
    let db = dir.path().join("graph.db");
    std::fs::create_dir(&db).unwrap();
    let engine = create_shared_engine(Some(db.to_str().unwrap())).unwrap();
    let _held = graph_lock::try_acquire(dir.path()).unwrap();
    assert!(engine.try_writer().is_ok());
}

#[test]
fn graph_lock_is_acquired_before_engine_mutex() {
    let (dir, _store, engine, _source) = disk_fixture(false);
    let graph = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let mutex = engine.lock().unwrap();
    let contender = engine.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let busy = matches!(contender.writer(), Err(GraphLockError::Busy));
        tx.send(busy).unwrap();
    });
    // If the ordering regresses, the worker waits on the held engine mutex
    // instead of reaching its bounded graph-lock timeout. Release both before
    // asserting so even that failure cannot leak a blocked thread.
    let bounded = rx.recv_timeout(Duration::from_secs(3));
    drop(mutex);
    drop(graph);
    worker.join().unwrap();
    assert!(bounded.unwrap());
}

#[test]
fn sqlite_memory_paths_do_not_acquire_disk_locks() {
    assert_eq!(
        crate::writer::disk_lock_dir("graph.db"),
        Some(PathBuf::from("."))
    );
    for path in [":memory:", ""] {
        assert!(crate::writer::disk_lock_dir(path).is_none());
        assert!(create_shared_engine(Some(path))
            .unwrap()
            .try_writer()
            .is_ok());
        assert!(crate::KeelServer::open(path, PathBuf::from("."))
            .unwrap()
            .engine
            .try_writer()
            .is_ok());
    }
}
