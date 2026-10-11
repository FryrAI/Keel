//! Acceptance cases for the texts a re-sync reads for a dependent file:
//! groups N (an edited, never compiled dependent) and W (the target of a
//! compile, W009) are red until the re-sync is reworked; group G pins four
//! guards that already work.

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
const GO_WITH_HELPER: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Helper() + Alpha(1)\n}\n\n// Helper.\nfunc Helper() int {\n    return 2\n}\n";
const GO_WITH_SPARE: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n\n// Spare is a compiled working-tree addition.\nfunc Spare() int {\n    return 5\n}\n";
const GO_CALL_MOVED: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return 1\n}\n\n// Fresh holds the call.\nfunc Fresh() int {\n    return Alpha(1)\n}\n";
const GO_THIRD: &str = "package pkg\n\n// Third.\nfunc Third(x int) int {\n    return x + 3\n}\n";
const TS_ALPHA: &str =
    "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const TS_CALLER: &str = "import { alpha } from './alpha';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";
const KEEP: &str = "/** Keep. */\nexport function keep(): number { return 4; }\n";
const BODY: &str = "  let total = 0;\n  for (let index = 0; index < values.length; index += 1) {\n    const current = values[index];\n    if (current > limit) {\n      total += current * 2;\n    } else if (current < 0) {\n      total -= current;\n    } else {\n      total += current;\n    }\n  }\n  return total;\n";
const NO_USABLE: &str = "keel compile: no usable graph — ";

// These helpers mirror tests/graph_resync_fold3.rs (each target carries its own copy).
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

