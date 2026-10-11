//! `keel hook` script lookup in nested project layouts, `init --merge`
//! migration, unreadable hook directories, and the checkout in which
//! `post-edit` checks a file edited outside the session's own.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use tempfile::TempDir;

use common::{git, keel_bin};

// The helpers below mirror `tests/hook_command.rs`.

fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let bin = keel_bin();
    let path = format!(
        "{}:{}",
        bin.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child = Command::new(&bin)
        .args(args)
        .current_dir(cwd)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to run `keel {}`: {e}", args.join(" ")));
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn write_script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    common::write_executable(path, body);
}

/// A stub hook that reports its cwd, its arguments and its stdin on stdout,
/// then exits 7. Mirrors `tests/hook_command.rs`.
const REPORTING_STUB: &str = "#!/bin/sh\n\
    printf 'cwd=%s\\n' \"$(pwd -P)\"\n\
    printf 'args='; for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'\n\
    printf 'stdin='; cat\n\
    exit 7\n";

fn commands_in(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                match (key.as_str(), v) {
                    ("command", Value::String(s)) => out.push(s.clone()),
                    _ => commands_in(v, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| commands_in(v, out)),
        _ => {}
    }
}

fn commands_of_file(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap();
    let mut found = Vec::new();
    commands_in(&serde_json::from_str(&text).unwrap(), &mut found);
    found
}

/// A git repository `mono/` with a package `mono/pkg/` holding `.claude/`
/// and a nested `mono/pkg/sub/`. Returns the guard, `pkg` and `pkg/sub`.
fn mono_repo() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let mono = dir.path().canonicalize().unwrap().join("mono");
    let pkg = mono.join("pkg");
    let sub = pkg.join("sub");
    fs::create_dir_all(pkg.join(".claude")).unwrap();
    fs::create_dir_all(&sub).unwrap();
    git(&mono, &["init", "-q"]);
    (dir, pkg, sub)
}

