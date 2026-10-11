//! Regression cases for committed graph re-sync recovery and identity.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel_bin};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const IDLE: &str = "/// Widget.\npub fn widget() -> u32 { 29 }\n";
const ALPHA: &str = "/// Alpha.\npub struct Alpha;\nimpl Alpha {\n    /// Same.\n    pub fn same(value: i32) -> i32 { value }\n}\n";
const BETA: &str = "/// Beta.\npub struct Beta;\nimpl Beta {\n    /// Same.\n    pub fn same(value: i32, extra: i32) -> i32 { value + extra }\n}\n";
const GO_ALPHA: &str =
    "package pkg\n\n// Alpha.\nfunc Alpha(value int) int {\n    return value + 1\n}\n";
const GO_BETA: &str =
    "package pkg\n\n// Beta.\nfunc Beta(value int) int {\n    return value * 3\n}\n";
const GO_CALLER: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n";
const BODY: &str = "  let total = 0;\n  for (let index = 0; index < values.length; index += 1) {\n    const current = values[index];\n    if (current > limit) {\n      total += current * 2;\n    } else if (current < 0) {\n      total -= current;\n    } else {\n      total += current;\n    }\n  }\n  return total;\n";

// These helpers mirror tests/graph_resync.rs.
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
    let out = keel(root, &["init"]);
    assert!(out.status.success(), "fixture init: {out:?}");
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

fn broken_matches_map(root: &Path, rel: &str, committed: &str, edit: &str) {
    write(root, rel, edit);
    let synced = keel(root, &["compile", rel]);
    write(root, rel, committed);
    map(root);
    write(root, rel, edit);
    let fresh = keel(root, &["compile", rel]);
    assert_eq!(
        fresh.status.code(),
        Some(1),
        "fresh map must detect the break: {fresh:?}"
    );
    assert!(
        String::from_utf8_lossy(&fresh.stdout).contains("E001"),
        "fresh map: {fresh:?}"
    );
    assert_eq!(
        synced.status.code(),
        Some(1),
        "re-sync must detect the same broken caller: {synced:?}"
    );
    assert!(
        String::from_utf8_lossy(&synced.stdout).contains("E001"),
        "re-sync: {synced:?}"
    );
}

