//! `keel hook <name>` must run the generated hook scripts from any directory
//! inside a checkout, and the commands `keel init` writes into tool configs
//! must be plain argv that works from any working directory.
#![cfg(unix)]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use keel_core::graph_lock;
use serde_json::{json, Value};
use tempfile::TempDir;

use common::{git, keel_bin};

/// Run `keel <args>` in `cwd` with `stdin` piped in, and return the raw
/// `Output` whatever the exit code. `keel`'s own directory leads `PATH`, so a
/// hook script that calls `keel` finds the binary under test.
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

/// Whether `tool` is on `PATH`; the real `post-edit.sh` needs `bash` and `jq`.
fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write an executable script at `path`, creating parent directories.
fn write_script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    common::write_executable(path, body);
}

/// A stub hook that reports its cwd, its arguments and its stdin on stdout,
/// then exits 7.
const REPORTING_STUB: &str = "#!/bin/sh\n\
    printf 'cwd=%s\\n' \"$(pwd -P)\"\n\
    printf 'args='; for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'\n\
    printf 'stdin='; cat\n\
    exit 7\n";

/// A git repository (no commits yet) with a nested subdirectory `sub/deep`.
/// Returns the guard, the canonical root and the nested directory.
fn repo() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-q"]);
    let nested = root.join("sub/deep");
    fs::create_dir_all(&nested).unwrap();
    (dir, root, nested)
}

/// `repo()` with a mapped python project: `.claude/` present, `keel init --yes`
/// run, one clean file committed, `keel map` run.
fn mapped_repo() -> (TempDir, PathBuf, PathBuf) {
    let (dir, root, nested) = repo();
    fs::create_dir_all(root.join(".claude")).unwrap();
    fs::write(
        nested.join("alpha.py"),
        "def alpha(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n",
    )
    .unwrap();
    let init = run(&root, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let map = run(&root, &["map"], b"");
    assert!(map.status.success(), "map failed: {}", stderr_of(&map));
    (dir, root, nested)
}

/// Every string found under a `"command"` key anywhere in `value`.
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
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let value: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
    let mut found = Vec::new();
    commands_in(&value, &mut found);
    found
}

/// Every `*.json` file under `dir`, skipping `.git`.
fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().unwrap() != ".git" {
                json_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
}

/// An agent edits a file in a subdirectory and the editor runs the hook from
/// there. The user sees "not found" (or nothing) instead of the script running
/// at the repo root with the editor's arguments and payload.
#[test]
fn hook_runs_script_at_repo_root_with_args_stdin_and_exit_code_from_nested_cwd() {
    let (_dir, root, nested) = repo();
    write_script(&root.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);

    let payload = b"{\"k\":\"a b\"}\n\n  tail-without-newline";
    let out = run(
        &nested,
        &["hook", "post-edit", "claude-code", "second"],
        payload,
    );

    let stdout = stdout_of(&out);
    let (head, stdin) = stdout.split_once("\nstdin=").unwrap_or_else(|| {
        panic!(
            "stub did not run; stdout={stdout:?} stderr={}",
            stderr_of(&out)
        )
    });
    assert_eq!(
        head,
        format!("cwd={}\nargs=[claude-code][second]", root.display()),
        "stub must run at the repo root with the args after the name"
    );
    assert_eq!(
        stdin.as_bytes(),
        payload,
        "stdin must pass through byte-for-byte"
    );
    assert_eq!(
        out.status.code(),
        Some(7),
        "keel hook must exit with the script's code"
    );
}

/// The whole point: the real post-edit script, driven through `keel hook` from
/// a nested directory, still reports a violation (exit 2 with E002 on stderr)
/// and stays silent on a clean file. Today the editor's relative script path
/// fails from there and the agent never sees the violation.
#[test]
fn hook_post_edit_reports_violation_from_nested_cwd_and_is_silent_when_clean() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let (_dir, root, nested) = repo();
    fs::create_dir_all(root.join(".claude")).unwrap();
    fs::write(
        nested.join("alpha.py"),
        "def alpha(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n",
    )
    .unwrap();
    assert!(run(&root, &["init", "--yes"], b"").status.success());
    // `--update-docs` re-installs an existing post-edit.sh: an empty one yields the real script.
    write_script(&root.join(".keel/hooks/post-edit.sh"), "");
    assert!(run(&root, &["init", "--update-docs"], b"").status.success());
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    assert!(run(&root, &["map"], b"").status.success());

    let payload = |file: &Path| json!({"tool_input": {"file_path": file}}).to_string();

    let clean = nested.join("gadget.py");
    fs::write(
        &clean,
        "def gadget(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n",
    )
    .unwrap();
    let ok = run(
        &nested,
        &["hook", "post-edit", "claude-code"],
        payload(&clean).as_bytes(),
    );
    assert_eq!(
        (ok.status.code(), stderr_of(&ok).as_str()),
        (Some(0), ""),
        "control: a clean file must exit 0 with empty stderr"
    );

    let bad = nested.join("widget.py");
    fs::write(&bad, "def widget(x): return x\n").unwrap();
    let out = run(
        &nested,
        &["hook", "post-edit", "claude-code"],
        payload(&bad).as_bytes(),
    );
    assert!(
        stderr_of(&out).contains("E002"),
        "violation must reach stderr; got stderr={:?}",
        stderr_of(&out)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "violations must block with exit 2"
    );
}