fn dump(root: &Path, sql: &str) -> Vec<String> {
    db(root)
        .prepare(sql)
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
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

fn graph_dump(root: &Path) -> (Vec<String>, Vec<String>) {
    let nodes = dump(root, "SELECT json_array(hash,kind,name,file_path,signature,line_start,line_end) FROM nodes ORDER BY file_path,name,hash");
    let edges = dump(root, "SELECT json_array(s.file_path,s.name,t.file_path,t.name,e.kind,e.file_path,e.line) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id ORDER BY s.file_path,s.name,t.file_path,t.name,e.kind,e.line");
    (nodes, edges)
}

/// Stored `calls` edges from `source` in `file` to any node named `target`.
fn calls(root: &Path, file: &str, source: &str, target: &str) -> i64 {
    count(
        root,
        &format!("SELECT count(*) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id WHERE e.kind='calls' AND s.file_path='{file}' AND s.name='{source}' AND t.name='{target}'"),
    )
}

fn body_rows(root: &Path, file: &str, name: &str) -> i64 {
    count(
        root,
        &format!("SELECT count(*) FROM body_index WHERE file_path='{file}' AND name='{name}'"),
    )
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Alpha lives in `pkg/alpha.go` beside Beta; `pkg/beta.go` holds the caller text given.
fn caller_fixture(caller: &str) -> TempDir {
    fixture(&[
        ("go.mod", GO_MOD),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", caller),
    ])
}

/// Commit ONLY the move of Alpha to `pkg/gadget.go`; the caller edit stays uncommitted.
fn move_alpha(root: &Path) {
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    git(root, &["add", "pkg/alpha.go", "pkg/gadget.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "move alpha"]);
}

/// Alpha (in `file`, whose committed text is `committed`) gains a parameter and
/// is compiled: E001 must name `caller`.
fn gains_parameter_names(root: &Path, file: &str, committed: &str, caller: &str) {
    write(
        root,
        file,
        &committed.replace("value int)", "value int, extra int)"),
    );
    let out = keel(root, &["compile", file]);
    let stdout = stdout_of(&out);
    assert!(
        out.status.code() == Some(1) && stdout.contains("E001") && stdout.contains(caller),
        "E001 must name {caller}: {out:?}"
    );
}

// ---- Group N: a dependent edited in the working tree and never compiled ----

#[test]
fn control_never_compiled_rename_without_a_move_reports_the_caller() {
    // Control: no HEAD move, so no re-sync; the fixture's edge exists.
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    write(root, "pkg/beta.go", &GO_CALLER.replace("Widget", "Renamed"));
    gains_parameter_names(
        root,
        "pkg/alpha.go",
        &format!("{GO_ALPHA}{GO_BETA}"),
        "pkg/beta.go",
    );
}

#[test]
fn never_compiled_rename_of_the_caller_keeps_its_edge() {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    write(root, "pkg/beta.go", &GO_CALLER.replace("Widget", "Renamed"));
    move_alpha(root);
    clean_resync(root);
    gains_parameter_names(root, "pkg/gadget.go", GO_ALPHA, "pkg/beta.go");
}

#[test]
fn never_compiled_move_of_the_call_keeps_the_committed_edge() {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    write(root, "pkg/beta.go", GO_CALL_MOVED);
    move_alpha(root);
    clean_resync(root);
    gains_parameter_names(root, "pkg/gadget.go", GO_ALPHA, "pkg/beta.go");
}

#[test]
fn never_compiled_rename_keeps_the_callers_same_file_edge() {
    let dir = caller_fixture(GO_WITH_HELPER);
    let root = dir.path();
    assert_eq!(
        calls(root, "pkg/beta.go", "Widget", "Helper"),
        1,
        "map stores it"
    );
    write(
        root,
        "pkg/beta.go",
        &GO_WITH_HELPER.replace("Widget", "Renamed"),
    );
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        calls(root, "pkg/beta.go", "Widget", "Helper"),
        1,
        "the same-file edge of the committed function must survive the re-sync"
    );
}

#[test]
fn an_edge_both_texts_yield_is_stored_once() {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    write(root, "pkg/beta.go", GO_WITH_SPARE);
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    assert_eq!(calls(root, "pkg/beta.go", "Widget", "Alpha"), 1);
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        calls(root, "pkg/beta.go", "Widget", "Alpha"),
        1,
        "blob and working tree both yield Widget -> Alpha: one edge, not 0 and not 2"
    );
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='Spare'"),
        1
    );
}

/// Maps Alpha beside Keep, writes the caller as `working` (never compiled),
/// commits only the move to `src/gadget.ts`, re-syncs, then gives Alpha a
/// parameter: E001 must name `src/widget.ts`.
fn typescript_move(working: &str) {
    let dir = fixture(&[
        ("src/alpha.ts", &format!("{TS_ALPHA}{KEEP}")),
        ("src/widget.ts", TS_CALLER),
    ]);
    let root = dir.path();
    write(root, "src/widget.ts", working);
    write(root, "src/alpha.ts", KEEP);
    write(root, "src/gadget.ts", TS_ALPHA);
    git(root, &["add", "src/alpha.ts", "src/gadget.ts"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "move alpha"]);
    clean_resync(root);
    write(
        root,
        "src/gadget.ts",
        &TS_ALPHA.replace("value: number)", "value: number, extra: number)"),
    );
    let out = keel(root, &["compile", "src/gadget.ts"]);
    let stdout = stdout_of(&out);
    assert!(
        out.status.code() == Some(1) && stdout.contains("E001") && stdout.contains("src/widget.ts"),
        "E001 must name src/widget.ts: {out:?}"
    );
}

#[test]
fn control_typescript_move_of_an_unedited_caller_keeps_its_edge() {
    typescript_move(TS_CALLER);
}

/// N5: the N1 rule in TypeScript (the move is committed, the caller's rename is not).
#[test]
fn never_compiled_rename_of_a_typescript_caller_keeps_its_edge() {
    typescript_move(&TS_CALLER.replace("widget", "renamed"));
}

// ---- Group W: the target of the compile (W009) ----

const WS_X: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n";
const WS_X_PULLED: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n\n/// Added by the pull.\npub fn added() -> i32 { 3 }\n";
const WS_Y: &str = "use crate::x::existing;\n\n/// Other.\npub fn other() -> i32 { existing() }\n";
const WS_Z: &str =
    "/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() }\n";
const WS_Z_EDITED: &str = "use a::x::existing;\n/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() + existing() }\n";
/// Crate b already depends on crate a at map time (its only edge into a).
const WS_Z_CALLS_A: &str = "use a::x::existing;\n/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() + existing() }\n";
const WS_Z_COMMENTED: &str = "use a::x::existing;\n// An unrelated comment.\n/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() + existing() }\n";
const WS_Z_CALLS_C: &str = "use a::x::existing;\nuse c::q::third;\n/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() + existing() + third() }\n";
const WS_Q: &str =
    "/// Third.\npub fn third() -> i32 { 7 }\n\n/// Fourth.\npub fn fourth() -> i32 { third() }\n";
const WS_W: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n";

/// A Cargo workspace of crates a, b (and c when `with_c`), initialised, committed and mapped.
fn ws_fixture(z: &str, with_c: bool) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    let crates: &[&str] = if with_c {
        &["a", "b", "c"]
    } else {
        &["a", "b"]
    };
    for name in crates {
        let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
        write(root, &format!("crates/{name}/Cargo.toml"), &manifest);
    }
    write(root, "crates/a/src/x.rs", WS_X);
    write(root, "crates/a/src/y.rs", WS_Y);
    write(root, "crates/b/src/z.rs", z);
    if with_c {
        write(root, "crates/c/src/q.rs", WS_Q);
    }
    git(root, &["init", "-q", "-b", "base"]);
    assert!(keel(root, &["init", "--yes"]).status.success());
    commit(root);
    map(root);
    dir
}