#[test]
fn committed_same_name_removal_preserves_survivor_and_caller() {
    let dir = fixture(&[
        (
            "Cargo.toml",
            "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub mod shapes;\npub mod user;\n"),
        ("src/shapes.rs", &format!("{ALPHA}{BETA}")),
        (
            "src/user.rs",
            "use crate::shapes::Beta;\n/// Widget.\npub fn widget() -> i32 { Beta::same(1, 2) }\n",
        ),
    ]);
    let root = dir.path();
    let beta_id: i64 = db(root)
        .query_row(
            "SELECT id FROM nodes WHERE name='same' AND signature LIKE '%extra%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    write(root, "src/shapes.rs", BETA);
    commit(root);
    clean_resync(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='same'"),
        1,
        "committed removal must leave exactly one same node"
    );
    let after_id: i64 = db(root)
        .query_row("SELECT id FROM nodes WHERE name='same'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_id, beta_id, "hash evidence must preserve Beta's id");
    assert_eq!(count(root, &format!("SELECT count(*) FROM edges WHERE target_id={beta_id} AND kind='calls' AND file_path='src/user.rs'")), 1);
    broken_matches_map(
        root,
        "src/shapes.rs",
        BETA,
        &BETA.replace("extra: i32)", "extra: i32, value2: i32)"),
    );
}

#[test]
fn committed_same_name_addition_inserts_both_definitions() {
    let dir = fixture(&[("src/lib.rs", ALPHA)]);
    let root = dir.path();
    write(root, "src/lib.rs", &format!("{ALPHA}{BETA}"));
    commit(root);
    clean_resync(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='same'"),
        2,
        "committed addition must insert the unbound same definition"
    );
}

fn go_fixture(callers: usize) -> TempDir {
    let dir = fixture(&[
        ("go.mod", "module example.invalid/pkg\n\ngo 1.21\n"),
        (
            "pkg/alpha.go",
            &format!(
                "{GO_ALPHA}\n{}",
                GO_BETA.trim_start_matches("package pkg\n")
            ),
        ),
    ]);
    let root = dir.path();
    for idx in 0..callers {
        write(
            root,
            &format!("pkg/widget{idx}.go"),
            &GO_CALLER.replace("Widget", &format!("Widget{idx}")),
        );
    }
    commit(root);
    map(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM edges WHERE kind='calls'"),
        callers as i64,
        "mapped fixture must have every caller"
    );
    dir
}

#[test]
fn moved_definition_restores_unchanged_callers() {
    let dir = go_fixture(1);
    let root = dir.path();
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    commit(root);
    clean_resync(root);
    assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='calls' AND n.name='Alpha' AND n.file_path='pkg/gadget.go'"), 1, "moved definition must regain its unchanged caller");
    broken_matches_map(
        root,
        "pkg/gadget.go",
        GO_ALPHA,
        &GO_ALPHA.replace("value int)", "value int, extra int)"),
    );
}

#[test]
fn renamed_file_restores_unchanged_callers() {
    let dir = go_fixture(1);
    let root = dir.path();
    git(root, &["mv", "pkg/alpha.go", "pkg/gadget.go"]);
    commit(root);
    clean_resync(root);
    assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='calls' AND n.name='Alpha' AND n.file_path='pkg/gadget.go'"), 1, "renamed definition must regain its unchanged caller");
    let source = fs::read_to_string(root.join("pkg/gadget.go")).unwrap();
    broken_matches_map(
        root,
        "pkg/gadget.go",
        &source,
        &source.replace("func Alpha(value int)", "func Alpha(value int, extra int)"),
    );
}

#[test]
fn dependent_limit_refuses_then_bare_compile_completes() {
    let dir = go_fixture(29);
    let root = dir.path();
    let mapped = marker(root);
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    commit(root);
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "2 changed + 29 dependent files must exceed the inline limit: {out:?}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("31 source files") && stderr.contains("Run `keel compile`"),
        "{stderr}"
    );
    assert!(out.stdout.is_empty());
    assert_eq!(marker(root), mapped);
    let out = keel(root, &["compile"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "bare compile must finish partial writes: {out:?}"
    );
    assert_eq!(marker(root), head(root));
    assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='calls' AND n.name='Alpha' AND n.file_path='pkg/gadget.go'"), 29);
}

#[test]
fn failed_edge_write_keeps_marker_and_retry_repairs_contains() {
    for moved in [false, true] {
        let dir = if moved { go_fixture(1) } else { fixture(&[]) };
        let root = dir.path();
        let mapped = marker(root);
        write(
            root,
            "src/alpha.rs",
            "/// Alpha.\npub fn alpha() -> u32 { 43 }\n",
        );
        if moved {
            write(root, "pkg/alpha.go", GO_BETA);
            write(root, "pkg/gadget.go", GO_ALPHA);
        }
        commit(root);
        db(root).execute_batch("CREATE TRIGGER reject_edges BEFORE INSERT ON edges BEGIN SELECT RAISE(FAIL, 'synthetic edge failure'); END;").unwrap();
        let out = keel(root, &["compile", "src/idle.rs"]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "failed write must refuse: {out:?}"
        );
        assert_eq!(
            marker(root),
            mapped,
            "failed writes must not move the marker"
        );
        assert_eq!(
            count(root, "SELECT count(*) FROM nodes WHERE name='alpha'"),
            1,
            "node write precedes failed edge write"
        );
        db(root)
            .execute_batch("DROP TRIGGER reject_edges;")
            .unwrap();
        clean_resync(root);
        assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='contains' AND n.name='alpha'"), 1, "retry must repair the missing Contains edge");
        if moved {
            assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='calls' AND n.name='Alpha' AND n.file_path='pkg/gadget.go'"), 1, "retry must resume dependents after the old caller edge cascaded away");
        }
    }
}

#[test]
fn removed_callee_does_not_abort_the_edge_batch() {
    let caller = "import { gadget } from './lib';\n/** Widget. */\nexport function widget(): number { return gadget(); }\n";
    let dir = fixture(&[
        (
            "src/lib.ts",
            "/** Gadget. */\nexport function gadget(): number { return 17; }\n",
        ),
        ("src/user.ts", caller),
    ]);
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "/** Beta. */\nexport function beta(): number { return 23; }\n",
    );
    write(
        root,
        "src/user.ts",
        &caller.replace("gadget();", "gadget() + 1;"),
    );
    commit(root);
    clean_resync(root);
    assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE e.kind='contains' AND n.name='beta'"), 1);
}

#[test]
fn committed_debt_stays_preexisting_like_a_fresh_map() {
    let initial = "export function alpha(value: number): number { return value + 1; }\n";
    let committed = initial.replace("+ 1", "+ 10");
    let dir = fixture(&[("src/lib.ts", initial)]);
    let root = dir.path();
    write(root, "src/lib.ts", &committed);
    commit(root);
    clean_resync(root);
    let synced = keel(root, &["compile", "src/lib.ts"]);
    map(root);
    let fresh = keel(root, &["compile", "src/lib.ts"]);
    assert_eq!(fresh.status.code(), Some(0), "fresh debt: {fresh:?}");
    assert!(
        String::from_utf8_lossy(&fresh.stdout).contains("E003"),
        "fixture must have docstring debt: {fresh:?}"
    );
    assert_eq!(
        synced.status.code(),
        Some(0),
        "committed debt must remain a WARNING: {synced:?}"
    );
    assert!(
        String::from_utf8_lossy(&synced.stdout).contains("warning[E003]"),
        "{synced:?}"
    );
}

#[cfg(unix)]
#[test]
fn head_blob_survives_an_uncommitted_symlink() {
    let initial = "/// Alpha.\npub fn alpha() -> u32 { 41 }\n";
    let committed = initial.replace("41", "43");
    let dir = fixture(&[("src/lib.rs", initial)]);
    let root = dir.path();
    write(root, "src/lib.rs", &committed);
    commit(root);
    fs::remove_file(root.join("src/lib.rs")).unwrap();
    std::os::unix::fs::symlink("idle.rs", root.join("src/lib.rs")).unwrap();
    clean_resync(root);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM nodes WHERE name='alpha' AND file_path='src/lib.rs'"
        ),
        1,
        "working-tree symlink must not prune HEAD's definition"
    );
    let mut parser = keel_parsers::treesitter::TreeSitterParser::new();
    let parsed = parser
        .parse_file("rust", Path::new("src/lib.rs"), &committed)
        .unwrap();
    let expected = parsed
        .definitions
        .iter()
        .find(|d| d.name == "alpha")
        .unwrap()
        .hash();
    let stored: String = db(root)
        .query_row("SELECT hash FROM nodes WHERE name='alpha'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, expected);
}

