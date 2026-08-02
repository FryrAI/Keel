// Tests for `keel validate-plan` — plan validation against the dependency graph.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    fallback
}

/// Mapped cross-file fixture: lib.ts::foo called by main.ts::run.
fn mapped_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
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
    assert!(Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap()
        .status
        .success());
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

fn keel(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(keel_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run keel")
}

#[test]
fn test_validate_plan_removal_is_high_risk() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "## Plan\n\n1. Remove foo since it is unused.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--llm"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "validate-plan should exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("VALIDATE-PLAN"), "missing header: {stdout}");
    assert!(
        stdout.contains("ACTION remove foo"),
        "missing action: {stdout}"
    );
    assert!(
        stdout.contains("risk=HIGH"),
        "removal with callers is HIGH: {stdout}"
    );
    assert!(
        stdout.contains("src/main.ts"),
        "caller in main.ts should be listed: {stdout}"
    );
    assert!(
        stdout.contains("order:"),
        "should suggest a callers-first order: {stdout}"
    );
}

#[test]
fn test_validate_plan_json_shape() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "Delete foo entirely.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["command"], "validate-plan");
    assert_eq!(json["unrecognized"], false);
    let actions = json["actions"].as_array().unwrap();
    assert_eq!(actions[0]["symbol"], "foo");
    assert_eq!(actions[0]["action"], "remove");
    assert_eq!(actions[0]["risk"], "HIGH");
    assert!(actions[0]["caller_count"].as_u64().unwrap() >= 1);
    assert!(actions[0]["callers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["file"].as_str().unwrap_or("").contains("main.ts")));
}

#[test]
fn test_validate_plan_nonsense_is_graceful() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "Buy groceries and water the plants.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md"]);
    assert_eq!(out.status.code(), Some(0), "nonsense plan must not error");
    let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
    assert!(
        stdout.contains("no graph-relevant actions detected"),
        "should report graceful no-detection: {stdout}"
    );
}

#[test]
fn test_validate_plan_stdin() {
    let dir = mapped_fixture();
    let mut child = Command::new(keel_bin())
        .args(["validate-plan", "-", "--llm"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn keel");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Rename foo to compute.\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ACTION rename foo"),
        "stdin plan not read: {stdout}"
    );
}

// --- T2.5: P001/P002 plan findings ---

#[test]
fn test_validate_plan_wrong_signature_is_p002() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "1. In run, call foo(a, b) and log the result.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--json"]);
    assert_eq!(out.status.code(), Some(0), "default must never fail");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let findings = json["findings"].as_array().expect("findings present");
    assert_eq!(findings.len(), 1, "expected exactly one P002: {findings:?}");
    assert_eq!(findings[0]["code"], "P002");
    assert_eq!(findings[0]["symbol"], "foo");
    assert_eq!(findings[0]["actual"], "foo(x: number) -> number");
    assert!(findings[0]["file"].as_str().unwrap().contains("lib.ts"));
    assert!(findings[0]["line"].as_u64().unwrap() >= 1);
    assert!(!findings[0]["hash"].as_str().unwrap().is_empty());
    assert!(!findings[0]["fix_hint"].as_str().unwrap().is_empty());
}

#[test]
fn test_validate_plan_strict_exits_1_only_with_findings() {
    let dir = mapped_fixture();
    let bad = dir.path().join("bad.md");
    fs::write(&bad, "1. In run, call foo(a, b) and log the result.\n").unwrap();
    let out = keel(
        dir.path(),
        &["validate-plan", "bad.md", "--strict", "--llm"],
    );
    assert_eq!(out.status.code(), Some(1), "--strict must gate on findings");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("P002"), "P002 line missing: {stdout}");

    let good = dir.path().join("good.md");
    fs::write(&good, "1. In run, call foo(a) and log the result.\n").unwrap();
    let out = keel(
        dir.path(),
        &["validate-plan", "good.md", "--strict", "--llm"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "a correct plan must pass --strict: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn test_validate_plan_unknown_call_target_is_p001() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(
        &plan,
        "1. In run, call computeTotals(rows) before returning foo(1).\n",
    )
    .unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let findings = json["findings"].as_array().expect("findings present");
    let p001: Vec<_> = findings.iter().filter(|f| f["code"] == "P001").collect();
    assert_eq!(p001.len(), 1, "{findings:?}");
    assert_eq!(p001[0]["symbol"], "computeTotals");
}

#[test]
fn test_validate_plan_correct_plan_keeps_the_old_report() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "Delete foo entirely.\n").unwrap();

    let out = keel(dir.path(), &["validate-plan", "plan.md", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(
        json.get("findings").is_none(),
        "no findings must serialize exactly as the pre-P-namespace report: {json}"
    );
    assert_eq!(json["actions"][0]["symbol"], "foo");
}

/// T2.6: three strikes on the same P-code auto-downgrade, so a stubborn claim
/// degrades to advice instead of deadlocking a session. The breaker counts fix
/// ATTEMPTS, so each strike must be a genuinely different claim.
#[test]
fn test_repeat_findings_downgrade_after_three_strikes() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");

    for (attempt, claim) in ["foo(a, b)", "foo(a, b, c)", "foo(a, b, c, d)"]
        .into_iter()
        .enumerate()
    {
        fs::write(&plan, format!("1. In run, call {claim} and log it.\n")).unwrap();
        let out = keel(
            dir.path(),
            &["validate-plan", "plan.md", "--strict", "--json"],
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
        let finding = &json["findings"][0];
        assert_eq!(finding["code"], "P002", "attempt {attempt}: {json}");
        if attempt < 2 {
            assert_eq!(
                out.status.code(),
                Some(1),
                "attempt {attempt} must still gate"
            );
            assert_eq!(finding["severity"], "WARNING");
            assert_eq!(finding["downgraded"], false);
        } else {
            assert_eq!(finding["downgraded"], true, "third strike must downgrade");
            assert_eq!(finding["severity"], "INFO");
            assert_eq!(
                out.status.code(),
                Some(0),
                "a downgraded finding must stop failing --strict"
            );
        }
    }
}

/// An identical re-submission is not a fix attempt: the counter must not move,
/// matching the compile-side circuit breaker's semantics.
#[test]
fn test_identical_replans_do_not_escalate() {
    let dir = mapped_fixture();
    let plan = dir.path().join("plan.md");
    fs::write(&plan, "1. In run, call foo(a, b) and log it.\n").unwrap();

    for _ in 0..4 {
        let out = keel(
            dir.path(),
            &["validate-plan", "plan.md", "--strict", "--json"],
        );
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
        assert_eq!(json["findings"][0]["downgraded"], false);
        assert_eq!(out.status.code(), Some(1));
    }
}
