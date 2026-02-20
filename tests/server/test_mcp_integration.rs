//! End-to-end integration test for `keel serve --mcp` over stdin/stdout.
//!
//! Spawns the keel binary as a child process, sends JSON-RPC messages
//! via stdin, and reads responses from stdout with timeouts.
//!
//! Each test uses its own temp directory to avoid SQLite races when
//! multiple test processes try to create `.keel/graph.db` concurrently.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Locate the `keel` binary in the same target directory as the test binary.
fn keel_binary() -> std::path::PathBuf {
    let test_exe = std::env::current_exe().expect("cannot find test executable");
    let target_dir = test_exe
        .parent() // deps/
        .and_then(|p| p.parent()) // debug/ or release/
        .expect("cannot find target dir");
    let bin = target_dir.join("keel");
    assert!(
        bin.exists(),
        "keel binary not found at {}. Run `cargo build` first.",
        bin.display()
    );
    bin
}

/// Create a temp directory with `.keel/` inside it so the keel binary
/// can open/create its own `graph.db` without racing other test processes.
fn make_test_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::create_dir_all(dir.path().join(".keel")).expect("failed to create .keel dir");
    dir
}

/// Send a JSON-RPC request line and read one response line with timeout.
fn send_and_receive(
    stdin: &mut impl Write,
    stdout: &mut BufReader<impl std::io::Read>,
    request: &serde_json::Value,
) -> serde_json::Value {
    let mut line = serde_json::to_string(request).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).unwrap();
    stdin.flush().unwrap();

    let mut response = String::new();
    // BufReader will block until a line is available; the test runner
    // timeout (set below) prevents infinite hangs.
    stdout.read_line(&mut response).unwrap();
    serde_json::from_str(response.trim()).expect("response should be valid JSON")
}

#[test]
fn test_mcp_stdin_stdout_initialize() {
    let test_dir = make_test_dir();
    let mut child = Command::new(keel_binary())
        .args(["serve", "--mcp"])
        .current_dir(test_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn keel serve --mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // 1. Send initialize request
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        },
        "id": 1
    });
    let resp = send_and_receive(&mut stdin, &mut stdout, &init_req);
    assert!(
        resp.get("result").is_some(),
        "initialize should return a result: {resp}"
    );
    let result = &resp["result"];
    assert!(
        result.get("capabilities").is_some(),
        "result should contain capabilities"
    );

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(2));
    let _ = child.kill();
}

#[test]
fn test_mcp_stdin_stdout_tools_list() {
    let test_dir = make_test_dir();
    let mut child = Command::new(keel_binary())
        .args(["serve", "--mcp"])
        .current_dir(test_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn keel serve --mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize first (MCP requires initialization)
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        },
        "id": 1
    });
    let _ = send_and_receive(&mut stdin, &mut stdout, &init_req);

    // 2. Send tools/list
    let list_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 2
    });
    let resp = send_and_receive(&mut stdin, &mut stdout, &list_req);
    assert!(
        resp.get("result").is_some(),
        "tools/list should return a result: {resp}"
    );

    // result is either {tools: [...]} or a direct array
    let result = &resp["result"];
    let tools = if let Some(arr) = result.as_array() {
        arr.clone()
    } else if let Some(arr) = result.get("tools").and_then(|t| t.as_array()) {
        arr.clone()
    } else {
        panic!("result should contain tools: {result}");
    };
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(!tool_names.is_empty(), "tools list should not be empty");

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(2));
    let _ = child.kill();
}

#[test]
fn test_mcp_stdin_stdout_invalid_json() {
    let test_dir = make_test_dir();
    let mut child = Command::new(keel_binary())
        .args(["serve", "--mcp"])
        .current_dir(test_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn keel serve --mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Initialize first to confirm the server is running
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        },
        "id": 1
    });
    let _ = send_and_receive(&mut stdin, &mut stdout, &init_req);

    // 3. Send invalid JSON
    stdin.write_all(b"this is not json\n").unwrap();
    stdin.flush().unwrap();

    let mut response = String::new();
    stdout.read_line(&mut response).unwrap();
    let resp: serde_json::Value =
        serde_json::from_str(response.trim()).expect("error response should be valid JSON");

    assert!(
        resp.get("error").is_some(),
        "invalid JSON should return an error response: {resp}"
    );

    // Clean up
    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(2));
    let _ = child.kill();
}

/// Helper trait for wait_timeout on Child.
trait ChildExt {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None if start.elapsed() >= timeout => return Ok(None),
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}
