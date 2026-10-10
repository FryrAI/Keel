//! Regression cases for `--since` over an unfinished re-sync, working-tree
//! dependents, branch round trips and body-index cleanup.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel_bin};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const IDLE: &str = "/// Widget.\npub fn widget() -> u32 { 29 }\n";
const GO_MOD: &str = "module pkg\n\ngo 1.21\n";
const GO_ALPHA: &str =
    "package pkg\n\n// Alpha.\nfunc Alpha(value int) int {\n    return value + 1\n}\n";
const GO_BETA: &str =
    "package pkg\n\n// Beta.\nfunc Beta(value int) int {\n    return value * 3\n}\n";
const GO_CALLER: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n";
const GO_BETA_CALLER: &str =
    "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Beta(1)\n}\n";
const GO_BOTH: &str =
    "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Beta(1) + Alpha(1)\n}\n";
const GO_LOOSE: &str = "package pkg\n\n// Loose.\nfunc Loose() int {\n    return Alpha(2)\n}\n";
const GO_FRESH: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1) + fresh(2)\n}\n\n// Fresh is new in the working tree.\nfunc fresh(n int) int {\n    return n * 5\n}\n";
const TS_ALPHA: &str =
    "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const TS_CALLER: &str = "import { alpha } from './alpha';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";
const KEEP: &str = "/** Keep. */\nexport function keep(): number { return 4; }\n";
const BODY: &str = "  let total = 0;\n  for (let index = 0; index < values.length; index += 1) {\n    const current = values[index];\n    if (current > limit) {\n      total += current * 2;\n    } else if (current < 0) {\n      total -= current;\n    } else {\n      total += current;\n    }\n  }\n  return total;\n";
const PENDING: &str = "SELECT count(*) FROM keel_meta WHERE key='resync_pending'";

// These helpers mirror tests/graph_resync_fold2.rs.
fn keel(root: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn write(root: &Path, rel: &str, content: &str) {
    let file = root.join(rel);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

fn commit(root: &Path) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "update alpha"]);
}

fn head(root: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn db(root: &Path) -> Connection {
    Connection::open(root.join(".keel/graph.db")).unwrap()
}

fn marker(root: &Path) -> String {
    db(root)
        .query_row(
            "SELECT value FROM keel_meta WHERE key='last_map_commit'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn count(root: &Path, sql: &str) -> i64 {
    db(root).query_row(sql, [], |r| r.get(0)).unwrap()
}

fn map(root: &Path) {
    let out = keel(root, &["map"]);
    assert!(out.status.success(), "fixture map: {out:?}");
}

fn fixture(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "src/idle.rs", IDLE);
    for (rel, content) in files {
        write(root, rel, content);
    }
    git(root, &["init", "-q", "-b", "base"]);
    assert!(keel(root, &["init"]).status.success());
    commit(root);
    map(root);
    dir
}

fn clean_resync(root: &Path) {
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(0), "re-sync must succeed: {out:?}");
    assert!(
        out.stdout.is_empty() && out.stderr.is_empty(),
        "clean re-sync: {out:?}"
    );
    assert_eq!(marker(root), head(root));
}

fn same_output(actual: &Output, oracle: &Output) {
    assert_eq!(
        actual.status.code(),
        oracle.status.code(),
        "{actual:?} vs {oracle:?}"
    );
    assert_eq!(actual.stdout, oracle.stdout);
    assert_eq!(actual.stderr, oracle.stderr);
}

fn dump(root: &Path, sql: &str) -> Vec<String> {
    db(root)
        .prepare(sql)
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn graph_dump(root: &Path) -> (Vec<String>, Vec<String>) {
    // Compare every node and edge by semantic identity; map allocates new ids.
    let nodes = dump(root, "SELECT json_array(hash,kind,name,file_path,signature,line_start,line_end) FROM nodes ORDER BY file_path,name,hash");
    let edges = dump(root, "SELECT json_array(s.file_path,s.name,t.file_path,t.name,e.kind,e.file_path,e.line) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id ORDER BY s.file_path,s.name,t.file_path,t.name,e.kind,e.line");
    (nodes, edges)
}

fn fail_resync(root: &Path, mapped: &str) {
    db(root).execute_batch("CREATE TRIGGER reject_edges BEFORE INSERT ON edges BEGIN SELECT RAISE(FAIL, 'synthetic edge failure'); END;").unwrap();
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(2), "write must fail: {out:?}");
    assert_eq!(marker(root), mapped);
    db(root).execute_batch("DROP TRIGGER reject_edges").unwrap();
}

