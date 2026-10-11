//! Fixtures and oracles shared by `tests/graph_per_worktree.rs` and
//! `tests/graph_per_worktree_layouts.rs` (issue #107, item C).
//!
//! Oracles are logical: a dump of nodes (name, kind, file, signature) and
//! reference edges (source name, target name, kind) plus the two map markers,
//! read through a fresh read-only connection - never file bytes - and always
//! checked to be non-empty, so two empty graphs can never compare equal.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;

use crate::common::{git, keel_bin};

pub const ALPHA: &str = "def alpha(value: int) -> int:\n    \"\"\"Alpha.\"\"\"\n    return value\n";
pub const ALPHA_BROKEN: &str =
    "def alpha(value: int, extra: int) -> int:\n    \"\"\"Alpha.\"\"\"\n    return value + extra\n";
pub const BETA: &str =
    "from alpha import alpha\n\n\ndef beta() -> int:\n    \"\"\"Beta.\"\"\"\n    return alpha(1)\n";
pub const GAMMA: &str = "def gamma() -> int:\n    \"\"\"Gamma.\"\"\"\n    return 3\n";
pub const REFUSAL_PREFIX: &str = "keel compile: no usable graph — ";

pub fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .env("KEEL_NO_TELEMETRY", "true")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

pub fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

pub fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

pub fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out(o),
        err(o)
    )
}

pub fn keel_ok(dir: &Path, args: &[&str]) -> Output {
    let o = keel(dir, args);
    assert!(
        o.status.success(),
        "`keel {}` failed in {}\n{}",
        args.join(" "),
        dir.display(),
        describe(&o)
    );
    o
}

pub fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn git_out(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        err(&o)
    );
    out(&o).trim().to_string()
}

pub fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", msg]);
}

/// A committed repository holding `alpha.py`, `beta.py` (calls `alpha(1)`) and
/// `sub/notes.txt`; `keel init` runs first when `init_keel`.
pub fn seed_repo(dir: &Path, init_keel: bool) {
    fs::create_dir_all(dir).unwrap();
    write(dir, "alpha.py", ALPHA);
    write(dir, "beta.py", BETA);
    write(dir, "sub/notes.txt", "notes\n");
    git(dir, &["init", "-q", "-b", "main"]);
    if init_keel {
        keel_ok(dir, &["init"]);
    }
    commit_all(dir, "base");
}

/// A main checkout (initialised, mapped) and a linked worktree on a branch that
/// diverges by one commit (it adds `gamma.py`).
pub struct Pair {
    pub tmp: TempDir,
    pub main: PathBuf,
    pub wt: PathBuf,
}

/// `map_wt`: map the worktree too. `untrack_config`: `.keel/keel.json` is
/// untracked before the worktree exists.
pub fn pair(map_wt: bool, untrack_config: bool) -> Pair {
    let tmp = TempDir::new().unwrap();
    let main = tmp.path().join("main");
    let wt = tmp.path().join("wt");
    seed_repo(&main, true);
    if untrack_config {
        git(&main, &["rm", "--cached", "-q", ".keel/keel.json"]);
        git(
            &main,
            &["commit", "-q", "--no-verify", "-m", "untrack config"],
        );
    }
    keel_ok(&main, &["map"]);
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    write(&wt, "gamma.py", GAMMA);
    commit_all(&wt, "add gamma");
    if map_wt {
        keel_ok(&wt, &["map"]);
    }
    Pair { tmp, main, wt }
}

pub fn pair_mapped() -> Pair {
    pair(true, false)
}

/// `main` mapped, the worktree never mapped.
pub fn pair_new_worktree() -> Pair {
    pair(false, false)
}

fn canon(p: PathBuf) -> PathBuf {
    fs::canonicalize(&p).unwrap_or(p)
}

pub fn abs_git_dir(dir: &Path) -> PathBuf {
    canon(PathBuf::from(git_out(
        dir,
        &["rev-parse", "--absolute-git-dir"],
    )))
}

pub fn abs_common_dir(dir: &Path) -> PathBuf {
    let p = PathBuf::from(git_out(dir, &["rev-parse", "--git-common-dir"]));
    canon(if p.is_absolute() { p } else { dir.join(p) })
}

