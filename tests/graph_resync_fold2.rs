//! Regression cases for body-index cleanup and re-sync recovery across HEADs.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel_bin};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const IDLE: &str = "/// Widget.\npub fn widget() -> u32 { 29 }\n";
const GO_ALPHA: &str =
    "package pkg\n\n// Alpha.\nfunc Alpha(value int) int {\n    return value + 1\n}\n";
const GO_BETA: &str =
    "package pkg\n\n// Beta.\nfunc Beta(value int) int {\n    return value * 3\n}\n";
const GO_CALLER: &str = "package pkg\n\n// Widget.\nfunc Widget() int {\n    return Alpha(1)\n}\n";
const GO_GADGET: &str = "\n// Gadget.\nfunc Gadget(value int) int {\n    return Alpha(value)\n}\n";
const TS_ALPHA: &str =
    "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const TS_CALLER: &str = "import { alpha } from './alpha';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";
const BODY: &str = "  let total = 0;\n  for (let index = 0; index < values.length; index += 1) {\n    const current = values[index];\n    if (current > limit) {\n      total += current * 2;\n    } else if (current < 0) {\n      total -= current;\n    } else {\n      total += current;\n    }\n  }\n  return total;\n";

// These helpers mirror tests/graph_resync_fold.rs.
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

fn broken_matches_map(root: &Path, rel: &str, committed: &str, edit: &str) {
    write(root, rel, edit);
    let synced = keel(root, &["compile", rel]);
    write(root, rel, committed);
    map(root);
    write(root, rel, edit);
    let fresh = keel(root, &["compile", rel]);
    assert_eq!(fresh.status.code(), Some(1), "oracle: {fresh:?}");
    assert!(String::from_utf8_lossy(&fresh.stdout).contains("E001"));
    same_output(&synced, &fresh);
}

fn tally(name: &str) -> String {
    format!("/** {name}. */\nexport function {name}(values: number[], limit: number): number {{\n{BODY}}}\n")
}

#[test]
fn ordinary_compile_keeps_w006_after_twin_doc_edit() {
    let dir = fixture(&[("src/alpha.ts", &tally("alpha"))]);
    let root = dir.path();
    write(
        root,
        "src/alpha.ts",
        &tally("alpha").replace("/** alpha. */", "/** Alpha widget. */"),
    );
    assert!(keel(root, &["compile", "src/alpha.ts"]).status.success());
    assert_eq!(marker(root), head(root));
    write(root, "src/beta.ts", &tally("beta"));
    let ordinary = keel(root, &["compile", "src/beta.ts"]);
    assert!(
        String::from_utf8_lossy(&ordinary.stdout).contains("W006"),
        "ordinary compile must name the existing twin: {ordinary:?}"
    );
    assert!(String::from_utf8_lossy(&ordinary.stdout).contains("src/alpha.ts"));
    fs::remove_file(root.join("src/beta.ts")).unwrap();
    map(root);
    write(root, "src/beta.ts", &tally("beta"));
    same_output(&ordinary, &keel(root, &["compile", "src/beta.ts"]));
}

#[test]
fn removed_definition_loses_only_its_body_index_row() {
    let source = format!("{}{}", tally("alpha"), tally("beta"));
    let dir = fixture(&[("src/alpha.ts", &source)]);
    let root = dir.path();
    write(
        root,
        "src/alpha.ts",
        &tally("beta").replace("/** beta. */", "/** Beta widget. */"),
    );
    commit(root);
    clean_resync(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM body_index WHERE name='alpha'"),
        0,
        "removed definition's body row must be deleted"
    );
    assert_eq!(
        count(root, "SELECT count(*) FROM body_index WHERE name='beta'"),
        1,
        "changed survivor keeps its mapped body row"
    );
}

