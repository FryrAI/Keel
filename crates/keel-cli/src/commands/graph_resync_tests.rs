use std::path::Path;
use std::process::Command;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit_files(dir: &Path, count: usize, tag: &str) -> String {
    for i in 0..count {
        std::fs::write(
            dir.join(format!("{tag}_{i}.rs")),
            format!("/// Doc.\npub fn {tag}_{i}() -> u32 {{ {i} }}\n"),
        )
        .unwrap();
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", tag]);
    gitdiff::head_commit(dir).expect("HEAD")
}

/// A repo with one commit and a graph whose marker names it.
fn mapped_repo() -> (tempfile::TempDir, SqliteGraphStore, String) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    let mapped = commit_files(dir.path(), 1, "first");
    let db = dir.path().join("graph.db");
    let store = SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    store.set_meta_value(LAST_MAP_COMMIT, &mapped).unwrap();
    (dir, store, mapped)
}

fn resync(dir: &Path, store: &mut SqliteGraphStore, explicit_targets: bool) -> Result<(), String> {
    bring_to_head(
        dir,
        store,
        explicit_targets,
        &BTreeSet::new(),
        &KeelConfig::default(),
        false,
    )
}

#[test]
fn never_mapped_predicate_distinguishes_missing_completion_and_commit() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteGraphStore::in_memory().unwrap();
    let unfinished = "this repository has never been mapped (or the last `keel map` did not finish). Run `keel map` to build the graph; nothing was checked.";
    let unrecorded = "the stored graph does not record the commit it was built at (it was mapped before the first commit, or by an older keel). Run `keel map` to rebuild it; nothing was checked.";
    assert_eq!(never_mapped_reason(dir.path(), &store), Some(unfinished));
    store.set_meta_value(LAST_MAP_AT, "1700000000").unwrap();
    assert_eq!(never_mapped_reason(dir.path(), &store), None);
    git(dir.path(), &["init", "-q"]);
    assert_eq!(never_mapped_reason(dir.path(), &store), None);
    let head = commit_files(dir.path(), 1, "alpha");
    assert_eq!(never_mapped_reason(dir.path(), &store), Some(unrecorded));
    store.set_meta_value(LAST_MAP_COMMIT, &head).unwrap();
    assert_eq!(never_mapped_reason(dir.path(), &store), None);
    store.clear_map_markers().unwrap();
    store.set_meta_value(LAST_MAP_COMMIT, &head).unwrap();
    assert_eq!(never_mapped_reason(dir.path(), &store), Some(unfinished));
}

/// The three refusal shapes share the prefix and the 12-char commit, and name
/// the command that fixes each state.
#[test]
fn refusal_messages_carry_prefix_commit_and_fix() {
    let stored = "0123456789abcdef0123";
    let gone = gone_message(stored);
    let many = too_many_message(stored, 700, "`keel map` to rebuild it");
    let inline = too_many_message(stored, 16, "`keel compile` to bring it up to date");
    let failed = failed_message(stored, "disk full");
    for line in [&gone, &many, &inline, &failed] {
        assert!(line.starts_with(NO_USABLE_GRAPH), "{line}");
        assert!(line.contains("0123456789ab") && !line.contains("0123456789abc"));
        assert!(line.ends_with("nothing was checked."), "{line}");
    }
    assert!(gone.contains("`keel map`") && gone.contains("no longer has"));
    assert!(many.contains("700 source files") && many.contains("`keel map`"));
    assert!(inline.contains("16 source files") && inline.contains("`keel compile`"));
    assert!(failed.contains("(disk full)") && failed.contains("`keel map`"));
}

/// Without a marker, or with the marker at HEAD, nothing is read or written.
#[test]
fn no_marker_and_current_marker_are_silent_no_ops() {
    let (dir, mut store, mapped) = mapped_repo();
    assert_eq!(resync(dir.path(), &mut store, true), Ok(()));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(mapped));

    let (dir, _, _) = mapped_repo();
    commit_files(dir.path(), 1, "second");
    let db = dir.path().join("unmarked.db");
    let mut store = SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    assert_eq!(resync(dir.path(), &mut store, true), Ok(()));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), None);
    assert!(store.get_nodes_in_file("second_0.rs").is_empty());
}

