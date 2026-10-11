//! E002/E003 "pre-existing" comes from a Git base, not from graph history: code the
//! base commit does not hold is an ERROR on every compile, committed unchanged debt
//! stays a WARNING, and without Git the graph rule applies silently.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use common::{git, keel_bin};

const LEGACY_PY: &str = "def alpha(x, y):\n    return x + y\n\n\ndef beta(a):\n    return a * 2\n";
const CLEAN_PY: &str = "\"\"\"Tidy module.\"\"\"\n\n\ndef tidy(x: int) -> int:\n    \"\"\"Double x.\"\"\"\n    return x * 2\n";
const FRESH_PY: &str = "def fresh(q):\n    return q\n";
const GAMMA_PY: &str = "def gamma(z):\n    return z\n";
/// `alpha` with its body edited: a new hash under the same name.
const LEGACY_ALPHA_EDITED: &str =
    "def alpha(x, y):\n    return x + y + 1\n\n\ndef beta(a):\n    return a * 2\n";

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

fn describe(o: &Output) -> String {
    format!(
        "exit={:?}\nstdout: {}\nstderr: {}",
        o.status.code(),
        out_s(o),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn map(dir: &Path) {
    let m = keel_run(dir, &["map"]);
    assert!(m.status.success(), "keel map failed: {}", describe(&m));
}

fn commit(dir: &Path, paths: &[&str], message: &str) {
    let mut add = vec!["add"];
    add.extend_from_slice(paths);
    git(dir, &add);
    git(dir, &["commit", "-q", "--no-verify", "-m", message]);
}

/// A committed, mapped repo: `legacy.py` (two untyped functions), `clean.py` (typed).
fn mapped_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    write(dir.path(), "src/legacy.py", LEGACY_PY);
    write(dir.path(), "src/clean.py", CLEAN_PY);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(
        init.status.success(),
        "keel init failed: {}",
        describe(&init)
    );
    commit(dir.path(), &["src/legacy.py", "src/clean.py"], "first");
    map(dir.path());
    dir
}

/// Assert `keel compile <args>` exited with `code`; return its output.
fn compile(dir: &Path, args: &[&str], code: i32, what: &str) -> Output {
    let mut full = vec!["compile"];
    full.extend_from_slice(args);
    let o = keel_run(dir, &full);
    assert_eq!(
        o.status.code(),
        Some(code),
        "{what}: `keel {}`\n{}",
        full.join(" "),
        describe(&o)
    );
    o
}

/// The finding lines of one severity (`error` / `warning`) for one code.
fn findings(o: &Output, severity: &str, code: &str) -> Vec<String> {
    let prefix = format!("{severity}[{code}]");
    out_s(o)
        .lines()
        .filter(|l| l.starts_with(&prefix))
        .map(str::to_string)
        .collect()
}

fn assert_error_for(o: &Output, code: &str, name: &str, what: &str) {
    let hit = findings(o, "error", code)
        .iter()
        .any(|l| l.contains(&format!("`{name}`")));
    assert!(
        hit,
        "{what}: expected error[{code}] for `{name}`\n{}",
        describe(o)
    );
}

fn assert_warning_for(o: &Output, code: &str, name: &str, what: &str) {
    let hit = findings(o, "warning", code)
        .iter()
        .any(|l| l.contains(&format!("`{name}`")));
    assert!(
        hit,
        "{what}: expected warning[{code}] for `{name}`\n{}",
        describe(o)
    );
}

fn assert_no_pre_existing(o: &Output, what: &str) {
    assert!(
        !out_s(o).contains("pre-existing"),
        "{what}: nothing here is pre-existing\n{}",
        describe(o)
    );
}

/// The legacy file compiled clean-of-errors: both functions warn "pre-existing".
fn assert_legacy_pre_existing(o: &Output, what: &str) {
    for name in ["alpha", "beta"] {
        assert_warning_for(o, "E002", name, what);
        assert_warning_for(o, "E003", name, what);
    }
    assert!(
        out_s(o).contains("pre-existing"),
        "{what}: expected pre-existing\n{}",
        describe(o)
    );
}

/// The E002 hash of a `--json` compile, whichever severity list holds it.
fn e002_hash(o: &Output) -> (String, bool) {
    let v: serde_json::Value = serde_json::from_str(&out_s(o))
        .unwrap_or_else(|e| panic!("compile --json is not JSON ({e}): {}", describe(o)));
    for (list, is_error) in [("errors", true), ("warnings", false)] {
        if let Some(f) = v[list]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["code"] == "E002")
        {
            return (f["hash"].as_str().unwrap_or_default().to_string(), is_error);
        }
    }
    panic!("no E002 in compile --json: {}", describe(o));
}