#[test]
fn dependent_reresolution_preserves_compiled_working_tree_nodes() {
    let dir = fixture(&[
        ("go.mod", "module pkg\n\ngo 1.21\n"),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_CALLER),
    ]);
    let root = dir.path();
    let dirty = format!("{GO_CALLER}{GO_GADGET}");
    write(root, "pkg/beta.go", &dirty);
    write(
        root,
        "pkg/widget.go",
        "package pkg\n\n// Beta.\nfunc Beta() int {\n    return Gadget(2)\n}\n",
    );
    assert!(keel(root, &["compile", "pkg/beta.go"]).status.success());
    assert!(keel(root, &["compile", "pkg/widget.go"]).status.success());
    let nodes_before = dependent_nodes(root);
    let edges_before = dump(root, "SELECT json_array(e.source_id,e.target_id,e.kind,e.file_path,e.line) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE n.name='Gadget' ORDER BY e.source_id, e.target_id, e.kind, e.file_path, e.line");
    assert_eq!(count(root, "SELECT count(*) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE n.name='Gadget' AND e.kind='calls'"), 1);
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    git(root, &["add", "pkg/alpha.go", "pkg/gadget.go"]);
    git(root, &["commit", "-q", "--no-verify", "-m", "move alpha"]);
    clean_resync(root);
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='Gadget'"),
        1,
        "dependent must retain the working-tree-only function"
    );
    assert_eq!(
        dependent_nodes(root),
        nodes_before,
        "dependent nodes must remain exactly unchanged"
    );
    assert_eq!(dump(root, "SELECT json_array(e.source_id,e.target_id,e.kind,e.file_path,e.line) FROM edges e JOIN nodes n ON n.id=e.target_id WHERE n.name='Gadget' ORDER BY e.source_id, e.target_id, e.kind, e.file_path, e.line"), edges_before);
    broken_matches_map(
        root,
        "pkg/beta.go",
        &dirty,
        &dirty.replace("Gadget(value int)", "Gadget(value int, extra int)"),
    );
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

fn dependent_nodes(root: &Path) -> Vec<String> {
    dump(root, "SELECT json_array(id,hash,kind,name,file_path,signature,line_start,line_end,module_id,package,docstring,is_public,type_hints_present,has_docstring,is_associated,complexity,is_trivial_wrapper,in_test_context,resolution_tier,created_at,updated_at,(SELECT group_concat(hash) FROM previous_hashes WHERE node_id=nodes.id)) FROM nodes WHERE file_path='pkg/beta.go' ORDER BY id")
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

#[test]
fn pending_resync_back_at_baseline_restores_nodes_and_edges() {
    let dir = fixture(&[
        ("go.mod", "module pkg\n\ngo 1.21\n"),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_CALLER),
    ]);
    let root = dir.path();
    let mapped = marker(root);
    git(root, &["checkout", "-q", "-b", "alpha"]);
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    commit(root);
    fail_resync(root, &mapped);
    let saved: String = db(root)
        .query_row(
            "SELECT value FROM keel_meta WHERE key='resync_pending'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (baseline, changed, dependents): (String, Vec<String>, Vec<String>) =
        serde_json::from_str(&saved).unwrap();
    assert_eq!(baseline, mapped);
    assert_eq!(changed, ["pkg/alpha.go", "pkg/gadget.go"]);
    assert_eq!(dependents, ["pkg/beta.go"]);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM nodes WHERE name='Alpha' AND file_path='pkg/alpha.go'"
        ),
        0,
        "failure must follow node removals"
    );
    git(root, &["checkout", "-q", "base"]);
    clean_resync(root);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM keel_meta WHERE key='resync_pending'"
        ),
        0
    );
    let oracle = fixture(&[
        ("go.mod", "module pkg\n\ngo 1.21\n"),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_CALLER),
    ]);
    assert_eq!(
        graph_dump(root),
        graph_dump(oracle.path()),
        "returning to G must restore G's graph"
    );
    broken_matches_map(
        root,
        "pkg/alpha.go",
        &format!("{GO_ALPHA}{GO_BETA}"),
        &format!(
            "{}{GO_BETA}",
            GO_ALPHA.replace("value int)", "value int, extra int)")
        ),
    );
}