#[test]
fn hook_finds_scripts_installed_in_a_package_subdirectory() {
    let (_dir, pkg, sub) = mono_repo();
    let init = run(&pkg, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    for cwd in [&pkg, &sub] {
        for name in ["session-start", "plan-check"] {
            let out = run(cwd, &["hook", name], b"{}");
            assert!(
                !stderr_of(&out).contains("script not found"),
                "{name} from {}: {}",
                cwd.display(),
                stderr_of(&out)
            );
            assert_eq!(
                out.status.code(),
                Some(0),
                "{name} from {}: {}",
                cwd.display(),
                stderr_of(&out)
            );
        }
    }
}

#[test]
fn hook_runs_the_nearest_ancestor_script_in_its_own_directory() {
    let (_dir, pkg, sub) = mono_repo();
    write_script(&pkg.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);
    let out = run(&sub, &["hook", "post-edit"], b"");
    assert_eq!(out.status.code(), Some(7), "{}", stderr_of(&out));
    assert!(
        stdout_of(&out).contains(&format!("cwd={}", pkg.display())),
        "stdout={:?}",
        stdout_of(&out)
    );
}

#[test]
fn init_merge_migrates_old_commands_instead_of_duplicating_them() {
    let (_dir, pkg, _sub) = mono_repo();
    fs::create_dir_all(pkg.join(".cursor")).unwrap();
    let init = run(&pkg, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    let claude = pkg.join(".claude/settings.json");
    let cursor = pkg.join(".cursor/hooks.json");
    let old_claude = json!({"hooks": {
        "SessionStart": [{"hooks": [
            {"type": "command", "command": "timeout 10 keel map --llm --cached"}]}],
        "PreToolUse": [{"matcher": "ExitPlanMode", "hooks": [
            {"type": "command", "command": ".keel/hooks/plan-check.sh"}]}],
        "PostToolUse": [{"matcher": "Edit|MultiEdit|Write", "hooks": [
            {"type": "command", "command": ".keel/hooks/post-edit.sh claude-code"}]}]
    }});
    let old_cursor = json!({"hooks": {"SessionStart": [
        {"command": "timeout 10 keel map --llm --cached", "type": "command"}]}});
    fs::write(&claude, old_claude.to_string()).unwrap();
    fs::write(&cursor, old_cursor.to_string()).unwrap();

    let merge = run(&pkg, &["init", "--merge", "--yes"], b"");
    assert!(
        merge.status.success(),
        "merge failed: {}",
        stderr_of(&merge)
    );

    for (path, hooked) in [
        (
            &claude,
            vec![
                "keel hook plan-check",
                "keel hook post-edit claude-code",
                "keel hook session-start",
            ],
        ),
        (&cursor, vec!["keel hook session-start"]),
    ] {
        let cmds = commands_of_file(path);
        for cmd in &cmds {
            assert!(
                !cmd.contains(".keel/hooks/") && !cmd.contains("keel map --llm --cached"),
                "old string left in {}: {cmds:?}",
                path.display()
            );
        }
        for want in hooked {
            let n = cmds.iter().filter(|c| *c == want).count();
            assert_eq!(n, 1, "{want:?} x{n} in {}: {cmds:?}", path.display());
        }
    }
}

#[test]
fn hook_with_an_unreadable_local_hooks_dir_fails_instead_of_using_main() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap().join("main");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("readme.txt"), "x\n").unwrap();
    git(&root, &["add", "readme.txt"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    write_script(
        &root.join(".keel/hooks/post-edit.sh"),
        "#!/bin/sh\necho from=main-checkout\n",
    );
    let wt = dir.path().canonicalize().unwrap().join("linked");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    let hooks = wt.join(".keel/hooks");
    write_script(
        &hooks.join("post-edit.sh"),
        "#!/bin/sh\necho from=worktree\n",
    );
    fs::set_permissions(&hooks, fs::Permissions::from_mode(0o000)).unwrap();
    // A root process reads the directory anyway, so the premise would not hold.
    let readable = fs::read_dir(&hooks).is_ok();

    let out = run(&wt, &["hook", "post-edit"], b"");

    fs::set_permissions(&hooks, fs::Permissions::from_mode(0o755)).unwrap();
    if readable {
        return;
    }
    assert_eq!(out.status.code(), Some(1), "{}", stderr_of(&out));
    assert!(!stdout_of(&out).contains("from=main-checkout"));
}

// The tests below pin where `keel hook post-edit` checks a file that lies in
// another checkout than the session's cwd.

/// Whether `tool` is on `PATH`; the real `post-edit.sh` needs `bash` and `jq`.
/// Mirrors `tests/hook_command.rs`.
fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const UNTYPED: &str = "def widget(x): return x\n";
const TYPED: &str = "def gadget(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n";

/// A stub hook that must never run: it leaves `marker` behind and exits 9.
fn marker_stub(marker: &Path) -> String {
    format!("#!/bin/sh\ntouch '{}'\nexit 9\n", marker.display())
}

/// The payload an editor sends for an edit of `file`.
fn payload(file: &Path) -> String {
    json!({"tool_input": {"file_path": file}}).to_string()
}

/// A python file `name` under `root/sub/deep`.
fn add_file(root: &Path, name: &str, body: &str) -> PathBuf {
    let file = root.join("sub/deep").join(name);
    fs::write(&file, body).unwrap();
    file
}

/// A git repository `<parent>/<name>` holding `.claude/` and one clean file,
/// with `keel init --yes` run. `parent` must be canonical; returns the root.
fn initialised_repo(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    fs::create_dir_all(root.join("sub/deep")).unwrap();
    git(&root, &["init", "-q"]);
    fs::create_dir_all(root.join(".claude")).unwrap();
    add_file(&root, "alpha.py", TYPED);
    let init = run(&root, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    root
}

/// `initialised_repo` carrying the REAL post-edit.sh, committed and mapped.
fn real_hook_repo(parent: &Path, name: &str) -> PathBuf {
    let root = initialised_repo(parent, name);
    // `--update-docs` re-installs an existing post-edit.sh: an empty one yields the real script.
    write_script(&root.join(".keel/hooks/post-edit.sh"), "");
    let update = run(&root, &["init", "--update-docs"], b"");
    assert!(
        update.status.success(),
        "update-docs failed: {}",
        stderr_of(&update)
    );
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let map = run(&root, &["map"], b"");
    assert!(map.status.success(), "map failed: {}", stderr_of(&map));
    root
}

/// `keel hook post-edit claude-code` in `cwd` with `stdin`.
fn post_edit(cwd: &Path, stdin: &[u8]) -> Output {
    run(cwd, &["hook", "post-edit", "claude-code"], stdin)
}

/// The notice `keel hook` adds when it checked a file in another checkout:
/// exactly one stderr line, after the script's output, naming `root`.
#[track_caller]
fn assert_checked_in(out: &Output, root: &Path) {
    let stderr = stderr_of(out);
    let notices: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("keel hook: checked in "))
        .collect();
    assert_eq!(notices.len(), 1, "one notice line; stderr={stderr:?}");
    assert!(
        notices[0].contains(&root.display().to_string()),
        "notice must name {}; got {:?}",
        root.display(),
        notices[0]
    );
    assert_eq!(
        stderr.trim_end().lines().last(),
        Some(notices[0]),
        "notice must follow the script's output; stderr={stderr:?}"
    );
}

/// The reporting stub ran in `root`, with the args and `stdin` intact, and
/// `keel hook` passed its exit code 7 through.
#[track_caller]
fn assert_stub_ran_in(out: &Output, root: &Path, stdin: &str) {
    assert_eq!(
        stdout_of(out),
        format!("cwd={}\nargs=[claude-code]\nstdin={stdin}", root.display()),
        "stub must run in {}; stderr={}",
        root.display(),
        stderr_of(out)
    );
    assert_eq!(out.status.code(), Some(7), "stderr={}", stderr_of(out));
}

/// Control for the real script: an untyped file edited from inside `root`
/// itself blocks with E002 and needs no notice, so a silent run from another
/// checkout cannot be a broken fixture.
#[track_caller]
fn assert_blocks_from_inside(root: &Path) {
    let file = add_file(root, "gizmo.py", UNTYPED);
    let out = post_edit(root, payload(&file).as_bytes());
    assert_eq!(out.status.code(), Some(2), "stderr={}", stderr_of(&out));
    assert!(stderr_of(&out).contains("E002"), "{}", stderr_of(&out));
    assert!(!stderr_of(&out).contains("keel hook: checked in"));
}

#[test]
fn hook_post_edit_checks_a_file_in_another_initialised_repository() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = real_hook_repo(&parent, "a");
    let b = real_hook_repo(&parent, "b");
    assert_blocks_from_inside(&b);

    let bad = add_file(&b, "widget.py", UNTYPED);
    let out = post_edit(&a, payload(&bad).as_bytes());

    assert_eq!(
        (out.status.code(), stderr_of(&out).contains("E002")),
        (Some(2), true),
        "an untyped function in another repository must block; stdout={:?} stderr={:?}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_checked_in(&out, &b);
}

#[test]
fn hook_post_edit_checks_a_file_in_a_linked_worktree_from_the_main_checkout() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = real_hook_repo(&parent, "a");
    let linked = parent.join("linked");
    git(
        &a,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ],
    );
    // A linked worktree has its own graph.
    let map = run(&linked, &["map"], b"");
    assert!(map.status.success(), "map failed: {}", stderr_of(&map));
    assert_blocks_from_inside(&linked);

    let bad = add_file(&linked, "widget.py", UNTYPED);
    let out = post_edit(&a, payload(&bad).as_bytes());

    assert_eq!(
        (out.status.code(), stderr_of(&out).contains("E002")),
        (Some(2), true),
        "an untyped function in a linked worktree must block; stdout={:?} stderr={:?}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_checked_in(&out, &linked);
}

#[test]
fn hook_post_edit_runs_the_sessions_script_in_the_files_checkout_and_never_the_other_checkouts_script(
) {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = initialised_repo(&parent, "a");
    let b = initialised_repo(&parent, "b");
    let marker = b.join("ran-in-b.marker");
    write_script(&a.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);
    write_script(&b.join(".keel/hooks/post-edit.sh"), &marker_stub(&marker));
    let file = b.join("sub/deep/alpha.py");
    // Valid JSON with whitespace around it: a re-serialised payload would differ.
    let stdin = format!(" \n{}\n\n", payload(&file));

    let out = post_edit(&a, stdin.as_bytes());

    assert_stub_ran_in(&out, &b, &stdin);
    assert!(
        !marker.exists(),
        "the other checkout's own script must never run"
    );
    assert_checked_in(&out, &b);
}

#[test]
fn hook_post_edit_stays_in_the_cwd_checkout_for_a_file_in_a_directory_keel_does_not_know() {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = initialised_repo(&parent, "a");
    write_script(&a.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);
    let plain = parent.join("plain");
    fs::create_dir_all(&plain).unwrap();
    git(&plain, &["init", "-q"]);
    let file = plain.join("alpha.py");
    fs::write(&file, TYPED).unwrap();
    let stdin = payload(&file);

    let out = post_edit(&a, stdin.as_bytes());

    assert_stub_ran_in(&out, &a, &stdin);
    assert!(
        !stderr_of(&out).contains("keel hook: checked in"),
        "no notice without a re-route; stderr={:?}",
        stderr_of(&out)
    );
}

#[test]
fn hook_plan_check_is_never_rerouted_by_a_file_path() {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = initialised_repo(&parent, "a");
    let b = initialised_repo(&parent, "b");
    write_script(&a.join(".keel/hooks/plan-check.sh"), REPORTING_STUB);
    let stdin = payload(&b.join("sub/deep/alpha.py"));

    let out = run(&a, &["hook", "plan-check", "claude-code"], stdin.as_bytes());

    assert_stub_ran_in(&out, &a, &stdin);
    assert!(
        !stderr_of(&out).contains("keel hook: checked in"),
        "no notice without a re-route; stderr={:?}",
        stderr_of(&out)
    );
}

#[test]
fn hook_post_edit_with_a_relative_or_unparsable_payload_stays_in_the_cwd_checkout() {
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = initialised_repo(&parent, "a");
    let _b = initialised_repo(&parent, "b");
    write_script(&a.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);

    for stdin in [
        r#"{"tool_input":{"file_path":"sub/alpha.py"}}"#,
        // Relative, but it names a file of the initialised repository `b` from `a`.
        r#"{"tool_input":{"file_path":"../b/sub/deep/alpha.py"}}"#,
        "not json at all",
    ] {
        let out = post_edit(&a, stdin.as_bytes());

        assert_stub_ran_in(&out, &a, stdin);
        assert!(
            !stderr_of(&out).contains("keel hook: checked in"),
            "no notice without a re-route for {stdin:?}; stderr={:?}",
            stderr_of(&out)
        );
    }
}

#[test]
fn hook_post_edit_is_silent_for_a_clean_file_in_another_repository() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let dir = TempDir::new().unwrap();
    let parent = dir.path().canonicalize().unwrap();
    let a = real_hook_repo(&parent, "a");
    let b = real_hook_repo(&parent, "b");
    let clean = add_file(&b, "gadget.py", TYPED);

    let out = post_edit(&a, payload(&clean).as_bytes());

    assert_eq!(
        (out.status.code(), stdout_of(&out), stderr_of(&out)),
        (Some(0), String::new(), String::new()),
        "a clean file must exit 0 with nothing on stdout or stderr"
    );
}
