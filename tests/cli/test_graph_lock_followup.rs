//! Mapped graph deletion and stdio startup while a writer holds the graph lock.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use keel_core::graph_lock;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_server::mcp::create_shared_engine;
use keel_server::watcher::{apply_batch, WatchBatch};
use serde_json::{json, Value};

fn mapped_fixture() -> tempfile::TempDir {
    crate::common::mapped_project(&[
        ("gone.ts", "/** Return a value. */\nexport function foo(): number { return bar(); }\n/** Return a constant. */\nexport function bar(): number { return 7; }\n"),
        ("keep.ts", "import { foo } from './gone';\n/** Call the other module. */\nexport function caller(): number { return foo(); }\n"),
    ])
}

#[test]
fn watcher_prunes_mapped_module_children_and_cross_file_calls() {
    let dir = mapped_fixture();
    let root = dir.path();
    let db_path = root.join(".keel/graph.db");
    let store = SqliteGraphStore::open(db_path.to_str().unwrap()).unwrap();
    let doomed = store.get_nodes_in_file("gone.ts");
    assert_eq!(doomed.len(), 3);
    let module = doomed.iter().find(|n| n.kind == NodeKind::Module).unwrap();
    assert!(doomed
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .all(|n| n.module_id == module.id));
    assert!(store.get_module_profile(module.id).is_some());
    let caller = store
        .get_nodes_in_file("keep.ts")
        .into_iter()
        .find(|n| n.name == "caller")
        .unwrap();
    assert!(
        store
            .get_edges(caller.id, EdgeDirection::Outgoing)
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && doomed.iter().any(|n| n.id == e.target_id)),
        "fixture must have a real cross-file caller edge"
    );
    let engine = create_shared_engine(Some(db_path.to_str().unwrap())).unwrap();
    std::fs::remove_file(root.join("gone.ts")).unwrap();
    let outcome = apply_batch(
        &engine,
        root,
        &WatchBatch {
            changed: vec![],
            removed: vec![root.join("gone.ts")],
        },
    )
    .unwrap();
    assert_eq!(outcome.pruned, doomed.len());
    assert_eq!(outcome.errors, 0);
    assert_eq!(outcome.compiled, 0);
    assert!(store.get_nodes_in_file("gone.ts").is_empty());
    assert!(store.get_module_profile(module.id).is_none());
    assert!(store.get_node_by_id(caller.id).is_some());
    for node in &doomed {
        assert!(store.get_edges(node.id, EdgeDirection::Both).is_empty());
    }
    assert!(store
        .get_edges(caller.id, EdgeDirection::Outgoing)
        .iter()
        .all(|e| doomed.iter().all(|n| n.id != e.target_id)));
}

struct McpChild(Child);

impl Drop for McpChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn cli_mcp_stdio_starts_and_reads_while_locked_then_compile_reports_busy() {
    let dir = mapped_fixture();
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let mut child = McpChild(
        Command::new(crate::common::keel_bin())
            .args(["serve", "--mcp", "--no-telemetry"])
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut stdin = child.0.stdin.take().unwrap();
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut exchange = |request: Value| -> Value {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let line = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("MCP must respond while the graph lock is held")
            .unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let initialized = exchange(json!({"jsonrpc":"2.0", "id":1, "method":"initialize",
        "params":{"protocolVersion":"2024-11-05", "capabilities":{},
            "clientInfo":{"name":"graph-lock-test", "version":"1"}}}));
    assert!(
        initialized["result"]["capabilities"].is_object(),
        "{initialized}"
    );
    let read = exchange(json!({"jsonrpc":"2.0", "id":2, "method":"tools/call",
        "params":{"name":"keel/search", "arguments":{"query":"foo"}}}));
    assert_ne!(read["result"]["isError"], true, "{read}");
    let result: Value =
        serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["count"], 1, "{result}");
    assert_eq!(result["results"][0]["name"], "foo");
    let compile = exchange(json!({"jsonrpc":"2.0", "id":3, "method":"tools/call",
        "params":{"name":"keel/compile", "arguments":{"files":["gone.ts"]}}}));
    assert_eq!(compile["result"]["isError"], true, "{compile}");
    assert!(compile.to_string().contains("graph busy"), "{compile}");
    drop(held);
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
    reader.join().unwrap();
}