#[test]
fn pending_resync_third_commit_drops_other_branch_content() {
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
    write(
        root,
        "src/gadget.ts",
        &TS_CALLER
            .replace("widget", "gadget")
            .replace("alpha(7)", "alpha(1, 2)"),
    );
    commit(root);
    fail_resync(root, &mapped);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM nodes WHERE name='alpha' AND signature LIKE '%extra%'"
        ),
        1,
        "failure must follow signature refresh"
    );
    git(root, &["checkout", "-q", "base"]);
    git(root, &["checkout", "-q", "-b", "beta"]);
    write(root, "src/idle.rs", &IDLE.replace("29", "31"));
    commit(root);
    clean_resync(root);
    let oracle = fixture(&[
        ("src/alpha.ts", TS_ALPHA),
        ("src/widget.ts", TS_CALLER),
        ("src/idle.rs", &IDLE.replace("29", "31")),
    ]);
    assert_eq!(graph_dump(root), graph_dump(oracle.path()));
    write(root, "src/beta.ts", &TS_CALLER.replace("widget", "beta"));
    let recovered = keel(root, &["compile", "src/beta.ts"]);
    assert_eq!(
        recovered.status.code(),
        Some(0),
        "correct call must not see another branch's signature: {recovered:?}"
    );
    assert!(!String::from_utf8_lossy(&recovered.stdout).contains("E005"));
    write(
        oracle.path(),
        "src/beta.ts",
        &TS_CALLER.replace("widget", "beta"),
    );
    same_output(
        &recovered,
        &keel(oracle.path(), &["compile", "src/beta.ts"]),
    );
}

#[test]
fn invalid_pending_record_does_not_refuse() {
    for value in ["garbage", "{}", "[\"alpha\"]"] {
        let dir = fixture(&[]);
        let root = dir.path();
        // The old key pins the red case; the replacement key pins recovery.
        for key in ["resync_pending_dependents", "resync_pending"] {
            db(root)
                .execute(
                    "INSERT OR REPLACE INTO keel_meta(key,value) VALUES(?1,?2)",
                    [key, value],
                )
                .unwrap();
        }
        write(
            root,
            "src/alpha.rs",
            "/// Alpha.\npub fn alpha() -> u32 { 43 }\n",
        );
        commit(root);
        clean_resync(root);
    }
}

#[test]
fn pending_paths_and_dependents_count_toward_both_limits() {
    for (paths, dependents, limit, fix) in [
        (8, 8, true, "`keel compile`"),
        (251, 250, false, "`keel map`"),
    ] {
        let dir = fixture(&[]);
        let root = dir.path();
        let mapped = marker(root);
        let changed: Vec<_> = (0..paths).map(|i| format!("src/alpha_{i}.rs")).collect();
        let callers: Vec<_> = (0..dependents)
            .map(|i| format!("src/beta_{i}.rs"))
            .collect();
        let record = serde_json::to_string(&(&mapped, changed, callers)).unwrap();
        db(root)
            .execute(
                "INSERT INTO keel_meta(key,value) VALUES('resync_pending',?1)",
                [&record],
            )
            .unwrap();
        let out = keel(
            root,
            if limit {
                &["compile", "src/idle.rs"]
            } else {
                &["compile"]
            },
        );
        assert_eq!(
            out.status.code(),
            Some(2),
            "recorded paths count even at G: {out:?}"
        );
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("{} source files", paths + dependents))
                && stderr.contains(fix)
        );
        assert_eq!(marker(root), mapped);
        assert_eq!(
            count(
                root,
                "SELECT count(*) FROM keel_meta WHERE key='resync_pending'"
            ),
            1
        );
        if limit {
            let out = keel(root, &["compile"]);
            assert!(
                out.status.success(),
                "bare compile recovers within its limit: {out:?}"
            );
            assert_eq!(
                count(
                    root,
                    "SELECT count(*) FROM keel_meta WHERE key='resync_pending'"
                ),
                0
            );
        }
    }
}

