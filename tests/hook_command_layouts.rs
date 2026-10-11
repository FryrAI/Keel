//! `keel hook` script lookup in nested project layouts, `init --merge`
//! migration, and unreadable hook directories.
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

const REPORTING_STUB: &str = "#!/bin/sh\n\
    printf 'cwd=%s\\n' \"$(pwd -P)\"\n\
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
