//! The fold of the re-sync review: runs that end before a target is synced
//! (A1), each text resolved with its own imports (A2), a failing git query is
//! an error (A3), and four pins for guards no test covered (P1-P4).

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
const GO_SPARE: &str =
    "\n// Spare is a compiled working-tree addition.\nfunc Spare() int {\n    return Alpha(5)\n}\n";
const GO_WITH_HELPER: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Helper() + Alpha(1)\n}\n\n// Helper.\nfunc Helper() int {\n    return 2\n}\n";
const GO_SPARE_HELPER: &str =
    "\n// Spare calls Helper.\nfunc Spare() int {\n    return Helper()\n}\n";
const GO_THIRD: &str = "package pkg\n\n// Third.\nfunc Third(x int) int {\n    return x + 3\n}\n";
const GO_CALLS_THIRD: &str =
    "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Third(1)\n}\n";
const TS_ONE: &str =
    "/** One. */\nexport function alpha(value: number): number { return value + 1; }\n";
const TS_TWO: &str =
    "/** Two. */\nexport function alpha(value: number): number { return value + 2; }\n";
const TS_USER: &str = "import { alpha } from './one';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";
const TS_USER_EDITED: &str = "import { alpha } from './two';\n/** Renamed. */\nexport function renamed(): number { return alpha(7); }\n";
const TS_EXTRA: &str = "/** Extra. */\nexport function extra(): number { return 9; }\n";