#[test]
fn package_heal_and_refresh_share_the_same_update() {
    let initial = "/// Alpha.\npub fn alpha(value: i32) -> i32 { value }\n";
    let dir = fixture(&[
        (
            "package.json",
            "{\"name\":\"pkg\",\"workspaces\":[\"packages/*\"]}",
        ),
        ("packages/alpha/package.json", "{\"name\":\"alpha\"}"),
        ("packages/alpha/lib.rs", initial),
    ]);
    let root = dir.path();
    let file = root.join(".keel/keel.json");
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    config["monorepo"]["enabled"] = false.into();
    fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
    map(root);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM nodes WHERE name='alpha' AND package IS NULL"
        ),
        1
    );
    config["monorepo"]["enabled"] = true.into();
    fs::write(file, serde_json::to_vec(&config).unwrap()).unwrap();
    write(
        root,
        "packages/alpha/lib.rs",
        &initial.replace("{ value }", "{ value + 1 }"),
    );
    commit(root);
    clean_resync(root);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM nodes WHERE name='alpha' AND package='alpha'"
        ),
        1,
        "refresh must retain the healed package"
    );
}

fn tally(name: &str) -> String {
    format!("/** {name}. */\nexport function {name}(values: number[], limit: number): number {{\n{BODY}}}\n")
}