/// The pull: a commit that adds a definition in crate a.
fn pull_adds_definition(root: &Path) {
    write(root, "crates/a/src/x.rs", WS_X_PULLED);
    git(root, &["add", "crates/a/src/x.rs"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "pull"]);
}

/// The pull moves `existing` out of `x.rs` into a new file of crate a.
fn pull_moves_existing(root: &Path) {
    write(
        root,
        "crates/a/src/x.rs",
        "/// Stay.\npub fn stay() -> i32 { 2 }\n",
    );
    write(root, "crates/a/src/w.rs", WS_W);
    git(root, &["add", "crates/a/src/x.rs", "crates/a/src/w.rs"]);
    git(
        root,
        &["commit", "-q", "--no-verify", "-m", "pull moves existing"],
    );
}

const Z: &str = "crates/b/src/z.rs";

fn require_w009(out: &Output, pair: &str, what: &str) {
    let stdout = stdout_of(out);
    assert!(
        stdout.contains("W009") && stdout.contains(pair),
        "{what}: W009 for {pair} expected: {out:?}"
    );
}

#[test]
fn control_first_cross_package_call_without_a_head_move_warns() {
    // W3, control: no HEAD move, the same edit.
    let dir = ws_fixture(WS_Z, false);
    let root = dir.path();
    write(root, Z, WS_Z_EDITED);
    let out = keel(root, &["compile", Z]);
    require_w009(&out, "`b` -> `a`", "control");
    assert_eq!(out.status.code(), Some(0), "W009 is a warning: {out:?}");
}