/// A compile that skipped because another process holds the graph lock checked
/// nothing. The real script, driven by the real binary, must say so with exit 2
/// (not pass the edit as checked), and the command it names must check the edit
/// once the lock is free.
#[test]
fn hook_post_edit_says_an_edit_was_not_checked_while_the_graph_lock_is_held() {
    if !have("bash") || !have("jq") {
        eprintln!("skipping: bash/jq not available");
        return;
    }
    let (_dir, root, nested) = repo();
    fs::create_dir_all(root.join(".claude")).unwrap();
    fs::write(
        nested.join("alpha.py"),
        "def alpha(x: int) -> int:\n    \"\"\"Return x.\"\"\"\n    return x\n",
    )
    .unwrap();
    assert!(run(&root, &["init", "--yes"], b"").status.success());
    // `--update-docs` re-installs an existing post-edit.sh: an empty one yields the real script.
    write_script(&root.join(".keel/hooks/post-edit.sh"), "");
    assert!(run(&root, &["init", "--update-docs"], b"").status.success());
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    assert!(run(&root, &["map"], b"").status.success());

    let bad = nested.join("widget.py");
    fs::write(&bad, "def widget(x): return x\n").unwrap();
    let payload = json!({"tool_input": {"file_path": &bad}}).to_string();
    let hook = || {
        run(
            &nested,
            &["hook", "post-edit", "claude-code"],
            payload.as_bytes(),
        )
    };

    let held = graph_lock::try_acquire(&root.join(".keel")).unwrap();
    let skipped = hook();
    let stderr = stderr_of(&skipped);
    assert_eq!(
        skipped.status.code(),
        Some(2),
        "a skipped compile must not pass as checked; got stderr={stderr:?}"
    );
    assert!(
        stderr.contains("NOT checked"),
        "the message must say the file was not checked; got stderr={stderr:?}"
    );
    assert!(
        stderr.contains(&bad.display().to_string()),
        "the unchecked file must be named; got stderr={stderr:?}"
    );
    assert!(
        !stderr.contains("E002"),
        "nothing was checked, so no violation may be reported; got stderr={stderr:?}"
    );

    drop(held);
    let checked = hook();
    assert_eq!(checked.status.code(), Some(2), "violations must block");
    assert!(
        stderr_of(&checked).contains("E002"),
        "with the lock free the same call must check the edit; got stderr={:?}",
        stderr_of(&checked)
    );
}

/// A linked worktree with committed hooks must run its OWN script, with the
/// worktree as cwd, not the main checkout's.
#[test]
fn hook_in_linked_worktree_runs_the_worktrees_own_script() {
    let (dir, root, _nested) = repo();
    write_script(&root.join(".keel/hooks/post-edit.sh"), REPORTING_STUB);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
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
    let wt_nested = wt.join("sub/deep");
    fs::create_dir_all(&wt_nested).unwrap();

    let out = run(&wt_nested, &["hook", "post-edit", "x"], b"");

    let stdout = stdout_of(&out);
    assert!(
        stdout.starts_with(&format!("cwd={}\n", wt.display())),
        "script must run with the worktree root as cwd; stdout={stdout:?} stderr={}",
        stderr_of(&out)
    );
}