#[test]
fn mapped_body_rows_have_live_hashes_including_disambiguated_siblings() {
    let method =
        format!("  /** Same. */\n  same(values: number[], limit: number): number {{\n{BODY}  }}\n");
    let source = format!("export class Alpha {{\n{method}}}\nexport class Beta {{\n{method}}}\n");
    let dir = fixture(&[("src/lib.ts", &source), ("src/alpha.ts", &tally("alpha"))]);
    let root = dir.path();
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='same'"),
        2
    );
    assert_eq!(
        count(root, "SELECT count(*) FROM body_index WHERE name='same'"),
        2
    );
    assert_eq!(count(root, "SELECT count(*) FROM body_index b WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.hash=b.node_hash AND n.file_path=b.file_path)"), 0, "BLOCKED V3: freshly mapped body rows must all have live graph identities");
    for source in [
        tally("beta"),
        tally("beta")
            .replace("total", "result")
            .replace("* 2", "* 3"),
    ] {
        write(root, "src/beta.ts", &source);
        let out = keel(root, &["compile", "src/beta.ts"]);
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("W006"),
            "live twin control: {out:?}"
        );
    }
}

fn removed_body_index_twin_is_silent(type_two: bool) {
    let dir = fixture(&[("src/alpha.ts", &tally("alpha"))]);
    let root = dir.path();
    fs::remove_file(root.join("src/alpha.ts")).unwrap();
    commit(root);
    clean_resync(root);
    let mut source = tally("beta");
    if type_two {
        source = source
            .replace("total", "result")
            .replace("current", "item")
            .replace("* 2", "* 3");
    }
    write(root, "src/beta.ts", &source);
    let synced = keel(root, &["compile", "src/beta.ts"]);
    map(root);
    let fresh = keel(root, &["compile", "src/beta.ts"]);
    assert!(
        !String::from_utf8_lossy(&fresh.stdout).contains("W006"),
        "fresh twin: {fresh:?}"
    );
    assert!(
        !String::from_utf8_lossy(&synced.stdout).contains("W006"),
        "removed twin must not count (type_two={type_two}): {synced:?}"
    );
}

#[test]
fn removed_type_one_body_index_twin_is_silent() {
    removed_body_index_twin_is_silent(false);
}

#[test]
fn removed_type_two_body_index_twin_is_silent() {
    removed_body_index_twin_is_silent(true);
}

#[test]
fn verbose_failed_resync_is_one_refusal_with_store_reason() {
    let dir = fixture(&[]);
    let root = dir.path();
    let mapped = marker(root);
    write(
        root,
        "src/alpha.rs",
        "/// Alpha.\npub fn alpha() -> u32 { 43 }\n",
    );
    commit(root);
    db(root).execute_batch("CREATE TRIGGER reject_edges BEFORE INSERT ON edges BEGIN SELECT RAISE(FAIL, 'synthetic edge failure'); END;").unwrap();
    let out = keel(root, &["--verbose", "compile", "src/idle.rs"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(marker(root), mapped);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        stderr.lines().count(),
        1,
        "refused re-sync must print exactly one line: {stderr}"
    );
    assert!(
        stderr.starts_with("keel compile: no usable graph — ")
            && stderr.contains("synthetic edge failure"),
        "{stderr}"
    );
}

#[test]
fn sixteen_changed_files_refuse_the_inline_budget() {
    let dir = fixture(&[]);
    let root = dir.path();
    let mapped = marker(root);
    for idx in 0..16 {
        write(
            root,
            &format!("src/alpha{idx}.rs"),
            &format!("/// Alpha.\npub fn alpha{idx}() -> u32 {{ {idx} }}\n"),
        );
    }
    commit(root);
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "16 changed files must exceed the measured inline budget: {out:?}"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("16 source files"));
    assert_eq!(marker(root), mapped);
}
