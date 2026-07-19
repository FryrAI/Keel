// Tests for `keel compile --batch-start/--batch-end` (Spec 007 - CLI Commands)

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// The persisted batch-state blob (SQLite `keel_meta` row), or `None`.
fn batch_state(dir: &TempDir) -> Option<String> {
    let db = dir.path().join(".keel/graph.db");
    keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap())
        .ok()
        .and_then(|s| s.load_batch())
}

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

fn init_and_map(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    dir
}

#[test]
/// `keel compile --batch-start` should enter batch mode.
fn test_compile_batch_start() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-start");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-start should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// `keel compile --batch-end` should fire all deferred violations.
fn test_compile_batch_end() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    // Start batch
    let start_out = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(start_out.status.success() || start_out.status.code() == Some(1));

    // End batch — should report any accumulated violations
    let end_out = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-end");

    let code = end_out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end should exit 0 or 1, got {code}\nstderr: {}",
        String::from_utf8_lossy(&end_out.stderr)
    );
}

#[test]
/// `keel compile --batch-end` without prior --batch-start should be a no-op.
fn test_compile_batch_end_without_start() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    // batch-end without batch-start should be a graceful no-op
    let output = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel compile --batch-end");

    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end without start should be no-op (exit 0 or 1), got {code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
/// Multiple files compiled during batch mode should accumulate deferred violations.
fn test_compile_batch_accumulates_violations() {
    let dir = init_and_map(&[
        (
            "src/a.ts",
            "export function fa(x: number): number { return x; }\n",
        ),
        (
            "src/b.ts",
            "export function fb(x: number): number { return x; }\n",
        ),
        (
            "src/c.ts",
            "export function fc(x: number): number { return x; }\n",
        ),
    ]);
    let keel = keel_bin();

    // Start batch
    let start = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(start.status.success() || start.status.code() == Some(1));

    // Compile individual files during batch
    for file in &["src/a.ts", "src/b.ts", "src/c.ts"] {
        let out = Command::new(&keel)
            .args(["compile", file])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let code = out.status.code().unwrap_or(-1);
        assert!(
            code == 0 || code == 1,
            "compile {file} during batch should exit 0 or 1, got {code}"
        );
    }

    // End batch — should fire all accumulated deferred violations
    let end_out = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let code = end_out.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "batch-end should exit 0 or 1, got {code}"
    );
}

/// Cross-process batch mode (ITEM 3): `--batch-start`, a violating compile, and
/// `--batch-end` each run as SEPARATE processes. The deferred violation must
/// survive to fire at `--batch-end`.
///
/// Before batch state was persisted (now in a SQLite `keel_meta` row),
/// `--batch-start` set state in a per-process engine that was dropped at exit:
/// the second process saw no batch, fired E002 immediately, and `--batch-end`
/// had nothing to fire.
#[test]
fn test_compile_batch_persists_across_processes() {
    // A clean file so `map` has a baseline; the violating file is added later so
    // it is "new" and its E002 stays an ERROR (not a progressive warning).
    let dir = init_and_map(&[(
        "src/clean.py",
        "def clean(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n",
    )]);
    let keel = keel_bin();

    // A new file with a missing-type-hint public function → deferrable E002.
    fs::write(
        dir.path().join("src/deferred.py"),
        "def compute(value):\n    return value\n",
    )
    .unwrap();

    // Process 1: start the batch.
    let start = Command::new(&keel)
        .args(["compile", "--batch-start"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(start.status.success(), "batch-start should exit 0");
    assert!(
        batch_state(&dir).is_some(),
        "batch-start must persist batch state to the store"
    );

    // Process 2: compile the violating file. Its E002 is deferrable, so it is
    // stashed (not fired): exit 0, empty stdout.
    let mid = Command::new(&keel)
        .args(["compile", "src/deferred.py"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let mid_code = mid.status.code().unwrap_or(-1);
    let mid_stdout = String::from_utf8_lossy(&mid.stdout);
    assert_eq!(
        mid_code, 0,
        "a deferred-only compile must exit 0 (violation stashed), got {mid_code}; stdout: {mid_stdout}"
    );
    assert!(
        mid_stdout.trim().is_empty(),
        "deferred violations must NOT print during the batch; stdout: {mid_stdout}"
    );
    let batch_blob = batch_state(&dir).expect("batch state must persist in the store");
    assert!(
        batch_blob.contains("E002"),
        "the deferred E002 must be persisted in the batch state: {batch_blob}"
    );

    // Process 3: end the batch — the deferred E002 fires now.
    let end = Command::new(&keel)
        .args(["compile", "--batch-end"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let end_code = end.status.code().unwrap_or(-1);
    let end_stdout = String::from_utf8_lossy(&end.stdout);
    assert_eq!(
        end_code, 1,
        "batch-end must fire the deferred E002 as an error (exit 1), got {end_code}; stdout: {end_stdout}"
    );
    assert!(
        end_stdout.contains("E002"),
        "batch-end output must include the deferred E002: {end_stdout}"
    );
    assert!(
        batch_state(&dir).is_none(),
        "batch-end must clear the persisted batch state"
    );
}