/// A linked worktree whose checkout lacks the script (it is untracked in the
/// main checkout) falls back to the main checkout's script.
#[test]
fn hook_in_linked_worktree_falls_back_to_main_checkouts_script() {
    let (dir, root, _nested) = repo();
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
    assert!(
        !wt.join(".keel/hooks/post-edit.sh").exists(),
        "premise: no own copy"
    );
    let wt_nested = wt.join("sub");
    fs::create_dir_all(&wt_nested).unwrap();

    let out = run(&wt_nested, &["hook", "post-edit", "x"], b"");

    assert!(
        stdout_of(&out).contains("from=main-checkout"),
        "must fall back to the main checkout's script; stdout={:?} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_eq!(out.status.code(), Some(0));
}

/// `keel hook <anything else>` must not execute arbitrary scripts under
/// `.keel/`: unknown names and path-like names are a usage error (exit 2).
#[test]
fn hook_refuses_unknown_names_and_path_separators() {
    let (_dir, root, nested) = repo();
    let marker = root.join("marker-known");
    let marker_nope = root.join("marker-nope");
    let marker_x = root.join("marker-x");
    // Control: the mechanism works for a known name, so absent markers below mean "refused".
    write_script(
        &root.join(".keel/hooks/post-edit.sh"),
        &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    );
    write_script(
        &root.join(".keel/hooks/nope.sh"),
        &format!("#!/bin/sh\ntouch '{}'\n", marker_nope.display()),
    );
    write_script(
        &root.join(".keel/x.sh"),
        &format!("#!/bin/sh\ntouch '{}'\n", marker_x.display()),
    );

    let known = run(&nested, &["hook", "post-edit", "x"], b"");
    assert!(
        marker.exists(),
        "control: known name must run its script; stderr={}",
        stderr_of(&known)
    );

    for (name, planted) in [("nope", &marker_nope), ("../x", &marker_x)] {
        let out = run(&nested, &["hook", name], b"");
        assert_eq!(
            out.status.code(),
            Some(2),
            "`keel hook {name}` must be a usage error"
        );
        assert!(
            !planted.exists(),
            "`keel hook {name}` must not run a script"
        );
    }
}

/// A known hook whose script is missing must say so on one stderr line and
/// exit 1, never exit 0 silently.
#[test]
fn hook_with_missing_script_exits_1_with_one_line_naming_it() {
    let (_dir, _root, nested) = repo();

    let out = run(&nested, &["hook", "post-edit", "x"], b"");

    assert_eq!(out.status.code(), Some(1), "missing script must exit 1");
    let stderr = stderr_of(&out);
    assert_eq!(
        stderr.trim_end().lines().count(),
        1,
        "one stderr line; got {stderr:?}"
    );
    assert!(
        stderr.contains("post-edit.sh"),
        "must name the script; got {stderr:?}"
    );
    assert!(stdout_of(&out).is_empty(), "stdout must stay empty");
}

/// After a fresh `keel init --yes`, every hook command in every generated JSON
/// config must be plain argv (`keel hook …`) that works from any directory; a
/// cwd-relative `.keel/hooks/...` path fails from a subdirectory.
#[test]
fn init_writes_only_cwd_independent_hook_commands() {
    let (_dir, root, _nested) = repo();
    // Directories `detect_tools` recognises, so every tool gets its config.
    for d in [
        ".claude",
        ".cursor",
        ".gemini",
        ".windsurf",
        ".letta",
        ".codex",
        ".agent",
        ".aider",
    ] {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    let init = run(&root, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));

    let hook_configs = [
        ".claude/settings.json",
        ".gemini/settings.json",
        ".letta/settings.json",
        ".cursor/hooks.json",
        ".windsurf/hooks.json",
    ];
    for rel in hook_configs {
        let cmds = commands_of_file(&root.join(rel));
        let hook_cmds = cmds.iter().filter(|c| c.starts_with("keel hook ")).count();
        assert!(
            hook_cmds >= 1,
            "{rel}: no `keel hook ` command found among {cmds:?}"
        );
    }

    let mut all = Vec::new();
    json_files(&root, &mut all);
    for rel in hook_configs {
        assert!(all.contains(&root.join(rel)), "walk must reach {rel}");
    }
    for file in all {
        for cmd in commands_of_file(&file) {
            let rel = file.strip_prefix(&root).unwrap().display();
            assert!(
                !cmd.starts_with(".keel/"),
                "{rel}: cwd-relative command {cmd:?}"
            );
            if cmd.contains("keel") {
                assert!(
                    cmd == "keel" || cmd.starts_with("keel hook "),
                    "{rel}: command {cmd:?} is neither the MCP entry nor `keel hook …`"
                );
            }
        }
    }

    let session = root.join(".keel/hooks/session-start.sh");
    let mode = fs::metadata(&session)
        .unwrap_or_else(|e| panic!("session-start.sh missing: {e}"))
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "session-start.sh must be executable, mode {mode:o}"
    );
}