#[test]
fn failed_pending_clear_rolls_back_marker_and_keeps_record() {
    let dir = fixture(&[]);
    let root = dir.path();
    let mapped = marker(root);
    write(
        root,
        "src/alpha.rs",
        "/// Alpha.\npub fn alpha() -> u32 { 43 }\n",
    );
    commit(root);
    db(root).execute_batch("CREATE TRIGGER reject_pending_clear BEFORE DELETE ON keel_meta WHEN OLD.key='resync_pending' BEGIN SELECT RAISE(FAIL, 'synthetic pending failure'); END;").unwrap();
    let out = keel(root, &["compile", "src/idle.rs"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "completion must refuse: {out:?}"
    );
    assert_eq!(
        marker(root),
        mapped,
        "record clear and marker must share a transaction"
    );
    assert_eq!(
        count(root, "SELECT count(*) FROM nodes WHERE name='alpha'"),
        1
    );
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM keel_meta WHERE key='resync_pending'"
        ),
        1
    );
    db(root)
        .execute_batch("DROP TRIGGER reject_pending_clear")
        .unwrap();
    clean_resync(root);
    assert_eq!(
        count(
            root,
            "SELECT count(*) FROM keel_meta WHERE key='resync_pending'"
        ),
        0
    );
}

#[test]
fn full_map_voids_pending_resync_record() {
    let dir = fixture(&[
        ("go.mod", "module pkg\n\ngo 1.21\n"),
        ("pkg/alpha.go", &format!("{GO_ALPHA}{GO_BETA}")),
        ("pkg/beta.go", GO_CALLER),
    ]);
    let root = dir.path();
    let mapped = marker(root);
    git(root, &["checkout", "-q", "-b", "alpha"]);
    write(root, "pkg/alpha.go", GO_BETA);
    write(root, "pkg/gadget.go", GO_ALPHA);
    commit(root);
    fail_resync(root, &mapped);
    git(root, &["checkout", "-q", "base"]);
    let committed = format!("{GO_ALPHA}{GO_BETA}");
    let extra = "\n// Delta.\nfunc Delta(value int) int {\n    return value\n}\n";
    let caller = "\n// Epsilon.\nfunc Epsilon() int {\n    return Delta(1)\n}\n";
    write(root, "pkg/alpha.go", &format!("{committed}{extra}"));
    write(root, "pkg/beta.go", &format!("{GO_CALLER}{caller}"));
    map(root);
    let pending = "SELECT count(*) FROM keel_meta WHERE key='resync_pending'";
    assert_eq!(count(root, pending), 0, "a full map must void the record");
    let delta = "SELECT count(*) FROM nodes WHERE name='Delta'";
    assert_eq!(
        count(root, delta),
        1,
        "map indexes the working-tree function"
    );
    assert_eq!(
        keel(root, &["compile", "pkg/beta.go"]).status.code(),
        Some(0)
    );
    assert_eq!(count(root, delta), 1, "compile must keep the map's node");
    let edit = format!(
        "{committed}{}",
        extra.replace("Delta(value int)", "Delta(value int, extra int)")
    );
    write(root, "pkg/alpha.go", &edit);
    let synced = keel(root, &["compile", "pkg/alpha.go"]);
    assert_eq!(synced.status.code(), Some(1), "E001 expected: {synced:?}");
    let stdout = String::from_utf8_lossy(&synced.stdout);
    assert!(
        stdout.contains("E001") && stdout.contains("callers: pkg/beta.go"),
        "{stdout}"
    );
    write(root, "pkg/alpha.go", &format!("{committed}{extra}"));
    map(root);
    write(root, "pkg/alpha.go", &edit);
    same_output(&synced, &keel(root, &["compile", "pkg/alpha.go"]));
}
