//! Pins added after review of the unmapped-graph refusal (issue #107): marker
//! write failures, batch modes, the one-line refusal, the stamping order and the
//! lock-free `map --cached`. The helpers mirror `tests/unmapped_graph.rs`, which
//! is frozen.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

use common::{git, keel_bin};

const REFUSAL_PREFIX: &str = "keel compile: no usable graph — ";

const WIDGET_BAD: &str = "def widget(x): return x\n";
const GADGET_CLEAN: &str = "def gadget(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n";
const ALPHA_CALLS_BETA: &str = "from beta import beta\n\n\ndef alpha(x: int) -> int:\n    \"\"\"Return beta of x.\"\"\"\n    return beta(x)\n";
const BETA: &str = "def beta(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n";

fn keel_run(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")))
}

fn out_s(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err_s(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out_s(o),
        err_s(o)
    )
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn graph_db(dir: &Path) -> PathBuf {
    dir.join(".keel").join("graph.db")
}

/// One `keel_meta` value, read-only.
fn meta(dir: &Path, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        graph_db(dir),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("failed to open graph.db");
    conn.query_row(
        "SELECT value FROM keel_meta WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// Make SQLite abort a statement: `sql` is the body of `CREATE TRIGGER`s.
fn add_triggers(dir: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(graph_db(dir)).expect("failed to open graph.db");
    conn.execute_batch(sql).expect("failed to add triggers");
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", msg]);
}

/// A git repo with `keel init` run and the files committed — never mapped.
fn init_repo(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, content) in files {
        write(dir.path(), rel, content);
    }
    git(dir.path(), &["init", "-q"]);
    let init = keel_run(dir.path(), &["init"]);
    assert!(
        init.status.success(),
        "keel init failed: {}",
        describe(&init)
    );
    commit_all(dir.path(), "first");
    dir
}

fn mapped_repo(files: &[(&str, &str)]) -> TempDir {
    let dir = init_repo(files);
    let m = keel_run(dir.path(), &["map"]);
    assert!(m.status.success(), "keel map failed: {}", describe(&m));
    dir
}

fn py_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/widget.py", WIDGET_BAD),
        ("src/gadget.py", GADGET_CLEAN),
    ]
}

fn assert_refusal(o: &Output, what: &str) {
    assert_eq!(
        o.status.code(),
        Some(2),
        "{what}: expected exit 2\n{}",
        describe(o)
    );
    assert!(
        o.stdout.is_empty(),
        "{what}: the refusal must leave stdout empty\n{}",
        describe(o)
    );
    assert!(
        err_s(o).starts_with(REFUSAL_PREFIX),
        "{what}: stderr must start with the refusal prefix\n{}",
        describe(o)
    );
}

// ------------------------------------------------- a failed marker write

/// The markers are load-bearing: a map that cannot record one leaves a graph
/// every compile refuses, so it must not report success.
fn assert_marker_write_failure_fails_the_map(key: &str) {
    let dir = init_repo(&py_files());
    add_triggers(
        dir.path(),
        &format!(
            "CREATE TRIGGER probe_insert BEFORE INSERT ON keel_meta WHEN NEW.key = '{key}'
             BEGIN SELECT RAISE(ABORT, 'probe'); END;
             CREATE TRIGGER probe_update BEFORE UPDATE ON keel_meta WHEN NEW.key = '{key}'
             BEGIN SELECT RAISE(ABORT, 'probe'); END;"
        ),
    );
    let out = keel_run(dir.path(), &["map"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a failed `{key}` write must fail the map\n{}",
        describe(&out)
    );
    assert!(
        err_s(&out)
            .lines()
            .any(|l| l.starts_with("keel map: failed to record ")),
        "stderr must carry a `keel map: failed to record …` line\n{}",
        describe(&out)
    );
}

#[test]
fn map_fails_when_the_last_map_at_marker_cannot_be_written() {
    assert_marker_write_failure_fails_the_map("last_map_at");
}

#[test]
fn map_fails_when_the_last_map_commit_marker_cannot_be_written() {
    assert_marker_write_failure_fails_the_map("last_map_commit");
}

// ---------------------------------------------------- the stamping order

/// The markers are stamped after the LAST graph write: a map whose edge write
/// fails must leave neither marker, so the next compile still refuses.
#[test]
fn a_failed_edge_write_leaves_no_marker_and_compile_refuses() {
    let dir = init_repo(&[("src/alpha.py", ALPHA_CALLS_BETA), ("src/beta.py", BETA)]);
    add_triggers(
        dir.path(),
        "CREATE TRIGGER probe_edges BEFORE INSERT ON edges
         BEGIN SELECT RAISE(ABORT, 'probe'); END;",
    );
    let map = keel_run(dir.path(), &["map"]);
    assert_eq!(map.status.code(), Some(2), "{}", describe(&map));
    assert_eq!(
        meta(dir.path(), "last_map_at"),
        None,
        "last_map_at stamped over a graph whose edge write failed"
    );
    assert_eq!(
        meta(dir.path(), "last_map_commit"),
        None,
        "last_map_commit stamped over a graph whose edge write failed"
    );
    let compile = keel_run(dir.path(), &["compile", "src/beta.py"]);
    assert_refusal(&compile, "compile after the failed map");
}

// ------------------------------------------------------------- batch modes

/// Batch handling comes after the never-mapped check: neither start nor end may
/// run, or write batch state, on a graph nobody mapped.
#[test]
fn batch_start_and_end_refuse_on_a_never_mapped_repo_and_write_no_state() {
    let dir = init_repo(&py_files());
    for flag in ["--batch-start", "--batch-end"] {
        let out = keel_run(dir.path(), &["compile", flag]);
        assert_refusal(&out, flag);
        assert_eq!(
            meta(dir.path(), "batch"),
            None,
            "{flag} wrote batch state on a never-mapped repo"
        );
    }
}

// --------------------------------------------------- exactly one stderr line

fn assert_one_refusal_line(dir: &Path, what: &str) {
    let out = keel_run(dir, &["compile", "src/widget.py"]);
    assert_refusal(&out, what);
    assert_eq!(
        err_s(&out).lines().count(),
        1,
        "{what}: the refusal must be exactly one stderr line\n{}",
        describe(&out)
    );
}

#[test]
fn the_refusal_is_exactly_one_stderr_line() {
    let dir = init_repo(&py_files());
    assert_one_refusal_line(dir.path(), "plain never-mapped repo");
}

/// A version-drift warning must not precede or follow the refusal.
#[test]
fn the_refusal_is_one_line_even_when_keel_json_records_an_older_version() {
    let dir = init_repo(&py_files());
    let config = dir.path().join(".keel/keel.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    json["version"] = serde_json::json!("0.0.1");
    fs::write(&config, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    assert_one_refusal_line(dir.path(), "version drift present");
}

// ------------------------------------------------------ map --cached, no lock

/// `map --cached` runs at every session start, possibly under a writer: it must
/// read without taking the graph lock (a lock would wait, then exit 2).
#[test]
fn map_cached_reads_while_another_process_holds_the_graph_lock() {
    let dir = mapped_repo(&py_files());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join(".keel/compile.lock"))
        .unwrap();
    lock.try_lock().expect("test could not take the graph lock");

    let out = keel_run(dir.path(), &["map", "--llm", "--cached"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(out_s(&out).contains("MAP nodes="), "{}", describe(&out));
}
