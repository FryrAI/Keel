// Tests for T1.6: version-stamped docs, drift detection (`map`/`compile`),
// and `keel init --update-docs`.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
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

fn setup_initialized_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "export function hello(name: string): string { return name; }\n",
    )
    .unwrap();

    let out = Command::new(keel_bin())
        .args(["init", "--yes"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// Rewrite `.keel/keel.json`'s pinned `version` field, simulating an install
/// that predates the current binary.
fn downgrade_pinned_version(dir: &std::path::Path, version: &str) {
    let config_path = dir.join(".keel/keel.json");
    let content = fs::read_to_string(&config_path).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&content).unwrap();
    json["version"] = serde_json::json!(version);
    fs::write(&config_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
}

#[test]
fn compile_reports_stale_keel_json_exactly_once_and_modifies_nothing() {
    let dir = setup_initialized_project();
    downgrade_pinned_version(dir.path(), "0.0.1");

    let config_path = dir.path().join(".keel/keel.json");
    let agents_path = dir.path().join("AGENTS.md");
    let config_before = fs::read_to_string(&config_path).unwrap();
    let agents_before = fs::read_to_string(&agents_path).unwrap();

    let out = Command::new(keel_bin())
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);

    let drift_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("run keel init --update-docs"))
        .collect();
    assert_eq!(
        drift_lines.len(),
        1,
        "expected exactly one drift line, got stderr: {stderr}"
    );
    assert!(
        drift_lines[0].contains(".keel/keel.json records 0.0.1"),
        "drift line must name the recorded version: {}",
        drift_lines[0]
    );

    // Detection must never rewrite anything.
    assert_eq!(fs::read_to_string(&config_path).unwrap(), config_before);
    assert_eq!(fs::read_to_string(&agents_path).unwrap(), agents_before);
}

#[test]
fn map_reports_stale_keel_json_exactly_once() {
    let dir = setup_initialized_project();
    downgrade_pinned_version(dir.path(), "0.0.1");

    let out = Command::new(keel_bin())
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let drift_lines = stderr
        .lines()
        .filter(|l| l.contains("run keel init --update-docs"))
        .count();
    assert_eq!(drift_lines, 1, "expected exactly one drift line: {stderr}");
}

#[test]
fn matching_version_is_silent() {
    let dir = setup_initialized_project();
    // Freshly initialized by this build — keel.json and the AGENTS.md stamp
    // both already match the running binary.
    let out = Command::new(keel_bin())
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("run keel init --update-docs"),
        "no drift line expected on a freshly-initialized project: {stderr}"
    );
}

#[test]
fn update_docs_rewrites_the_block_and_syncs_the_version() {
    let dir = setup_initialized_project();
    downgrade_pinned_version(dir.path(), "0.0.1");

    let out = Command::new(keel_bin())
        .args(["init", "--update-docs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init --update-docs failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = dir.path().join(".keel/keel.json");
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let binary_version = json["version"].as_str().unwrap().to_string();
    assert_ne!(binary_version, "0.0.1", "version must have been synced");

    let agents_md = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(
        agents_md.contains(&format!("<!-- keel:version {binary_version} -->")),
        "AGENTS.md must carry the current version stamp: {agents_md}"
    );
    assert!(agents_md.contains("W005"), "AGENTS.md must list W005-W007");
    assert!(agents_md.contains("W006"));
    assert!(agents_md.contains("W007"));
    assert!(
        agents_md.contains("keel skeleton") && agents_md.contains("keel checkpoint"),
        "AGENTS.md must list the v0.5 command set"
    );

    // Detection must now be silent.
    let compile_out = Command::new(keel_bin())
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let compile_stderr = String::from_utf8_lossy(&compile_out.stderr);
    assert!(
        !compile_stderr.contains("run keel init --update-docs"),
        "drift should be resolved after --update-docs: {compile_stderr}"
    );
}

#[test]
fn update_docs_hook_file_contains_client_and_5s_timeout() {
    let dir = setup_initialized_project();

    let out = Command::new(keel_bin())
        .args(["init", "--update-docs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let hook = fs::read_to_string(dir.path().join(".keel/hooks/post-edit.sh")).unwrap();
    assert!(hook.contains("--client"), "hook must pass --client: {hook}");
    assert!(
        hook.contains("timeout 5 "),
        "hook must use the 5s timeout: {hook}"
    );
    assert!(!hook.contains("timeout 15"), "old 15s timeout must be gone");
}

#[test]
fn update_docs_without_init_fails_cleanly() {
    let dir = TempDir::new().unwrap();
    let out = Command::new(keel_bin())
        .args(["init", "--update-docs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.to_lowercase().contains("not initialized"));
}