// These helpers mirror tests/graph_resync_fold4.rs (each target carries its own copy).
fn keel_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn keel(root: &Path, args: &[&str]) -> Output {
    keel_in(root, args)
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

fn meta(root: &Path, key: &str) -> String {
    Connection::open(root.join(".keel/graph.db"))
        .unwrap()
        .query_row("SELECT value FROM keel_meta WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .unwrap()
}

fn marker(root: &Path) -> String {
    meta(root, "last_map_commit")
}

fn count(root: &Path, sql: &str) -> i64 {
    Connection::open(root.join(".keel/graph.db"))
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
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

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Stored `calls` edges from `source` in `file` to any node named `target`.
fn calls(root: &Path, file: &str, source: &str, target: &str) -> i64 {
    count(
        root,
        &format!("SELECT count(*) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id WHERE e.kind='calls' AND s.file_path='{file}' AND s.name='{source}' AND t.name='{target}'"),
    )
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

// ---- A1: a run that ends before it syncs its targets has no targets ----

/// `pkg/beta.go` holds the committed Widget plus a COMPILED, uncommitted Spare, both
/// calling Alpha; `before_move` runs after that compile, `first` is the first command
/// after the commit that moves Alpha. Spare's edge must exist right after `first`, and
/// a later signature change must name both callers.
fn compiled_caller_survives(before_move: &[&str], first: &[&str], exit: i32) {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    write(root, "pkg/beta.go", &format!("{GO_CALLER}{GO_SPARE}"));
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    assert_eq!(calls(root, "pkg/beta.go", "Spare", "Alpha"), 1);
    if !before_move.is_empty() {
        assert!(keel(root, before_move).status.success());
    }
    move_alpha(root);
    let out = keel(root, first);
    assert_eq!(out.status.code(), Some(exit), "{first:?}: {out:?}");
    assert_eq!(
        calls(root, "pkg/beta.go", "Spare", "Alpha"),
        1,
        "the compiled working-tree function keeps its edge after {first:?}"
    );
    write(
        root,
        "pkg/gadget.go",
        &GO_ALPHA.replace("value int)", "value int, extra int)"),
    );
    let out = keel(root, &["compile", "pkg/gadget.go"]);
    let stdout = stdout_of(&out);
    assert!(
        out.status.code() == Some(1) && stdout.contains("E001") && stdout.contains("2 caller(s)"),
        "E001 must report both callers: {out:?}"
    );
}

#[test]
fn batch_start_keeps_a_compiled_working_tree_caller() {
    compiled_caller_survives(&[], &["compile", "--batch-start"], 0);
}

/// A batch is started before the move, so `--batch-end` has one to end.
#[test]
fn batch_end_keeps_a_compiled_working_tree_caller() {
    compiled_caller_survives(
        &["compile", "--batch-start"],
        &["compile", "--batch-end"],
        0,
    );
}

#[test]
fn a_missing_sibling_argument_keeps_a_compiled_working_tree_caller() {
    compiled_caller_survives(&[], &["compile", "pkg/beta.go", "pkg/missing.go"], 2);
}

// ---- A2: each text is resolved with its own imports ----

#[test]
fn the_blob_is_resolved_with_its_own_imports() {
    let dir = fixture(&[
        ("src/one.ts", TS_ONE),
        ("src/two.ts", TS_TWO),
        ("src/user.ts", TS_USER),
    ]);
    let root = dir.path();
    let into = |file: &str| {
        count(
            root,
            &format!("SELECT count(*) FROM edges e JOIN nodes s ON s.id=e.source_id JOIN nodes t ON t.id=e.target_id WHERE e.kind='calls' AND s.name='widget' AND t.name='alpha' AND t.file_path='{file}'"),
        )
    };
    assert_eq!(into("src/one.ts"), 1, "map stores widget -> one");
    // Never compiled: the import is re-pointed and widget is renamed.
    write(root, "src/user.ts", TS_USER_EDITED);
    write(root, "src/extra.ts", TS_EXTRA);
    git(root, &["add", "src/extra.ts"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "add extra"]);
    clean_resync(root);
    assert_eq!(
        into("src/two.ts"),
        0,
        "the committed widget never called two's alpha"
    );
    assert_eq!(into("src/one.ts"), 1, "the committed edge stays");
}

// ---- A3: a failing git query is an error, not an empty list ----

const WS_X: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n";
const WS_X_PULLED: &str = "/// Existing.\npub fn existing() -> i32 { 1 }\n\n/// Added by the pull.\npub fn added() -> i32 { 3 }\n";
const WS_Y: &str = "use crate::x::existing;\n\n/// Other.\npub fn other() -> i32 { existing() }\n";
const WS_Z: &str =
    "/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() }\n";
const WS_Z_EDITED: &str = "use a::x::existing;\n/// Helper.\npub fn helper() -> i32 { 1 }\n\n/// Run.\npub fn run() -> i32 { helper() + existing() }\n";
const Z: &str = "crates/b/src/z.rs";

/// Crates a and b, initialised, committed and mapped.
fn ws_fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    for name in ["a", "b"] {
        let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
        write(root, &format!("crates/{name}/Cargo.toml"), &manifest);
    }
    write(root, "crates/a/src/x.rs", WS_X);
    write(root, "crates/a/src/y.rs", WS_Y);
    write(root, Z, WS_Z);
    git(root, &["init", "-q", "-b", "base"]);
    assert!(keel(root, &["init", "--yes"]).status.success());
    commit(root);
    map(root);
    dir
}

/// The W1 scenario: a pull adds a definition in crate a, then b's file makes its
/// first call into a. Returns the fixture, ready to compile.
fn pulled_then_edited() -> TempDir {
    let dir = ws_fixture();
    let root = dir.path();
    write(root, "crates/a/src/x.rs", WS_X_PULLED);
    git(root, &["add", "crates/a/src/x.rs"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "pull"]);
    write(root, Z, WS_Z_EDITED);
    dir
}

fn require_w009(out: &Output, what: &str) {
    assert!(
        stdout_of(out).contains("W009") && stdout_of(out).contains("`b` -> `a`"),
        "{what}: W009 for `b` -> `a` expected: {out:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_git_query_is_an_error_not_an_empty_list() {
    use std::os::unix::fs::PermissionsExt;
    let dir = pulled_then_edited();
    let root = dir.path();
    let real = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap().trim().to_owned();
    assert!(real.starts_with('/'), "git resolves to an absolute path");
    let fake = tempfile::tempdir().unwrap();
    let switch = fake.path().join("fail-name-only");
    let script = format!(
        "#!/bin/sh\nif [ -e '{}' ]; then\n  case \"$*\" in\n    *diff*) case \"$*\" in *--name-only*) exit 128;; esac;;\n  esac\nfi\nexec '{real}' \"$@\"\n",
        switch.display()
    );
    let git_path = fake.path().join("git");
    fs::write(&git_path, script).unwrap();
    fs::set_permissions(&git_path, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    let changed = || {
        Command::new(keel_bin())
            .args(["compile", "--changed"])
            .current_dir(root)
            .env("PATH", &path)
            .output()
            .unwrap()
    };
    let probe = |on: bool| {
        if on {
            fs::write(&switch, "on").unwrap();
        } else if switch.exists() {
            fs::remove_file(&switch).unwrap();
        }
        Command::new(&git_path)
            .args(["-C", ".", "diff", "--name-only", "-z", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .code()
    };
    // Positive control: the fake fails exactly the name-only diff, and only while on.
    assert_eq!(probe(true), Some(128));
    assert_eq!(probe(false), Some(0));
    let mapped = marker(root);
    fs::write(&switch, "on").unwrap();
    let out = changed();
    assert!(
        out.status.code() == Some(2)
            && String::from_utf8_lossy(&out.stderr).contains("git diff failed"),
        "a failing git query must exit 2: {out:?}"
    );
    assert_eq!(marker(root), mapped, "the map marker is unchanged");
    assert_eq!(calls(root, Z, "run", "existing"), 0, "nothing was written");
    fs::remove_file(&switch).unwrap();
    let out = changed();
    require_w009(&out, "git healthy again");
}

// ---- Pins: green today ----

/// Pins the `source != ResolveReferences` clause of `refresh_reference_edges`
/// (`compile_sync.rs`): a re-sync that cannot re-yield a compiled same-file call
/// (the working tree is not UTF-8 and the blob has no Spare) must not delete it.
#[test]
fn a_compiled_same_file_call_survives_a_resync_that_cannot_re_yield_it() {
    let dir = caller_fixture(GO_WITH_HELPER);
    let root = dir.path();
    write(
        root,
        "pkg/beta.go",
        &format!("{GO_WITH_HELPER}{GO_SPARE_HELPER}"),
    );
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    assert_eq!(calls(root, "pkg/beta.go", "Spare", "Helper"), 1);
    fs::write(root.join("pkg/beta.go"), b"package pkg\n\xff\xfe\n").unwrap();
    move_alpha(root);
    clean_resync(root);
    assert_eq!(
        calls(root, "pkg/beta.go", "Spare", "Helper"),
        1,
        "the stored same-file edge must survive"
    );
}

/// Pins the normalisation of explicit targets in `compile.rs` (`make_relative(&root, &cwd.join(f))`).
#[test]
fn explicit_targets_are_normalised_like_graph_paths() {
    for form in ["dot", "absolute", "subdirectory"] {
        let dir = pulled_then_edited();
        let root = dir.path();
        let out = match form {
            "dot" => keel(root, &["compile", "./crates/b/src/z.rs"]),
            "absolute" => keel(root, &["compile", root.join(Z).to_str().unwrap()]),
            _ => keel_in(&root.join("crates/b"), &["compile", "src/z.rs"]),
        };
        require_w009(&out, form);
    }
}

/// Pins the target set of `--changed` and of a bare compile (`git_changed_files` in `compile.rs`).
#[test]
fn changed_and_bare_compiles_have_targets_too() {
    for args in [&["compile", "--changed"][..], &["compile"][..]] {
        let dir = pulled_then_edited();
        let out = keel(dir.path(), args);
        require_w009(&out, &args.join(" "));
    }
}

/// Pins the recovery record of `bring_to_head` (`graph_resync.rs`, `save_pending` after the
/// dirty-file join): a joined dependent is listed before the first graph write.
#[test]
fn the_recovery_record_lists_a_joined_dependent_before_the_first_write() {
    let dir = caller_fixture(GO_CALLER);
    let root = dir.path();
    let mapped = marker(root);
    write(root, "pkg/beta.go", GO_CALLS_THIRD);
    let compiled = keel(root, &["compile", "pkg/beta.go"]);
    assert!(compiled.status.success(), "{compiled:?}");
    write(root, "pkg/t.go", GO_THIRD);
    git(root, &["add", "pkg/t.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "add third"]);
    let db = Connection::open(root.join(".keel/graph.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_edges BEFORE INSERT ON edges BEGIN SELECT RAISE(FAIL, 'synthetic edge failure'); END;").unwrap();
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "the first write must fail: {out:?}"
    );
    assert_eq!(marker(root), mapped);
    let record = meta(root, "resync_pending");
    assert!(record.contains("pkg/beta.go"), "record: {record}");
    db.execute_batch("DROP TRIGGER reject_edges").unwrap();
    clean_resync(root);
    write(
        root,
        "pkg/t.go",
        &GO_THIRD.replace("x int)", "x int, y int)"),
    );
    let out = keel(root, &["compile", "pkg/t.go"]);
    assert!(
        out.status.code() == Some(1)
            && stdout_of(&out).contains("E001")
            && stdout_of(&out).contains("pkg/beta.go"),
        "E001 must name pkg/beta.go: {out:?}"
    );
}