/// A stored commit this clone does not have is a refusal, and the marker stays.
#[test]
fn an_unknown_stored_commit_refuses_and_keeps_the_marker() {
    let (dir, mut store, _) = mapped_repo();
    let gone = "f".repeat(40);
    store.set_meta_value(LAST_MAP_COMMIT, &gone).unwrap();
    let line = resync(dir.path(), &mut store, false).unwrap_err();
    assert!(line.starts_with(NO_USABLE_GRAPH) && line.contains(&gone[..12]));
    assert!(line.contains("`keel map`"));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(gone));
}

/// An explicit-file compile refuses above the inline limit and names
/// `keel compile`; a bare compile of the same move catches up and moves the
/// marker to HEAD.
#[test]
fn the_inline_limit_refuses_explicit_files_but_not_a_bare_compile() {
    let (dir, mut store, mapped) = mapped_repo();
    let head = commit_files(dir.path(), RESYNC_INLINE_MAX + 1, "more");
    let line = resync(dir.path(), &mut store, true).unwrap_err();
    assert!(line.contains(&format!("{} source files", RESYNC_INLINE_MAX + 1)));
    assert!(line.contains("`keel compile`"));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(mapped));

    assert_eq!(resync(dir.path(), &mut store, false), Ok(()));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(head));
    assert!(!store.get_nodes_in_file("more_15.rs").is_empty());
}

/// A completed re-sync indexes an added file from HEAD's blob, whatever the
/// working tree says, and moves the marker to HEAD.
#[test]
fn a_completed_resync_follows_head_not_the_working_tree() {
    let (dir, mut store, _) = mapped_repo();
    let root = dir.path();
    let head = commit_files(root, 1, "second");
    std::fs::write(root.join("second_0.rs"), "// working tree edit\n").unwrap();

    assert_eq!(resync(root, &mut store, true), Ok(()));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(head));
    assert!(store
        .get_nodes_in_file("second_0.rs")
        .iter()
        .any(|n| n.name == "second_0"));
}

const LIB: &str = "/** Alpha. */\nexport function alpha(value: number): number { return value; }\n";
const CALLER: &str = "import { alpha } from './lib';\n/** Widget. */\nexport function widget(): number { return alpha(7); }\n";

fn commit_ts(dir: &Path, lib: &str, caller: &str, message: &str) -> String {
    std::fs::write(dir.join("lib.ts"), lib).unwrap();
    std::fs::write(dir.join("caller.ts"), caller).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    gitdiff::head_commit(dir).expect("HEAD")
}

fn widget_calls_alpha(store: &SqliteGraphStore) -> Vec<keel_core::types::GraphEdge> {
    let id = |file: &str, name: &str| {
        store
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.name == name)
            .unwrap()
            .id
    };
    let (widget, alpha) = (id("caller.ts", "widget"), id("lib.ts", "alpha"));
    store
        .all_edges()
        .into_iter()
        .filter(|e| e.kind == keel_core::types::EdgeKind::Calls)
        .filter(|e| e.source_id == widget && e.target_id == alpha)
        .collect()
}

/// An edge to a definition the re-sync itself adds exists only because a second
/// pass re-resolves after the first one stored the node; a re-sync that adds no
/// node runs one pass, so the re-added call edge takes the next id exactly.
#[test]
fn the_second_pass_runs_only_when_the_first_added_a_node() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    let first = commit_ts(root, "// nothing yet\n", "// nothing yet\n", "first");
    let db = root.join("graph.db");
    let mut store = SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    store.set_meta_value(LAST_MAP_COMMIT, &first).unwrap();
    commit_ts(root, LIB, CALLER, "second");

    assert_eq!(resync(root, &mut store, false), Ok(()));
    assert_eq!(
        widget_calls_alpha(&store).len(),
        1,
        "pass 2 resolves the edge to the node pass 1 added"
    );

    let third = commit_ts(root, LIB, &CALLER.replace('7', "8"), "third");
    let watermark = store.max_id();
    assert_eq!(resync(root, &mut store, false), Ok(()));
    assert_eq!(store.meta_value(LAST_MAP_COMMIT), Some(third));
    let edges = widget_calls_alpha(&store);
    assert_eq!(edges.len(), 1);
    assert_eq!(
        edges[0].id,
        watermark + 1,
        "no node was added, so exactly one pass re-added the edge"
    );
}

#[test]
fn hook_prefix_matches_the_rust_constant() {
    let template = include_str!("../../templates/hooks/post-edit.sh");
    let literal = template
        .lines()
        .find_map(|line| {
            line.strip_prefix("NO_USABLE_GRAPH='")
                .and_then(|s| s.strip_suffix('\''))
        })
        .expect("hook prefix literal");
    assert_eq!(literal, NO_USABLE_GRAPH);
}
