// Tests for `keel audit` output shape (T1.5 — an audit a human will read).

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use crate::common::keel_bin;

/// A mapped project with an oversized CLAUDE.md and 30 all-public Rust files —
/// enough per-file smells to bury the one finding that matters.
fn noisy_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("CLAUDE.md"), "line\n".repeat(400)).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("tests")).unwrap();

    for i in 0..30 {
        let mut src = String::new();
        for f in 0..8 {
            src.push_str(&format!(
                "pub fn helper_{i}_{f}(x: i32) -> i32 {{ x + {f} }}\n"
            ));
        }
        fs::write(dir.path().join(format!("src/mod_{i}.rs")), &src).unwrap();
        // Same shape under tests/ — must never be graded.
        fs::write(dir.path().join(format!("tests/mod_{i}_test.rs")), &src).unwrap();
    }

    let keel = keel_bin();
    for args in [vec!["init"], vec!["map"]] {
        let out = Command::new(&keel)
            .args(&args)
            .current_dir(dir.path())
            .output()
            .expect("failed to run keel");
        assert!(
            out.status.success(),
            "keel {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    dir
}

fn audit(dir: &TempDir, extra: &[&str]) -> String {
    let mut args = vec!["audit", "--llm"];
    args.extend_from_slice(extra);
    let out = Command::new(keel_bin())
        .args(&args)
        .current_dir(dir.path())
        .output()
        .expect("failed to run keel audit");
    assert_ne!(
        out.status.code(),
        Some(2),
        "audit hit an internal error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
/// The scan-order wall is gone: output is capped, says what it left out, and
/// never repeats a finding.
fn test_audit_llm_is_capped_and_deduplicated() {
    let dir = noisy_project();
    let out = audit(&dir, &["--max-tokens", "100000"]);

    let findings: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("FAIL:") || l.starts_with("WARN:") || l.starts_with("TIP:"))
        .collect();
    assert!(
        findings.len() <= 20,
        "default --llm output must cap at 20 findings, got {}",
        findings.len()
    );

    let mut unique = findings.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        findings.len(),
        unique.len(),
        "audit output contains duplicate lines:\n{out}"
    );
}

#[test]
/// The single highest-leverage finding in a repo (a 400-line CLAUDE.md) must
/// not be buried under per-file smells.
fn test_agent_config_finding_is_ranked_first() {
    let dir = noisy_project();
    let out = audit(&dir, &["--max-tokens", "100000"]);
    let first = out
        .lines()
        .find(|l| l.starts_with("FAIL:") || l.starts_with("WARN:"))
        .unwrap_or_default();
    assert!(
        first.contains("agent_instructions_size"),
        "expected the CLAUDE.md finding first, got: {first}\n\nfull output:\n{out}"
    );
}

#[test]
/// Test files are not maintainability defects: no finding may name one.
fn test_test_files_are_not_graded() {
    let dir = noisy_project();
    let out = audit(&dir, &["--max-tokens", "100000", "--top", "0"]);
    for check in ["public_ratio", "god_file", "function_size", "cryptic_name"] {
        for line in out.lines().filter(|l| l.contains(check)) {
            assert!(
                !line.contains("tests/"),
                "{check} must not be reported on a test file: {line}"
            );
        }
    }
}

#[test]
/// `--top 0` lifts the cap for the reader who wants everything.
fn test_top_zero_shows_more_than_the_default_cap() {
    let dir = noisy_project();
    let capped = audit(&dir, &["--max-tokens", "100000"]);
    let full = audit(&dir, &["--max-tokens", "100000", "--top", "0"]);
    assert!(
        full.lines().count() >= capped.lines().count(),
        "--top 0 must not show less than the capped view"
    );
    assert!(!full.contains("omitted"), "--top 0 omits nothing: {full}");
}

#[test]
/// `--strict-cycles` is accepted and does not change the exit contract.
fn test_strict_cycles_flag_runs() {
    let dir = noisy_project();
    let out = audit(&dir, &["--strict-cycles"]);
    assert!(out.starts_with("audit:"), "unexpected output: {out}");
}

#[test]
/// A one-line delegating wrapper with no callers is a `trivial_wrapper`
/// finding (#60).
fn test_trivial_wrapper_smell_detected() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/wrap.rs"),
        "pub fn helper(x: i32) -> i32 {\n    x + 1\n}\n\npub fn wrapper(x: i32) -> i32 {\n    helper(x)\n}\n",
    )
    .unwrap();

    let keel = keel_bin();
    for args in [vec!["init"], vec!["map"]] {
        let out = Command::new(&keel)
            .args(&args)
            .current_dir(dir.path())
            .output()
            .expect("failed to run keel");
        assert!(
            out.status.success(),
            "keel {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let out = audit(&dir, &["--max-tokens", "100000", "--top", "0"]);
    assert!(
        out.contains("trivial_wrapper"),
        "expected a trivial_wrapper finding, got:\n{out}"
    );
}