/// Pin (a): an untracked untyped file blocks on every compile, not just the first.
#[test]
fn new_function_blocks_on_every_compile() {
    let dir = mapped_repo();
    write(dir.path(), "src/fresh.py", FRESH_PY);
    for round in 1..=3 {
        let what = format!("compile #{round} of an untracked file");
        let o = compile(dir.path(), &["src/fresh.py"], 1, &what);
        assert_error_for(&o, "E002", "fresh", &what);
        assert_no_pre_existing(&o, &what);
    }
}

/// The T2 fixture: a branch commit adds untyped `gamma.py`, another file is left
/// modified, then a full map.
fn branch_with_dirty_map() -> TempDir {
    let dir = mapped_repo();
    write(dir.path(), "src/gamma.py", GAMMA_PY);
    commit(dir.path(), &["src/gamma.py"], "second");
    let mut clean = fs::read_to_string(dir.path().join("src/clean.py")).unwrap();
    clean.push_str("# touched\n");
    write(dir.path(), "src/clean.py", &clean);
    map(dir.path());
    dir
}

/// Pin (b): a dirty `keel map` must not grandfather a branch's own additions.
#[test]
fn dirty_map_does_not_grandfather_a_branch_under_since() {
    let dir = branch_with_dirty_map();
    let o = compile(
        dir.path(),
        &["--since", "HEAD~1"],
        1,
        "since the branch point",
    );
    assert_error_for(&o, "E002", "gamma", "since the branch point");
    assert_error_for(&o, "E003", "gamma", "since the branch point");
}

/// The hook's one block is a delta (reported once); the finding is still an ERROR
/// for every plain compile and `--changed` afterwards.
#[test]
fn ignored_hook_block_is_still_an_error() {
    let dir = mapped_repo();
    let mut clean = fs::read_to_string(dir.path().join("src/clean.py")).unwrap();
    clean.push_str("\n\ndef sloppy(v):\n    return v\n");
    write(dir.path(), "src/clean.py", &clean);

    let hook = ["--delta", "--llm", "--", "src/clean.py"];
    compile(dir.path(), &hook, 1, "hook compile, first");
    compile(
        dir.path(),
        &hook,
        0,
        "hook compile, again (delta reports once)",
    );

    let o = compile(
        dir.path(),
        &["src/clean.py"],
        1,
        "plain compile after the hook",
    );
    assert_error_for(&o, "E002", "sloppy", "plain compile after the hook");
    compile(dir.path(), &["--changed"], 1, "--changed after the hook");
}

/// CONTROL: committed unchanged debt warns; editing one function escalates it alone.
#[test]
fn committed_debt_warns_and_escalates_when_edited() {
    let dir = mapped_repo();
    let o = compile(dir.path(), &["src/legacy.py"], 0, "committed debt");
    assert_legacy_pre_existing(&o, "committed debt");

    write(dir.path(), "src/legacy.py", LEGACY_ALPHA_EDITED);
    let o = compile(dir.path(), &["src/legacy.py"], 1, "after editing alpha");
    assert_error_for(&o, "E002", "alpha", "after editing alpha");
    assert_warning_for(&o, "E002", "beta", "after editing alpha");
    assert!(
        !findings(&o, "error", "E002")
            .iter()
            .any(|l| l.contains("`beta`")),
        "beta was not edited\n{}",
        describe(&o)
    );
}

/// An edit stays an ERROR through compiles and maps until it is reverted or committed.
#[test]
fn edited_debt_stays_error_until_reverted_or_committed() {
    let dir = mapped_repo();
    write(dir.path(), "src/legacy.py", LEGACY_ALPHA_EDITED);
    compile(dir.path(), &["src/legacy.py"], 1, "edit, first compile");
    compile(dir.path(), &["src/legacy.py"], 1, "edit, second compile");

    map(dir.path());
    let o = compile(dir.path(), &["src/legacy.py"], 1, "edit, after keel map");
    assert_error_for(&o, "E002", "alpha", "edit, after keel map");

    write(dir.path(), "src/legacy.py", LEGACY_PY);
    let o = compile(dir.path(), &["src/legacy.py"], 0, "edit reverted");
    assert_legacy_pre_existing(&o, "edit reverted");

    write(dir.path(), "src/legacy.py", LEGACY_ALPHA_EDITED);
    commit(dir.path(), &["src/legacy.py"], "edit alpha");
    compile(dir.path(), &["src/legacy.py"], 0, "edit committed");
}

/// A map on a dirty tree must not turn an untracked file into history.
#[test]
fn dirty_map_does_not_grandfather_an_untracked_file() {
    let dir = mapped_repo();
    write(dir.path(), "src/fresh.py", FRESH_PY);
    map(dir.path());
    let o = compile(dir.path(), &["src/fresh.py"], 1, "untracked file after map");
    assert_error_for(&o, "E002", "fresh", "untracked file after map");
}

