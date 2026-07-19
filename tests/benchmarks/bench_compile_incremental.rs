// Benchmark tests for incremental compile performance
// Uses CLI binary to measure single-file compile speed.
//
// Performance contract (CLAUDE.md "Key Commands" table): `keel compile [file]`
// incremental validate target is <200ms for a single file. The unconditional
// asserts below stay loose because debug builds run ~15x slower; the
// `#[cfg(not(debug_assertions))]` asserts enforce the real 200ms contract when
// the suite is built `--release`, so a genuine regression can no longer pass.

use super::common;

use std::fs;
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    common::keel_bin()
}

fn setup_mapped_project(files: &[(&str, &str)]) -> TempDir {
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
/// Single TypeScript file compile benchmark.
fn bench_compile_single_typescript_file_under_200ms() {
    let dir = setup_mapped_project(&[(
        "src/target.ts",
        "export function target(x: number): number { return x + 1; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", "src/target.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0 || code == 1, "compile failed with {code}");
    // Debug mode: allow 3s (release target: 200ms)
    assert!(elapsed.as_millis() < 3000, "compile took {:?}", elapsed);
    // Release: enforce the documented <200ms single-file compile contract.
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 200,
        "release single-file compile took {:?} — exceeds documented 200ms",
        elapsed
    );
}

#[test]
/// Single Python file compile benchmark.
fn bench_compile_single_python_file_under_200ms() {
    let dir = setup_mapped_project(&[(
        "src/target.py",
        "def target(x: int) -> int:\n    return x + 1\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", "src/target.py"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0 || code == 1, "compile failed with {code}");
    assert!(elapsed.as_millis() < 3000, "compile took {:?}", elapsed);
    // Release: enforce the documented <200ms single-file compile contract.
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 200,
        "release single-file compile took {:?} — exceeds documented 200ms",
        elapsed
    );
}

#[test]
/// Compile a file with many callers.
fn bench_compile_file_with_many_callers() {
    let files: Vec<(&str, &str)> = vec![(
        "src/util.ts",
        "export function util(x: number): number { return x; }\n",
    )];

    // Create caller files that reference util (static allocations)
    let caller_contents: Vec<String> = (0..20)
        .map(|i| {
            format!(
                "import {{ util }} from './util';\nexport function caller_{i}(): number {{ return util({i}); }}\n"
            )
        })
        .collect();

    let caller_files: Vec<(String, &str)> = caller_contents
        .iter()
        .enumerate()
        .map(|(i, c)| (format!("src/caller_{i}.ts"), c.as_str()))
        .collect();

    // Need to convert to &str pairs — use a Vec of owned tuples
    let all_files: Vec<(String, String)> = {
        let mut v = vec![(
            "src/util.ts".to_string(),
            "export function util(x: number): number { return x; }\n".to_string(),
        )];
        for (i, content) in caller_contents.iter().enumerate() {
            v.push((format!("src/caller_{i}.ts"), content.clone()));
        }
        v
    };

    let dir = TempDir::new().unwrap();
    for (path, content) in &all_files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    let keel = keel_bin();
    Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Modify the utility function
    fs::write(
        dir.path().join("src/util.ts"),
        "export function util(x: number, y: number): number { return x + y; }\n",
    )
    .unwrap();

    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", "src/util.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0 || code == 1, "compile failed with {code}");
    assert!(
        elapsed.as_secs() < 5,
        "compile with callers took {:?}",
        elapsed
    );
    // Release: still a single-file compile, so the documented 200ms applies.
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 200,
        "release single-file compile (many callers) took {:?} — exceeds documented 200ms",
        elapsed
    );

    let _ = (files, caller_files); // suppress unused warnings
}

#[test]
/// Compile a file with no changes — fast-path.
fn bench_compile_file_with_no_violations() {
    let dir = setup_mapped_project(&[(
        "src/clean.ts",
        "/** Clean. */\nexport function clean(x: number): number { return x; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let output = Command::new(&keel)
        .args(["compile", "src/clean.ts"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0 || code == 1, "compile failed with {code}");
    assert!(
        elapsed.as_secs() < 3,
        "no-change compile took {:?}",
        elapsed
    );
    // Release: enforce the documented <200ms single-file compile contract.
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 200,
        "release no-change compile took {:?} — exceeds documented 200ms",
        elapsed
    );
}