/// Where the graph database of the checkout at `dir` must live: under its own
/// git directory for a linked worktree, in `.keel` otherwise.
pub fn graph_db(dir: &Path) -> PathBuf {
    let git_dir = abs_git_dir(dir);
    if git_dir != abs_common_dir(dir) {
        git_dir.join("keel").join("graph.db")
    } else {
        dir.join(".keel").join("graph.db")
    }
}

/// File names directly inside `dir`.
pub fn entries(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect()
}

/// A logical dump of one graph database plus its two map markers.
#[derive(Debug, PartialEq)]
pub struct Snap {
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    pub last_map_at: Option<String>,
    pub last_map_commit: Option<String>,
}

pub fn snap(db: &Path) -> Snap {
    assert!(
        db.is_file(),
        "no graph database at {} - a linked worktree's graph must live under its own git directory",
        db.display()
    );
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let marker = |key: &str| -> Option<String> {
        conn.query_row("SELECT value FROM keel_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .ok()
    };
    Snap {
        nodes: rows("SELECT json_array(name,kind,file_path,signature) FROM nodes ORDER BY 1"),
        edges: rows(
            "SELECT json_array(s.name,t.name,e.kind) FROM edges e \
             JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id ORDER BY 1",
        ),
        last_map_at: marker("last_map_at"),
        last_map_commit: marker("last_map_commit"),
    }
}

/// The dump is a real graph: `beta` calls `alpha`, and `gamma` is present
/// exactly when the checkout has `gamma.py`.
pub fn assert_populated(s: &Snap, expect_gamma: bool) {
    let has = |list: &[String], needle: &str| list.iter().any(|l| l == needle);
    assert!(
        has(
            &s.nodes,
            r#"["alpha","function","alpha.py","alpha(value: int) -> int"]"#
        ),
        "{s:?}"
    );
    assert!(has(&s.edges, r#"["beta","alpha","calls"]"#), "{s:?}");
    assert_eq!(
        s.nodes.iter().any(|n| n.starts_with(r#"["gamma","#)),
        expect_gamma,
        "{s:?}"
    );
    assert!(s.last_map_at.is_some(), "{s:?}");
}

/// T1's pin: `alpha` gains a required parameter in `violator` and is compiled
/// there (`E001`); the same compile in `other` is clean and `other`'s logical
/// dump and both markers are unchanged. `cwd`/`alpha_arg` let a caller run the
/// violating compile from a subdirectory.
pub fn assert_violation_isolated(violator: &Path, cwd: &Path, alpha_arg: &str, other: &Path) {
    let before = snap(&graph_db(other));
    assert_populated(&before, other.join("gamma.py").exists());
    assert!(
        graph_db(violator).is_file(),
        "violator has no graph at {}",
        graph_db(violator).display()
    );
    write(violator, "alpha.py", ALPHA_BROKEN);
    let v = keel(cwd, &["compile", alpha_arg]);
    assert_eq!(
        v.status.code(),
        Some(1),
        "violator compile\n{}",
        describe(&v)
    );
    assert!(
        out(&v).contains("E001"),
        "violator compile must report E001\n{}",
        describe(&v)
    );
    let c = keel(other, &["compile", "alpha.py"]);
    assert_eq!(
        c.status.code(),
        Some(0),
        "the same compile in the other checkout\n{}",
        describe(&c)
    );
    assert!(
        out(&c).is_empty(),
        "the other checkout must compile clean\n{}",
        describe(&c)
    );
    assert_eq!(
        snap(&graph_db(other)),
        before,
        "the other checkout's graph changed"
    );
}

// ---- additions for the fold of review round 1 (tests/graph_per_worktree_fold.rs) ----

/// True when the process may ignore file permissions (permission tests skip).
#[cfg(unix)] // unix: uid check via `id -u`
pub fn running_as_root() -> bool {
    out(&Command::new("id").arg("-u").output().unwrap()).trim() == "0"
}

/// Makes a directory read-only, restoring it on drop so the tempdir can go.
#[cfg(unix)] // unix: directory made read-only with chmod
pub struct Locked(PathBuf);

#[cfg(unix)] // unix: directory made read-only with chmod
impl Locked {
    pub fn new(dir: &Path) -> Locked {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
        Locked(dir.to_path_buf())
    }
}

#[cfg(unix)] // unix: restores the chmod on drop
impl Drop for Locked {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

/// The failure is ONE non-empty stderr line naming `graph_dir`, exit 2.
pub fn assert_one_line_naming(o: &Output, graph_dir: &Path) {
    assert_eq!(o.status.code(), Some(2), "{}", describe(o));
    let stderr = err(o);
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected ONE stderr line\n{}", describe(o));
    assert!(
        lines[0].contains(graph_dir.to_str().unwrap()),
        "the line must name {}\n{}",
        graph_dir.display(),
        describe(o)
    );
}

/// A node only the main checkout's graph has: `delta_only` (mapped in main).
pub fn main_only_node(p: &Pair) {
    write(
        &p.main,
        "delta_only.py",
        "def delta_only() -> int:\n    \"\"\"Only in main.\"\"\"\n    return 4\n",
    );
    commit_all(&p.main, "add delta_only");
    keel_ok(&p.main, &["map"]);
}

/// The `hash=` token `keel discover --name <name>` prints in `dir`.
pub fn hash_of(dir: &Path, name: &str) -> String {
    let o = keel_ok(dir, &["discover", "--name", name]);
    out(&o)
        .split_whitespace()
        .find_map(|t| t.strip_prefix("hash="))
        .unwrap_or_else(|| panic!("no hash for {name} in {}\n{}", dir.display(), describe(&o)))
        .to_string()
}

pub fn has_node(s: &Snap, name: &str) -> bool {
    s.nodes
        .iter()
        .any(|n| n.starts_with(&format!("[\"{name}\",")))
}

/// A running `keel` server process; killed on drop so a failed test never leaks it.
pub struct Server {
    pub child: std::process::Child,
    lines: Option<std::sync::mpsc::Receiver<String>>,
}

pub const WAIT: std::time::Duration = std::time::Duration::from_secs(20);

impl Server {
    /// Spawn `keel <args>` with piped stdin and stdout, stderr lines forwarded.
    pub fn start(dir: &Path, args: &[&str]) -> Server {
        use std::io::BufRead;
        use std::process::Stdio;
        let mut child = Command::new(keel_bin())
            .args(args)
            .current_dir(dir)
            .env("KEEL_NO_TELEMETRY", "true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let stderr = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                let _ = tx.send(line);
            }
        });
        Server {
            child,
            lines: Some(rx),
        }
    }

    /// Wait up to `timeout` for a stderr line containing `needle`.
    pub fn saw_stderr(&mut self, needle: &str, timeout: std::time::Duration) -> bool {
        let rx = self.lines.as_ref().unwrap();
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(l) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
                if l.contains(needle) {
                    return true;
                }
            }
        }
        false
    }

    /// Wait (bounded) for a stderr line containing `needle`.
    pub fn wait_for_stderr(&mut self, needle: &str) {
        assert!(
            self.saw_stderr(needle, WAIT),
            "no stderr line containing {needle:?}"
        );
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One MCP session over the real `keel serve --mcp` stdio loop.
pub struct Mcp {
    server: Server,
    replies: std::sync::mpsc::Receiver<String>,
    next_id: u64,
}

impl Mcp {
    pub fn start(dir: &Path) -> Mcp {
        use std::io::BufRead;
        let mut server = Server::start(dir, &["serve", "--mcp", "--no-telemetry"]);
        let stdout = server.child.stdout.take().unwrap();
        let (tx, replies) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                let _ = tx.send(line);
            }
        });
        Mcp {
            server,
            replies,
            next_id: 1,
        }
    }

    /// `tools/call`: (is_error, the tool's text content).
    pub fn call(&mut self, tool: &str, arguments: serde_json::Value) -> (bool, String) {
        use std::io::Write;
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":tool,"arguments":arguments}});
        let stdin = self.server.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let line = self
            .replies
            .recv_timeout(WAIT)
            .unwrap_or_else(|_| panic!("no reply to {tool}"));
        let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let failed =
            reply["result"]["isError"].as_bool().unwrap_or(false) || reply.get("error").is_some();
        (failed, if text.is_empty() { line } else { text })
    }
}

/// One HTTP/1.1 request to the local `keel serve --http` port (bounded by socket timeouts).
pub fn http(method: &str, path: &str, body: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", 4815)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = String::new();
    let _ = s.read_to_string(&mut raw);
    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (
        status,
        raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string(),
    )
}