/// Edit `rel` to `edit`, compile it, and require E001 naming `caller`; a fresh
/// map of the same tree followed by the same compile must print the same.
fn broken_names_caller(root: &Path, rel: &str, committed: &str, edit: &str, caller: &str) {
    write(root, rel, edit);
    let synced = keel(root, &["compile", rel]);
    let stdout = String::from_utf8_lossy(&synced.stdout);
    assert!(
        synced.status.code() == Some(1) && stdout.contains("E001") && stdout.contains(caller),
        "E001 must name {caller}: {synced:?}"
    );
    write(root, rel, committed);
    map(root);
    write(root, rel, edit);
    same_output(&synced, &keel(root, &["compile", rel]));
}

fn go_move_fixture() -> TempDir {
    fixture(&[
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_BETA_CALLER),
    ])
}

fn move_alpha(root: &Path) {
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    git(root, &["add", "pkg/alpha.go", "pkg/gadget.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "move alpha"]);
}

fn alpha_gains_parameter(root: &Path, caller: &str) {
    broken_names_caller(
        root,
        "pkg/gadget.go",
        GO_ALPHA,
        &GO_ALPHA.replace("value int)", "value int, extra int)"),
        caller,
    );
}

#[test]
fn since_refuses_while_a_resync_is_unfinished() {
    let dir = fixture(&[("src/alpha.ts", TS_ALPHA), ("src/widget.ts", TS_CALLER)]);
    let root = dir.path();
    let mapped = marker(root);
    git(root, &["checkout", "-q", "-b", "alpha"]);
    write(
        root,
        "src/alpha.ts",
        &TS_ALPHA.replace("value: number)", "value: number, extra: number)"),
    );
    write(
        root,
        "src/widget.ts",
        &TS_CALLER.replace("alpha(7)", "alpha(7, 9)"),
    );
    commit(root);
    fail_resync(root, &mapped);
    git(root, &["checkout", "-q", "base"]);
    git(root, &["checkout", "-q", "-b", "beta"]);
    write(root, "src/beta.ts", &TS_CALLER.replace("widget", "beta"));
    commit(root);
    let out = keel(root, &["compile", "--since", "base"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "--since must refuse over an unfinished re-sync: {out:?}"
    );
    assert!(out.stdout.is_empty(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "keel compile: no usable graph — the stored graph has an unfinished re-sync. Run \
         `keel compile` to finish it; nothing was checked.\n"
    );
    assert_eq!(marker(root), mapped);
    assert_eq!(count(root, PENDING), 1, "--since must not touch the record");
    assert_eq!(
        keel(root, &["compile", "src/idle.rs"]).status.code(),
        Some(0)
    );
    let since = keel(root, &["compile", "--since", "base"]);
    assert_eq!(since.status.code(), Some(0), "after recovery: {since:?}");
}

#[test]
fn dirty_dependent_keeps_its_working_tree_call_into_a_moved_function() {
    let dir = go_move_fixture();
    let root = dir.path();
    write(root, "pkg/beta.go", GO_BOTH);
    assert!(keel(root, &["compile", "pkg/beta.go"]).status.success());
    move_alpha(root);
    clean_resync(root);
    alpha_gains_parameter(root, "pkg/beta.go");
}

#[test]
fn untracked_dependent_keeps_its_call_into_a_moved_function() {
    let dir = go_move_fixture();
    let root = dir.path();
    write(root, "pkg/loose.go", GO_LOOSE);
    assert!(keel(root, &["compile", "pkg/loose.go"]).status.success());
    move_alpha(root);
    clean_resync(root);
    alpha_gains_parameter(root, "pkg/loose.go");
}

#[test]
fn dirty_dependent_keeps_its_working_tree_only_same_file_edge() {
    let dir = fixture(&[
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_CALLER),
    ]);
    let root = dir.path();
    write(root, "pkg/beta.go", GO_FRESH);
    assert!(keel(root, &["compile", "pkg/beta.go"]).status.success());
    let into_fresh = "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE n.name='fresh' AND e.kind='calls'";
    assert_eq!(count(root, into_fresh), 1);
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        count(root, into_fresh),
        1,
        "the working-tree-only call must keep its edge"
    );
    let synced = keel(root, &["discover", "--name", "fresh", "--llm"]);
    map(root);
    same_output(
        &synced,
        &keel(root, &["discover", "--name", "fresh", "--llm"]),
    );
}

/// Maps at G, compiles `caller` (written to the working tree), round-trips
/// through a branch without Alpha's file, then requires E001 naming `caller`.
fn round_trip_keeps_caller(rel: &str, committed: Option<&str>, working: &str) {
    let alpha = format!("{GO_ALPHA}{GO_BETA}");
    let mut files = vec![("go.mod", GO_MOD), ("pkg/alpha.go", alpha.as_str())];
    files.extend(committed.map(|text| (rel, text)));
    let dir = fixture(&files);
    let root = dir.path();
    git(root, &["checkout", "-q", "-b", "older"]);
    git(root, &["rm", "-q", "pkg/alpha.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "drop alpha"]);
    git(root, &["checkout", "-q", "base"]);
    write(root, rel, working);
    assert!(keel(root, &["compile", rel]).status.success());
    git(root, &["checkout", "-q", "older"]);
    clean_resync(root);
    git(root, &["checkout", "-q", "base"]);
    clean_resync(root);
    let into_alpha = "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE n.name='Alpha' AND e.kind='calls'";
    assert_eq!(count(root, into_alpha), 1, "the call must be re-resolved");
    broken_names_caller(
        root,
        "pkg/alpha.go",
        &format!("{GO_ALPHA}{GO_BETA}"),
        &format!("{GO_ALPHA}{GO_BETA}").replace("Alpha(value int)", "Alpha(value int, extra int)"),
        rel,
    );
}

#[test]
fn round_trip_restores_an_untracked_callers_edge() {
    round_trip_keeps_caller("pkg/loose.go", None, GO_LOOSE);
}

#[test]
fn round_trip_restores_a_modified_tracked_callers_edge() {
    round_trip_keeps_caller(
        "pkg/other.go",
        Some(&GO_BETA_CALLER.replace("Widget", "Other")),
        &GO_BOTH.replace("Widget", "Other"),
    );
}

#[test]
fn definition_only_resync_lists_no_extra_dependent() {
    let dir = go_move_fixture();
    let root = dir.path();
    write(root, "pkg/beta.go", GO_BOTH);
    assert!(keel(root, &["compile", "pkg/beta.go"]).status.success());
    write(root, "pkg/loose.go", GO_LOOSE);
    assert!(keel(root, &["compile", "pkg/loose.go"]).status.success());
    write(
        root,
        "pkg/alpha.go",
        &format!("{GO_ALPHA}{GO_BETA}").replace("// Beta.", "// Beta, again."),
    );
    git(root, &["add", "pkg/alpha.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "doc edit"]);
    let out = keel(root, &["--verbose", "compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("re-synced 1 file(s)"), "{stderr}");
}

fn tally(name: &str) -> String {
    format!("/** {name}. */\nexport function {name}(values: number[], limit: number): number {{\n{BODY}}}\n")
}

#[test]
fn twin_removed_after_a_hash_change_leaves_no_body_row() {
    let dir = fixture(&[("src/dup.ts", &format!("{KEEP}{}", tally("alpha")))]);
    let root = dir.path();
    write(
        root,
        "src/dup.ts",
        &format!(
            "{KEEP}{}",
            tally("alpha").replace("/** alpha. */", "/** Alpha sums. */")
        ),
    );
    commit(root);
    clean_resync(root);
    write(root, "src/dup.ts", KEEP);
    commit(root);
    clean_resync(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM body_index WHERE name='alpha'"),
        0,
        "the removed twin's mapped row must go"
    );
    write(root, "src/omega.ts", &tally("omega"));
    let out = keel(root, &["compile", "src/omega.ts"]);
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("W006"),
        "a removed function must not be a W006 twin: {out:?}"
    );
}

const CLASSES: &str = "/** Run. */\nexport class First {\n  /** Run. */\n  run(values: number[], limit: number): number {\nBODY  }\n}\n/** Run. */\nexport class Second {\n  /** Run. */\n  run(values: number[], limit: number): number {\nBODY  }\n}\n";

#[test]
fn resync_without_a_removal_leaves_body_rows_identical() {
    let classes = CLASSES.replace("BODY", BODY);
    let source = format!("{}{classes}", tally("alpha"));
    let dir = fixture(&[("src/dup.ts", &source)]);
    let root = dir.path();
    let rows = "SELECT json_array(node_hash,body_hash,t2_hash,name,file_path,line) FROM body_index ORDER BY node_hash";
    let before = dump(root, rows);
    assert_eq!(
        before.len(),
        3,
        "function and two same-named methods: {before:?}"
    );
    write(
        root,
        "src/dup.ts",
        &source
            .replace("/** alpha. */", "/** Alpha sums. */")
            .replace(
                "/** Run. */\nexport class First",
                "/** First runs. */\nexport class First",
            ),
    );
    commit(root);
    clean_resync(root);
    assert_eq!(dump(root, rows), before, "no definition was removed");
    write(root, "src/beta.ts", &tally("beta"));
    let out = keel(root, &["compile", "src/beta.ts"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("W006") && stdout.contains("src/dup.ts"),
        "a duplicate of a surviving body still warns: {out:?}"
    );
}

#[test]
fn deletion_only_resync_that_fails_part_way_is_restored_at_the_baseline() {
    let files = [
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", GO_ALPHA),
        ("pkg/beta.go", GO_BETA),
        ("pkg/widget.go", GO_CALLER),
        (
            "pkg/gadget.go",
            "package pkg\n\n// Gadget.\nfunc Gadget() int {\n    return Beta(1)\n}\n",
        ),
    ];
    let dir = fixture(&files);
    let root = dir.path();
    let mapped = marker(root);
    git(root, &["checkout", "-q", "-b", "feature"]);
    git(root, &["rm", "-q", "pkg/alpha.go", "pkg/beta.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "delete both"]);
    db(root).execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON nodes WHEN OLD.file_path='pkg/beta.go' BEGIN SELECT RAISE(FAIL, 'synthetic delete failure'); END;").unwrap();
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "second prune must fail: {out:?}"
    );
    assert_eq!(marker(root), mapped);
    db(root)
        .execute_batch("DROP TRIGGER reject_delete")
        .unwrap();
    git(root, &["checkout", "-q", "base"]);
    clean_resync(root);
    let oracle = fixture(&files);
    assert_eq!(
        graph_dump(root),
        graph_dump(oracle.path()),
        "returning to G must restore G's graph"
    );
}

#[test]
fn record_keyed_to_another_commit_is_ignored() {
    let dir = fixture(&[
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", GO_ALPHA),
        ("pkg/beta.go", GO_BETA),
    ]);
    let root = dir.path();
    let older = head(root);
    write(root, "src/idle.rs", &IDLE.replace("29", "31"));
    commit(root);
    map(root);
    write(root, "pkg/alpha.go", &format!("{GO_ALPHA}\n// Fresh is new in the working tree.\nfunc Fresh(n int) int {{\n    return n\n}}\n"));
    write(
        root,
        "pkg/loose.go",
        &GO_LOOSE.replace("Alpha(2)", "Fresh(2)"),
    );
    assert!(keel(root, &["compile", "pkg/alpha.go"]).status.success());
    assert!(keel(root, &["compile", "pkg/loose.go"]).status.success());
    let record = serde_json::to_string(&(&older, ["pkg/alpha.go"], [] as [&str; 0])).unwrap();
    db(root)
        .execute(
            "INSERT OR REPLACE INTO keel_meta(key,value) VALUES('resync_pending',?1)",
            [&record],
        )
        .unwrap();
    assert_eq!(
        keel(root, &["compile", "src/idle.rs"]).status.code(),
        Some(0)
    );
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='Fresh'"),
        1,
        "a record keyed to another commit must not prune the compiled function"
    );
    let edit = format!("{GO_ALPHA}\n// Fresh is new in the working tree.\nfunc Fresh(n int, m int) int {{\n    return n\n}}\n");
    write(root, "pkg/alpha.go", &edit);
    let out = keel(root, &["compile", "pkg/alpha.go"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.code() == Some(1) && stdout.contains("E001") && stdout.contains("pkg/loose.go"),
        "{out:?}"
    );
}