/// A staged rename keeps untouched debt a WARNING; an edit there escalates alone.
#[test]
fn staged_rename_keeps_untouched_debt_a_warning() {
    let dir = mapped_repo();
    git(dir.path(), &["mv", "src/legacy.py", "src/moved.py"]);
    let o = compile(
        dir.path(),
        &["src/moved.py"],
        0,
        "first compile of the rename",
    );
    assert_legacy_pre_existing(&o, "first compile of the rename");

    write(dir.path(), "src/moved.py", LEGACY_ALPHA_EDITED);
    let o = compile(dir.path(), &["src/moved.py"], 1, "edit in the renamed file");
    assert_error_for(&o, "E002", "alpha", "edit in the renamed file");
    assert_warning_for(&o, "E002", "beta", "edit in the renamed file");
}

/// CONTROL: outside any repository the graph rule applies, silently.
#[test]
fn no_git_keeps_the_graph_rule() {
    let dir = TempDir::new().unwrap();
    for ancestor in dir.path().ancestors() {
        assert!(
            !ancestor.join(".git").exists(),
            "fixture must be outside git: {}",
            ancestor.display()
        );
    }
    write(dir.path(), "src/legacy.py", LEGACY_PY);
    write(dir.path(), "src/clean.py", CLEAN_PY);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(
        init.status.success(),
        "keel init failed: {}",
        describe(&init)
    );
    map(dir.path());

    let o = compile(dir.path(), &["src/legacy.py"], 0, "no git, legacy");
    assert_legacy_pre_existing(&o, "no git, legacy");

    let mut legacy = LEGACY_PY.to_string();
    legacy.push_str("\n\ndef gadget(w):\n    return w\n");
    write(dir.path(), "src/legacy.py", &legacy);
    let first = compile(dir.path(), &["src/legacy.py"], 1, "no git, added function");
    assert_error_for(&first, "E002", "gadget", "no git, added function");
    let second = compile(dir.path(), &["src/legacy.py"], 0, "no git, same again");
    for o in [&o, &first, &second] {
        assert!(
            !String::from_utf8_lossy(&o.stderr).contains("keel: progressive"),
            "no progressive note outside git\n{}",
            describe(o)
        );
    }
}

/// CONTROL: review counts the branch's findings as new, as before.
#[test]
fn review_is_unchanged() {
    let dir = branch_with_dirty_map();
    let o = keel_run(dir.path(), &["review", "--base", "HEAD~1", "--llm"]);
    assert!(
        out_s(&o).contains("NEW 2 new violation(s) (0 pre-existing)"),
        "review output\n{}",
        describe(&o)
    );
}

/// CONTROL: a CRLF working tree over an LF blob (autocrlf) stays pre-existing, also
/// for a multi-line signature.
#[test]
fn crlf_working_tree_against_lf_blob_stays_pre_existing() {
    let wrapped =
        "def alpha(\n    x,\n    y,\n):\n    return x + y\n\n\ndef beta(a):\n    return a * 2\n";
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "core.autocrlf", "true"]);
    let crlf = wrapped.replace('\n', "\r\n");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/legacy.py"), crlf.as_bytes()).unwrap();
    write(dir.path(), "src/clean.py", CLEAN_PY);
    let init = keel_run(dir.path(), &["init", "--yes"]);
    assert!(
        init.status.success(),
        "keel init failed: {}",
        describe(&init)
    );
    commit(dir.path(), &["src/legacy.py", "src/clean.py"], "first");

    let blob = Command::new("git")
        .args(["cat-file", "-p", "HEAD:src/legacy.py"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(blob.status.success(), "git cat-file failed");
    assert!(!blob.stdout.contains(&b'\r'), "the blob must hold LF only");
    let disk = fs::read(dir.path().join("src/legacy.py")).unwrap();
    assert!(disk.contains(&b'\r'), "the working file must hold CRLF");

    map(dir.path());
    let o = compile(dir.path(), &["src/legacy.py"], 0, "CRLF over an LF blob");
    assert_legacy_pre_existing(&o, "CRLF over an LF blob");
}

/// A function on line 1 of a new file keeps one hash across compiles, so `--delta`
/// reports it once and every plain compile calls it an error.
#[test]
fn function_on_line_one_keeps_its_hash_and_delta_reports_it_once() {
    let dir = mapped_repo();
    write(dir.path(), "src/fresh.py", FRESH_PY);
    let delta = ["--delta", "--llm", "--", "src/fresh.py"];
    compile(dir.path(), &delta, 1, "delta, first");
    compile(dir.path(), &delta, 0, "delta, again");

    let dir = mapped_repo();
    write(dir.path(), "src/fresh.py", FRESH_PY);
    let hashes: Vec<(String, bool)> = (1..=3)
        .map(|round| {
            let o = compile(
                dir.path(),
                &["--json", "src/fresh.py"],
                1,
                &format!("json compile #{round}"),
            );
            e002_hash(&o)
        })
        .collect();
    assert!(
        hashes.iter().all(|(_, is_error)| *is_error),
        "E002 must be an error each time: {hashes:?}"
    );
    assert!(
        hashes.iter().all(|(h, _)| *h == hashes[0].0),
        "the hash moved between compiles of an unchanged file: {hashes:?}"
    );
}