#[test]
fn first_cross_package_call_after_a_pull_still_warns() {
    let dir = ws_fixture(WS_Z, false);
    let root = dir.path();
    pull_adds_definition(root);
    write(root, Z, WS_Z_EDITED);
    let out = keel(root, &["compile", Z]);
    require_w009(&out, "`b` -> `a`", "pull, then edit");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn first_cross_package_call_edited_before_the_pull_still_warns() {
    let dir = ws_fixture(WS_Z, false);
    let root = dir.path();
    write(root, Z, WS_Z_EDITED);
    pull_adds_definition(root);
    let out = keel(root, &["compile", Z]);
    require_w009(&out, "`b` -> `a`", "edit, then pull");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn an_old_cross_package_dependency_is_not_reported_as_new() {
    let dir = ws_fixture(WS_Z_CALLS_A, false);
    let root = dir.path();
    assert_eq!(calls(root, Z, "run", "existing"), 1, "map stores b -> a");
    pull_moves_existing(root);
    write(root, Z, WS_Z_COMMENTED);
    let out = keel(root, &["compile", Z]);
    assert!(
        !stdout_of(&out).contains("W009"),
        "an old dependency is not new: {out:?}"
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        calls(root, Z, "run", "existing"),
        1,
        "the committed edge into the moved function must exist afterwards"
    );
}

#[test]
fn the_target_is_not_resolved_from_its_working_tree_before_the_check() {
    let dir = ws_fixture(WS_Z_CALLS_A, true);
    let root = dir.path();
    pull_moves_existing(root);
    write(root, Z, WS_Z_CALLS_C);
    let out = keel(root, &["compile", Z]);
    require_w009(&out, "`b` -> `c`", "dependent and target at once");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

// ---- Group G: guards that exist and work ----

/// Pins `graph_resync.rs:267` (`!store.get_nodes_in_file(p).is_empty()`):
/// untracked files the graph holds no node for are not joined, so 16 of them
/// plus a pull that adds a definition stay under the per-edit limit.
#[test]
fn files_without_stored_nodes_are_not_joined_to_a_resync() {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    for i in 1..=16 {
        let text = format!(
            "package pkg\n\n// W{i} calls Alpha.\nfunc W{i}() int {{\n    return Alpha({i})\n}}\n"
        );
        write(root, &format!("pkg/w{i}.go"), &text);
    }
    write(
        root,
        "pkg/alpha.go",
        &format!("{GO_ALPHA}{GO_BETA}\n// Fourth is added by the pull.\nfunc Fourth(x int) int {{\n    return x * 4\n}}\n"),
    );
    git(root, &["add", "pkg/alpha.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "pull"]);
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(0), "no per-edit refusal: {out:?}");
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
    assert_eq!(marker(root), head(root));
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='Fourth'"),
        1
    );
}

/// Pins `graph_resync.rs:201-211`: the dirty files are joined BEFORE the limit
/// check and the record save, so the refusal lands before any write.
#[test]
fn a_refused_resync_writes_nothing() {
    let dir = fixture(&[("go.mod", GO_MOD), ("pkg/alpha.go", GO_ALPHA)]);
    let root = dir.path();
    git(root, &["checkout", "-q", "-b", "older"]);
    write(root, "src/idle.rs", &IDLE.replace("29", "30"));
    commit(root);
    git(root, &["checkout", "-q", "base"]);
    write(root, "pkg/t.go", GO_THIRD);
    commit(root);
    map(root);
    let mut files = Vec::new();
    for i in 1..=16 {
        let rel = format!("pkg/w{i}.go");
        let text = format!(
            "package pkg\n\n// W{i} calls Third.\nfunc W{i}() int {{\n    return Third({i})\n}}\n"
        );
        write(root, &rel, &text);
        files.push(rel);
    }
    let mut args = vec!["compile"];
    args.extend(files.iter().map(String::as_str));
    let compiled = keel(root, &args);
    assert!(
        compiled.status.success(),
        "16 callers compiled: {compiled:?}"
    );
    git(root, &["checkout", "-q", "older"]);
    let bare = keel(root, &["compile"]);
    assert_eq!(
        bare.status.code(),
        Some(0),
        "bare re-sync on older: {bare:?}"
    );
    git(root, &["checkout", "-q", "base"]);
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='Third'"),
        0
    );
    let (marker_before, graph_before) = (marker(root), graph_dump(root));
    let out = keel(root, &["compile", "src/idle.rs"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.code() == Some(2)
            && stderr.starts_with(NO_USABLE)
            && stderr.contains("Run `keel compile`"),
        "explicit compile must refuse with the per-edit limit: {out:?}"
    );
    assert_eq!(marker(root), marker_before, "map marker unchanged");
    assert_eq!(
        graph_dump(root),
        graph_before,
        "nothing was written before the refusal"
    );
}

fn tally(name: &str, factor: u32) -> String {
    let body = BODY.replace("* 2", &format!("* {factor}"));
    format!("/** {name}. */\nexport function {name}(values: number[], limit: number): number {{\n{body}}}\n")
}

/// Pins the `n.file_path = body_index.file_path` clause of
/// `remove_body_rows_without_nodes` (`sqlite_helpers.rs`): a removed twin whose
/// name also exists in another file loses its row; the other file keeps its own.
#[test]
fn a_removed_twin_leaves_the_same_named_row_of_another_file() {
    let dir = fixture(&[
        ("src/one.ts", &tally("alpha", 2)),
        ("src/two.ts", &tally("alpha", 5)),
    ]);
    let root = dir.path();
    assert_eq!(body_rows(root, "src/one.ts", "alpha"), 1);
    assert_eq!(body_rows(root, "src/two.ts", "alpha"), 1);
    write(root, "src/one.ts", KEEP);
    commit(root);
    clean_resync(root);
    assert_eq!(
        body_rows(root, "src/one.ts", "alpha"),
        0,
        "the removed twin's row goes"
    );
    assert_eq!(
        body_rows(root, "src/two.ts", "alpha"),
        1,
        "the other file's row stays"
    );
}

/// Pins the `file_path IN (changed paths)` scope of
/// `remove_body_rows_without_nodes`: a row of an unchanged path survives even
/// when its node is missing.
#[test]
fn a_body_row_of_an_unchanged_path_survives_a_resync() {
    let dir = fixture(&[
        ("src/quiet.ts", &tally("quiet", 3)),
        ("src/changed.ts", &format!("{KEEP}{}", tally("gone", 4))),
    ]);
    let root = dir.path();
    let staged = db(root)
        .execute(
            "DELETE FROM nodes WHERE name='quiet' AND file_path='src/quiet.ts'",
            [],
        )
        .unwrap();
    assert_eq!(staged, 1, "the node row is staged away");
    assert_eq!(body_rows(root, "src/quiet.ts", "quiet"), 1);
    write(root, "src/changed.ts", KEEP);
    commit(root);
    clean_resync(root);
    assert_eq!(
        body_rows(root, "src/changed.ts", "gone"),
        0,
        "the re-sync did clean its own paths"
    );
    assert_eq!(
        body_rows(root, "src/quiet.ts", "quiet"),
        1,
        "an unchanged path keeps its row"
    );
}
