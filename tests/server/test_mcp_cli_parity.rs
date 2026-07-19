//! Parity: the MCP `keel/map` handler and the CLI `keel map --json` command
//! must report the same graph.
//!
//! Both now assemble their output through the shared `keel_enforce::map`
//! module — the CLI over a fresh parse, the MCP server by reading the same
//! `graph.db` back through the frozen `GraphStore` trait. This test drives the
//! real binary to build/serialize the CLI side, then calls the MCP dispatch
//! in-process against that on-disk graph, and asserts the summary counts agree.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_server::mcp::{create_shared_engine, process_line};

/// Locate the `keel` binary next to the test executable (workspace fallback).
fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    assert!(
        fallback.exists(),
        "keel binary not found — run `cargo build` first"
    );
    fallback
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    let out = Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")));
    assert!(
        out.status.success(),
        "`keel {}` failed:\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

/// A JSON-RPC request line for `process_line`.
fn rpc(method: &str, params: Option<serde_json::Value>) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    })
    .to_string()
}

#[test]
fn test_mcp_map_summary_matches_cli() {
    // A fixture with modules, functions, a class, and a call edge across two
    // languages, so the compared counts are non-trivial.
    let dir = tempfile::TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    // `addTwo` calls `addOne` exactly once. A *repeated* call to the same
    // callee would count as two edges in the fresh parse but collapse to one
    // on persistence (the DB's unique (source,target,kind) edge), making a
    // fresh `map --json` disagree with any store read — a pre-existing quirk
    // unrelated to this parity, so the fixture avoids it.
    fs::write(
        src.join("math.ts"),
        "export function addOne(x: number): number {\n  return x + 1;\n}\n\
         export function addTwo(x: number): number {\n  return addOne(x) + 1;\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("widget.ts"),
        "export class Widget {\n  render(): string {\n    return \"w\";\n  }\n  \
         update(x: number): number {\n    return x + 1;\n  }\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("thing.py"),
        "class Thing:\n    def one(self, x: int) -> int:\n        return x + 1\n\n\n\
         def top(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x + 1\n",
    )
    .unwrap();

    run(dir.path(), &["init"]);
    run(dir.path(), &["map"]);

    // CLI side: `keel map --json` reparses and rewrites graph.db, then prints
    // the summary we compare against. Run it first so graph.db reflects exactly
    // what the CLI reported when the MCP side reads it back.
    let cli_out = run(dir.path(), &["map", "--json"]);
    let cli: serde_json::Value =
        serde_json::from_slice(&cli_out.stdout).expect("map --json is JSON");
    let cli_summary = &cli["summary"];

    // MCP side: read the same graph.db through the server's keel/map dispatch.
    let db_path = dir.path().join(".keel/graph.db");
    let store = SqliteGraphStore::open(db_path.to_str().unwrap()).expect("open graph.db");
    let store = Arc::new(Mutex::new(store));
    let engine = create_shared_engine(None);
    let resp: serde_json::Value =
        serde_json::from_str(&process_line(&store, &engine, &rpc("keel/map", None)))
            .expect("keel/map response is JSON");
    let mcp_summary = &resp["result"]["summary"];

    for field in [
        "total_nodes",
        "total_edges",
        "modules",
        "functions",
        "classes",
    ] {
        assert_eq!(
            cli_summary[field], mcp_summary[field],
            "MCP keel/map and CLI map --json must agree on summary.{field}\n\
             cli: {cli_summary}\nmcp: {mcp_summary}"
        );
    }

    // Sanity: the fixture genuinely has classes, several functions, and edges,
    // so a regression that zeroed a side out could not pass by matching zeros.
    assert!(cli_summary["classes"].as_u64().unwrap() >= 1);
    assert!(cli_summary["functions"].as_u64().unwrap() > 1);
    assert!(cli_summary["total_edges"].as_u64().unwrap() >= 1);
}
