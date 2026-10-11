//! A graph nobody mapped must be loud, never a silent pass (issue #107, design
//! §2 and §5). Every test drives the real `keel` binary.
//!
//! "Never mapped" means `keel_meta` has no `last_map_at` row, or a HEAD commit
//! exists and there is no `last_map_commit` row. The refusal is exit 2, empty
//! stdout, and a first stderr line starting `keel compile: no usable graph — `.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

use common::{git, keel_bin};

const REFUSAL_PREFIX: &str = "keel compile: no usable graph — ";

const WIDGET_BAD: &str = "def widget(x): return x\n";
const GADGET_CLEAN: &str = "def gadget(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n";

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

/// The state `keel map` leaves when it is killed after clearing its markers.
fn delete_markers(dir: &Path) {
    let conn = rusqlite::Connection::open(graph_db(dir)).expect("failed to open graph.db");
    conn.execute(
        "DELETE FROM keel_meta WHERE key IN ('last_map_at', 'last_map_commit')",
        [],
    )
    .unwrap();
}

fn head(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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

/// `init_repo` plus a `keel map` at HEAD.
fn mapped_repo(files: &[(&str, &str)]) -> TempDir {
    let dir = init_repo(files);
    map(dir.path());
    dir
}

fn map(dir: &Path) {
    let m = keel_run(dir, &["map"]);
    assert!(m.status.success(), "keel map failed: {}", describe(&m));
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
    let stderr = err_s(o);
    let first = stderr.lines().next().unwrap_or("");
    assert!(
        first.starts_with(REFUSAL_PREFIX),
        "{what}: first stderr line must start with the refusal prefix\n{}",
        describe(o)
    );
    assert!(
        stderr.contains("keel map"),
        "{what}: the refusal must name `keel map`\n{}",
        describe(o)
    );
}

/// After a map the committed, unedited violating file is pre-existing debt: keel
/// reports its E002 as a warning (exit 0), not an error (exit 1) - the brief's
/// "exit 1" does not hold on this tree. What must hold: not refused, finding shown.
fn assert_compiles_with_e002(o: &Output, what: &str) {
    assert!(
        matches!(o.status.code(), Some(0) | Some(1)),
        "{what}: compile must run, not refuse\n{}",
        describe(o)
    );
    assert!(out_s(o).contains("E002"), "{what}\n{}", describe(o));
}

fn assert_cached_refusal(o: &Output, what: &str) {
    assert_eq!(
        o.status.code(),
        Some(2),
        "{what}: expected exit 2\n{}",
        describe(o)
    );
    assert!(
        o.stdout.is_empty(),
        "{what}: must not print a stored or fresh map\n{}",
        describe(o)
    );
    let stderr = err_s(o);
    assert!(
        stderr.contains("no usable graph") && stderr.contains("keel map"),
        "{what}: stderr must say `no usable graph` and name `keel map`\n{}",
        describe(o)
    );
}

// ---------------------------------------------------------------- compile

/// A fresh clone has `.keel/keel.json` but no graph. Without the slice the first
/// compile exits 1 on file-local findings only, writes that file into a new
/// graph, and every later compile trusts a graph that is one file big.
#[test]
fn fresh_clone_compile_refuses_and_seeds_nothing() {
    let origin = mapped_repo(&py_files());
    let clones = TempDir::new().unwrap();
    git(
        clones.path(),
        &["clone", "-q", origin.path().to_str().unwrap(), "clone"],
    );
    let clone = clones.path().join("clone");
    assert!(
        clone.join(".keel/keel.json").exists(),
        "fixture: clone lacks keel.json"
    );

    let refused = keel_run(&clone, &["compile", "src/widget.py"]);
    assert_refusal(&refused, "fresh clone, explicit file");

    let search = keel_run(&clone, &["search", "widget"]);
    assert!(
        out_s(&search).contains("(0 found)") && !out_s(&search).contains("widget hash="),
        "the refused compile seeded the graph with the file's nodes\n{}",
        describe(&search)
    );

    map(&clone);
    let after = keel_run(&clone, &["compile", "--llm", "src/widget.py"]);
    assert_compiles_with_e002(&after, "after map");
}

/// Every way to run compile must refuse on a never-mapped repo, `--since`
/// included; any mode that passes silently is a hook that reports nothing.
#[test]
fn fresh_init_every_compile_mode_refuses() {
    let dir = init_repo(&py_files());
    write(
        dir.path(),
        "src/widget.py",
        "def widget(x): return x  # edited\n",
    );
    let calls: [(&str, Vec<&str>); 4] = [
        ("explicit file", vec!["compile", "src/widget.py"]),
        ("bare compile", vec!["compile"]),
        ("--changed", vec!["compile", "--changed"]),
        ("--since HEAD", vec!["compile", "--since", "HEAD"]),
    ];
    for (what, args) in calls {
        assert_refusal(&keel_run(dir.path(), &args), what);
    }
}

/// A `keel map` killed after clearing its markers leaves a small graph; compile
/// used to check against it as if it were the repository.
#[test]
fn compile_refuses_after_a_map_that_died() {
    let dir = mapped_repo(&py_files());
    delete_markers(dir.path());
    assert_refusal(
        &keel_run(dir.path(), &["compile", "src/widget.py"]),
        "markers deleted",
    );
}

/// Mapping before the first commit leaves no `last_map_commit`; once a HEAD
/// exists, that graph is not known to match it.
#[test]
fn compile_refuses_when_mapped_before_the_first_commit() {
    let dir = TempDir::new().unwrap();
    for (rel, content) in py_files() {
        write(dir.path(), rel, content);
    }
    git(dir.path(), &["init", "-q"]);
    assert!(keel_run(dir.path(), &["init"]).status.success());
    map(dir.path());
    commit_all(dir.path(), "first");

    assert_eq!(
        meta(dir.path(), "last_map_commit"),
        None,
        "fixture: no commit existed at map time"
    );
    assert_refusal(
        &keel_run(dir.path(), &["compile", "src/widget.py"]),
        "mapped before the first commit",
    );
}

/// Outside git there is no commit to compare, but a never-mapped graph is still
/// never-mapped: refuse first, work normally once mapped.
#[test]
fn outside_git_never_mapped_refuses_then_works_after_map() {
    let dir = TempDir::new().unwrap();
    for (rel, content) in py_files() {
        write(dir.path(), rel, content);
    }
    assert!(keel_run(dir.path(), &["init"]).status.success());
    assert!(
        !dir.path().join(".git").exists(),
        "fixture must be outside git"
    );

    assert_refusal(
        &keel_run(dir.path(), &["compile", "src/widget.py"]),
        "outside git, never mapped",
    );

    map(dir.path());
    let bad = keel_run(dir.path(), &["compile", "--llm", "src/widget.py"]);
    assert_compiles_with_e002(&bad, "violating file");
    let clean = keel_run(dir.path(), &["compile", "src/gadget.py"]);
    assert_eq!(
        clean.status.code(),
        Some(0),
        "clean file\n{}",
        describe(&clean)
    );
    assert!(clean.stdout.is_empty(), "clean file\n{}", describe(&clean));
}

/// Control: a graph mapped at HEAD compiles a clean file silently, as before.
#[test]
fn control_mapped_at_head_clean_file_is_silent() {
    let dir = mapped_repo(&py_files());
    let out = keel_run(dir.path(), &["compile", "src/gadget.py"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(out.stdout.is_empty(), "{}", describe(&out));
    assert!(out.stderr.is_empty(), "{}", describe(&out));
}

// ------------------------------------------------------------- map --cached

/// `map --cached` on a never-mapped repo used to run a full map behind the
/// caller's back and exit 0; a "cached" read must never build anything.
#[test]
fn map_cached_on_a_never_mapped_repo_refuses_without_mapping() {
    let dir = init_repo(&py_files());
    let out = keel_run(dir.path(), &["map", "--llm", "--cached"]);
    assert_cached_refusal(&out, "fresh init");
    assert_eq!(
        meta(dir.path(), "last_map_at"),
        None,
        "a full map ran behind --cached"
    );
}

/// Nodes with no markers is a map that died; printing it as "the map" would
/// hand the agent a partial graph.
#[test]
fn map_cached_with_nodes_but_no_markers_refuses() {
    let dir = mapped_repo(&py_files());
    delete_markers(dir.path());
    let out = keel_run(dir.path(), &["map", "--llm", "--cached"]);
    assert_cached_refusal(&out, "markers deleted");
}

/// A stored map from another commit is still printed, but the reader must be
/// told which commits differ and how to fix it.
#[test]
fn map_cached_names_both_commits_when_the_map_is_not_heads() {
    let dir = mapped_repo(&py_files());
    let mapped_at = head(dir.path());
    write(dir.path(), "src/extra.py", GADGET_CLEAN);
    commit_all(dir.path(), "second");
    let now = head(dir.path());
    assert_ne!(mapped_at, now);

    let out = keel_run(dir.path(), &["map", "--llm", "--cached"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(out_s(&out).contains("MAP nodes="), "{}", describe(&out));
    let stderr = err_s(&out);
    assert_eq!(
        stderr.trim_end().lines().count(),
        1,
        "stderr must be exactly one line\n{}",
        describe(&out)
    );
    assert!(
        stderr.contains(&mapped_at[..12])
            && stderr.contains(&now[..12])
            && stderr.contains("keel map"),
        "the line must name both 12-char commits and `keel map`\n{}",
        describe(&out)
    );
}

/// Control: a map at HEAD prints exactly the stored map and nothing on stderr.
#[test]
fn control_map_cached_at_head_is_unchanged() {
    let dir = mapped_repo(&py_files());
    let out = keel_run(dir.path(), &["map", "--llm", "--cached"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(out_s(&out).contains("MAP nodes="), "{}", describe(&out));
    assert!(out.stderr.is_empty(), "{}", describe(&out));
}

// ------------------------------------------------------------- the hook

fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A committed, never-mapped repo with the real `post-edit.sh` installed.
fn hook_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, content) in py_files() {
        write(dir.path(), rel, content);
    }
    git(dir.path(), &["init", "-q"]);
    assert!(keel_run(dir.path(), &["init"]).status.success());
    write(dir.path(), ".keel/hooks/post-edit.sh", "");
    let upd = keel_run(dir.path(), &["init", "--update-docs"]);
    assert!(upd.status.success(), "{}", describe(&upd));
    commit_all(dir.path(), "first");
    dir
}

/// Run the real hook, with the built `keel` first on PATH, for `rel`.
fn run_real_hook(root: &Path, rel: &str) -> Output {
    let file = root.canonicalize().unwrap().join(rel);
    let payload = serde_json::json!({ "tool_input": { "file_path": file.to_string_lossy() } });
    let path = format!(
        "{}:{}",
        keel_bin().parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = Command::new("bash")
        .args([".keel/hooks/post-edit.sh", "claude-code"])
        .current_dir(root)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The agent only sees a hook's stderr on exit 2. A never-mapped repo used to
/// pass the hook silently; it must now reach the agent as a blocking message.
#[test]
fn hook_on_a_never_mapped_repo_exits_2_with_the_prefix() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = hook_repo();
    let out = run_real_hook(dir.path(), "src/gadget.py");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert!(
        err_s(&out).starts_with(REFUSAL_PREFIX),
        "stderr must start with the refusal prefix\n{}",
        describe(&out)
    );
}

/// Control: once mapped, the same clean-file edit passes the hook silently.
#[test]
fn control_hook_on_a_mapped_repo_is_silent() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = hook_repo();
    map(dir.path());
    let out = run_real_hook(dir.path(), "src/gadget.py");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(out.stderr.is_empty(), "{}", describe(&out));
}
