//! `keel hook` on Windows. The hook scripts are shell scripts, so the command
//! must hand them to a POSIX shell on `PATH` that honours their shebang — and
//! never to the WSL launcher, which Windows resolves for a bare `bash` before
//! it looks at `PATH`.
#![cfg(windows)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.email=test@keel.dev",
            "-c",
            "user.name=keel test",
        ])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository in `<tmp>/demo` with one commit.
fn repo(tmp: &Path) -> std::path::PathBuf {
    repo_named(tmp, "demo")
}

/// A repository in `<tmp>/<name>` with one commit.
fn repo_named(tmp: &Path, name: &str) -> std::path::PathBuf {
    let root = tmp.join(name);
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["commit", "-q", "--allow-empty", "-m", "init"]);
    root
}

/// A bash hook that reports its label, the directory it ran in, whether bash
/// runs it as bash (not in the POSIX mode of `sh <script>`), its arguments
/// and its stdin, then exits `code`.
fn write_stub(dir: &Path, label: &str, code: u8) {
    let hooks = dir.join(".keel").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    let body = format!(
        "#!/bin/bash\n\
         if shopt -qo posix; then mode=posix; else mode=bash; fi\n\
         printf '{label} dir=%s mode=%s args=' \"$(basename \"$(pwd)\")\" \"$mode\"\n\
         for a in \"$@\"; do printf '[%s]' \"$a\"; done\n\
         printf ' stdin='; cat\n\
         exit {code}\n"
    );
    fs::write(hooks.join("post-edit.sh"), body).unwrap();
}

fn hook(cwd: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keel"))
        .arg("hook")
        .arg("post-edit")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("keel runs");
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

#[track_caller]
fn assert_ran(out: &Output, code: i32, stdout: &str) {
    let got = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(got, stdout, "stdout; stderr: {err}");
    assert_eq!(out.status.code(), Some(code), "exit code; stderr: {err}");
}

#[test]
fn hook_runs_the_root_script_from_a_nested_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo(tmp.path());
    write_stub(&root, "root", 7);
    let nested = root.join("other").join("deep");
    fs::create_dir_all(&nested).unwrap();

    let out = hook(&nested, &["alpha", "two words"], b"payload");

    assert_ran(
        &out,
        7,
        "root dir=demo mode=bash args=[alpha][two words] stdin=payload",
    );
}

#[test]
fn hook_runs_the_nearest_script_in_its_own_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo(tmp.path());
    write_stub(&root, "root", 7);
    write_stub(&root.join("pkg"), "pkg", 3);
    let nested = root.join("pkg").join("sub");
    fs::create_dir_all(&nested).unwrap();

    let out = hook(&nested, &[], b"");

    assert_ran(&out, 3, "pkg dir=pkg mode=bash args= stdin=");
}

#[test]
fn hook_in_a_linked_worktree_falls_back_to_the_main_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo(tmp.path());
    write_stub(&root, "root", 7);
    let linked = tmp.path().join("linked");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().unwrap(),
        ],
    );

    let out = hook(&linked, &["x"], b"");

    assert_ran(&out, 7, "root dir=linked mode=bash args=[x] stdin=");
}

#[test]
fn hook_in_a_linked_worktree_prefers_its_own_script() {
    let tmp = tempfile::tempdir().unwrap();
    let root = repo(tmp.path());
    write_stub(&root, "root", 7);
    let linked = tmp.path().join("linked");
    git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().unwrap(),
        ],
    );
    write_stub(&linked, "own", 5);

    let out = hook(&linked, &[], b"");

    assert_ran(&out, 5, "own dir=linked mode=bash args= stdin=");
}

#[test]
fn hook_post_edit_runs_the_sessions_script_in_another_initialised_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let a = repo(tmp.path());
    let b = repo_named(tmp.path(), "bravo");
    write_stub(&a, "a", 7);
    write_stub(&b, "b", 9);
    let file = b.join("alpha.py");
    fs::write(&file, "x = 1\n").unwrap();
    let payload = serde_json::json!({"tool_input": {"file_path": file}}).to_string();

    let out = hook(&a, &["claude-code"], payload.as_bytes());

    assert_ran(
        &out,
        7,
        &format!("a dir=bravo mode=bash args=[claude-code] stdin={payload}"),
    );
}
