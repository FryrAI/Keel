//! Acceptance tests for committed removals before a working-tree compile.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{git, keel_bin};
use tempfile::TempDir;

const ALPHA: &str =
    "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const GADGET: &str = "/** Gadget. */\nexport function gadget(): number { return 43; }\n";
const CALLER: &str = "import { gadget } from './lib';\n/** Widget. */\nexport function widget(): number { return gadget(); }\n";
const IDLE: &str = "/** Idle. */\nexport function idle(): number { return 29; }\n";
const HOLLOW: &str = "/** Hollow. */\nexport function hollow_fn(): number { return 47; }\n";
const BETA: &str = "/** Beta. */\nexport function beta(): number { return 83; }\n";

// These helpers mirror tests/graph_resync.rs.
fn keel(dir: &Path, args: &[&str]) -> Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

fn write(dir: &Path, rel: &str, content: &str) {
    let file = dir.join(rel);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

fn git_output(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git")
}

fn head(dir: &Path) -> String {
    let out = git_output(dir, &["rev-parse", "HEAD"]);
    assert!(out.status.success(), "fixture must have HEAD");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn meta(dir: &Path, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        dir.join(".keel/graph.db"),
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

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--no-verify", "-m", message]);
}

fn map(dir: &Path) -> String {
    let out = keel(dir, &["map"]);
    assert!(out.status.success(), "fixture map failed: {out:?}");
    let mapped = head(dir);
    assert_eq!(meta(dir, "last_map_commit"), Some(mapped.clone()));
    mapped
}

fn search(root: &Path, term: &str) -> serde_json::Value {
    let out = keel(root, &["search", term, "--json"]);
    assert!(out.status.success(), "search failed: {out:?}");
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    result["results"].clone()
}

fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "src/lib.ts", &format!("{ALPHA}{GADGET}"));
    write(root, "src/caller.ts", CALLER);
    write(root, "src/idle.ts", IDLE);
    write(root, "src/extra.ts", HOLLOW);
    git(root, &["init", "-q", "-b", "base"]);
    let out = keel(root, &["init"]);
    assert!(out.status.success(), "fixture init failed: {out:?}");
    commit(root, "base");
    map(root);
    dir
}

/// The agent must see E004 when its own edit removes a callee still present at HEAD.
#[test]
fn control_mapped_head_reports_working_tree_gadget_removal() {
    let dir = fixture();
    let root = dir.path();
    write(root, "src/lib.ts", ALPHA);
    let out = keel(root, &["compile", "src/lib.ts"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "working-tree removal must fail enforcement: {out:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E004"),
        "working-tree gadget removal must report E004: {stdout}"
    );
}

/// A stale graph must not blame the agent for a gadget removal already committed at HEAD.
#[test]
fn ancestor_move_does_not_report_committed_gadget_removal() {
    let dir = fixture();
    let root = dir.path();
    let mapped = head(root);
    write(root, "src/lib.ts", ALPHA);
    commit(root, "remove gadget");
    assert_ne!(head(root), mapped);
    git(root, &["merge-base", "--is-ancestor", &mapped, "HEAD"]);
    assert_eq!(meta(root, "last_map_commit"), Some(mapped));
    assert_eq!(
        fs::read_to_string(root.join("src/caller.ts")).unwrap(),
        CALLER
    );
    write(root, "src/lib.ts", &format!("{ALPHA}{BETA}"));

    let out = keel(root, &["compile", "src/lib.ts"]);
    assert!(
        matches!(out.status.code(), Some(0 | 1)),
        "committed removal must be checked without refusal: {out:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("E004"),
        "committed gadget removal must not report E004: {stdout}"
    );
    for line in stdout.lines() {
        let has_error_code = line
            .as_bytes()
            .windows(4)
            .any(|code| code[0] == b'E' && code[1..].iter().all(u8::is_ascii_digit));
        assert!(
            !(line.contains("gadget") && has_error_code),
            "committed gadget removal must have no error finding: {stdout}"
        );
    }
}

/// Search must not offer the agent hollow_fn after HEAD leaves its file with no definitions.
#[test]
fn ancestor_move_prunes_definitions_from_comment_only_file() {
    let dir = fixture();
    let root = dir.path();
    let mapped = head(root);
    assert!(
        search(root, "hollow_fn")
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["file"] == "src/extra.ts"),
        "fixture map must contain hollow_fn"
    );
    write(root, "src/extra.ts", "// No definitions remain.\n");
    commit(root, "empty definitions");
    assert_ne!(head(root), mapped);
    git(root, &["merge-base", "--is-ancestor", &mapped, "HEAD"]);
    assert_eq!(meta(root, "last_map_commit"), Some(mapped));
    assert_eq!(fs::read_to_string(root.join("src/idle.ts")).unwrap(), IDLE);

    let out = keel(root, &["compile", "src/idle.ts"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean unrelated file must compile: {out:?}"
    );
    assert_eq!(
        search(root, "hollow_fn"),
        serde_json::json!([]),
        "comment-only file must lose hollow_fn from search"
    );
}
