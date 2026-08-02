// T2.6 — the Claude Code `ExitPlanMode` advisory hook installed by `keel init`.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

use tempfile::TempDir;

use crate::common::keel_bin;

/// `PATH` with the freshly built `keel` in front, so the hook's `keel` is ours.
fn path_with_keel() -> String {
    let bin_dir = keel_bin().parent().unwrap().to_path_buf();
    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bin_dir.display(), existing)
}

fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A mapped repo with `.claude/` present, so `keel init` detects Claude Code.
fn claude_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("lib.ts"),
        "export function foo(x: number): number {\n  return x + 1;\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("main.ts"),
        "import { foo } from './lib';\nexport function run(): number {\n  return foo(1);\n}\n",
    )
    .unwrap();

    let keel = keel_bin();
    let init = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let map = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        map.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    dir
}

/// Run the installed hook with a Claude Code `ExitPlanMode` payload.
fn run_hook(dir: &std::path::Path, plan: &str, env: &[(&str, &str)]) -> std::process::Output {
    let payload = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "ExitPlanMode",
        "tool_input": { "plan": plan },
    })
    .to_string();

    let mut cmd = Command::new("bash");
    cmd.arg(".keel/hooks/plan-check.sh")
        .current_dir(dir)
        .env("PATH", path_with_keel())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("failed to spawn hook");
    // A bypassed hook (KEEL_PLAN_HOOK=0) exits before reading stdin, so the
    // write races the child's exit — EPIPE here is expected, not a failure.
    let _ = child.stdin.take().unwrap().write_all(payload.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn test_init_installs_the_exit_plan_mode_hook() {
    let dir = claude_fixture();

    let script = dir.path().join(".keel/hooks/plan-check.sh");
    assert!(script.exists(), "plan-check.sh must be installed");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "hook must be executable");
    }

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let entry = settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse hook must be scaffolded")
        .iter()
        .find(|e| e["matcher"] == "ExitPlanMode")
        .expect("matcher must be ExitPlanMode");
    assert_eq!(entry["hooks"][0]["command"], ".keel/hooks/plan-check.sh");

    // Claude Code only — no sibling scaffolding for other tools.
    for other in [".cursor/hooks.json", ".gemini/settings.json"] {
        assert!(
            !dir.path().join(other).exists(),
            "{other} must not be created"
        );
    }
}

#[test]
fn test_hook_surfaces_findings_on_stderr_without_blocking() {
    if !have("jq") || !have("bash") {
        eprintln!("skipping: jq/bash not available");
        return;
    }
    let dir = claude_fixture();

    let out = run_hook(dir.path(), "1. In run, call foo(a, b) and log it.", &[]);
    assert_eq!(out.status.code(), Some(0), "advisory hook must never block");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("P002"), "P002 not surfaced: {stderr}");
    assert!(
        stderr.contains("advisory"),
        "must say it is advisory: {stderr}"
    );

    // A correct plan is silent.
    let out = run_hook(dir.path(), "1. In run, call foo(a) and log it.", &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).trim().is_empty(),
        "a clean plan must produce no output"
    );
}

#[test]
fn test_hook_bypass_and_opt_in_blocking() {
    if !have("jq") || !have("bash") {
        eprintln!("skipping: jq/bash not available");
        return;
    }
    let dir = claude_fixture();
    let bad = "1. In run, call foo(a, b) and log it.";

    let out = run_hook(dir.path(), bad, &[("KEEL_PLAN_HOOK", "0")]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        out.stderr.is_empty(),
        "the one-line bypass must silence the hook entirely"
    );

    let out = run_hook(dir.path(), bad, &[("KEEL_PLAN_STRICT", "1")]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "opt-in strict must block: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Acceptance: the hook adds < 150ms to plan acceptance. Asserted against a
/// deliberately loose ceiling because this runs against an unoptimized debug
/// binary in CI; the measured number is printed so a regression is visible.
#[test]
fn test_hook_latency_for_a_trivial_plan() {
    if !have("jq") || !have("bash") {
        eprintln!("skipping: jq/bash not available");
        return;
    }
    let dir = claude_fixture();
    // Warm the page cache / db so the first-run cost is not attributed here.
    run_hook(dir.path(), "1. Update the README.", &[]);

    let mut best = u128::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        let out = run_hook(dir.path(), "1. Update the README.", &[]);
        best = best.min(start.elapsed().as_millis());
        assert_eq!(out.status.code(), Some(0));
    }
    eprintln!("plan-check hook: {best}ms (debug build)");
    assert!(
        best < 1000,
        "plan hook took {best}ms — well past the 150ms release-build budget"
    );
}