/// A session-start hook that exits non-zero shows the agent nothing, and its
/// stderr never reaches the agent. From a subdirectory the map must still
/// arrive; when keel itself fails, keel's own error text must arrive on stdout
/// with exit 0.
#[test]
fn hook_session_start_prints_map_and_surfaces_keel_errors_on_stdout() {
    let (_dir, root, nested) = mapped_repo();

    let out = run(&nested, &["hook", "session-start"], b"");
    assert!(
        stdout_of(&out).contains("MAP nodes="),
        "map must reach stdout; stdout={:?} stderr={}",
        stdout_of(&out),
        stderr_of(&out)
    );
    assert_eq!(out.status.code(), Some(0));

    fs::write(
        root.join(".keel/graph.db"),
        b"garbage\x00\x01 not a database",
    )
    .unwrap();
    let broken = run(&nested, &["hook", "session-start"], b"");
    assert!(
        !stdout_of(&broken).trim().is_empty(),
        "keel's error must reach stdout; stderr={}",
        stderr_of(&broken)
    );
    assert_eq!(
        broken.status.code(),
        Some(0),
        "session-start must always exit 0"
    );
}

/// A project initialised by an older keel carries the old cwd-relative
/// commands. `keel init --update-docs` must rewrite exactly those, leave a
/// user's own hook byte-identical, and be idempotent.
#[test]
fn update_docs_migrates_old_generated_commands_and_leaves_user_hooks_alone() {
    let (_dir, root, _nested) = repo();
    fs::create_dir_all(root.join(".claude")).unwrap();
    fs::create_dir_all(root.join(".cursor")).unwrap();
    let init = run(&root, &["init", "--yes"], b"");
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));

    let user_hook = json!({"type": "command", "command": "./scripts/my-hook.sh"});
    let claude = root.join(".claude/settings.json");
    let cursor = root.join(".cursor/hooks.json");
    let old_claude = json!({"hooks": {
        "SessionStart": [{"hooks": [
            {"type": "command", "command": "timeout 10 keel map --llm --cached"}]}],
        "PreToolUse": [{"matcher": "ExitPlanMode", "hooks": [
            {"type": "command", "command": ".keel/hooks/plan-check.sh"}]}],
        "PostToolUse": [{"matcher": "Edit|MultiEdit|Write", "hooks": [
            {"type": "command", "command": ".keel/hooks/post-edit.sh claude-code"},
            user_hook.clone()]}]
    }});
    let old_cursor = json!({"hooks": {"SessionStart": [
        {"command": "timeout 10 keel map --llm --cached", "type": "command"}]}});
    fs::write(&claude, serde_json::to_string_pretty(&old_claude).unwrap()).unwrap();
    fs::write(&cursor, serde_json::to_string_pretty(&old_cursor).unwrap()).unwrap();

    let update = run(&root, &["init", "--update-docs"], b"");
    assert!(
        update.status.success(),
        "update-docs failed: {}",
        stderr_of(&update)
    );

    let mut claude_cmds = commands_of_file(&claude);
    claude_cmds.sort();
    assert_eq!(
        claude_cmds,
        [
            "./scripts/my-hook.sh",
            "keel hook plan-check",
            "keel hook post-edit claude-code",
            "keel hook session-start",
        ],
        "claude settings after migration"
    );
    let claude_json: Value = serde_json::from_str(&fs::read_to_string(&claude).unwrap()).unwrap();
    assert!(
        claude_json.to_string().contains(&user_hook.to_string()),
        "the user's hook object must survive unchanged: {claude_json}"
    );
    assert_eq!(commands_of_file(&cursor), ["keel hook session-start"]);
    assert!(
        root.join(".keel/hooks/session-start.sh").exists(),
        "the scripts the new commands need must be installed"
    );

    let (claude_bytes, cursor_bytes) = (fs::read(&claude).unwrap(), fs::read(&cursor).unwrap());
    let again = run(&root, &["init", "--update-docs"], b"");
    assert!(
        again.status.success(),
        "second update-docs failed: {}",
        stderr_of(&again)
    );
    assert_eq!(
        fs::read(&claude).unwrap(),
        claude_bytes,
        "second run changed claude settings"
    );
    assert_eq!(
        fs::read(&cursor).unwrap(),
        cursor_bytes,
        "second run changed cursor hooks"
    );
}
