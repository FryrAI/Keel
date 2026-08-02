use std::fs;

use tempfile::TempDir;

use super::*;

const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A minimal initialized project: `.keel/keel.json` pinned at a stale
/// version plus a stale `AGENTS.md` (no version stamp at all, as pre-T1.6
/// docs would be).
fn setup_stale_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    fs::create_dir_all(&keel_dir).unwrap();
    fs::write(
        keel_dir.join("keel.json"),
        r#"{"version": "0.1.0", "languages": ["rust"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("AGENTS.md"),
        "<!-- keel:start -->\nstale content, no version stamp\n<!-- keel:end -->\n",
    )
    .unwrap();
    dir
}

#[test]
fn errors_when_not_initialized() {
    let dir = TempDir::new().unwrap();
    let code = run(dir.path(), false);
    assert_eq!(code, 2);
    assert!(!dir.path().join("AGENTS.md").exists());
}

#[test]
fn rewrites_the_block_and_syncs_the_version() {
    let dir = setup_stale_project();

    let code = run(dir.path(), false);
    assert_eq!(code, 0);

    let agents_md = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(
        agents_md.contains(&format!("<!-- keel:version {BINARY_VERSION} -->")),
        "AGENTS.md must carry the current version stamp: {agents_md}"
    );
    assert!(!agents_md.contains("stale content"));

    let keel_json = fs::read_to_string(dir.path().join(".keel/keel.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&keel_json).unwrap();
    assert_eq!(parsed["version"], BINARY_VERSION);
    // Non-version fields must survive the sync untouched.
    assert_eq!(parsed["languages"][0], "rust");
}

#[test]
fn regenerates_the_hook_with_client_flag_and_5s_timeout() {
    let dir = setup_stale_project();

    run(dir.path(), false);

    let hook = fs::read_to_string(dir.path().join(".keel/hooks/post-edit.sh")).unwrap();
    assert!(hook.contains("--client"), "hook must pass --client: {hook}");
    assert!(
        hook.contains("timeout 5 "),
        "hook must use the T1.1 5s timeout, not the old 15s crutch: {hook}"
    );
    assert!(
        !hook.contains("timeout 15"),
        "hook must not still use the old 15s timeout: {hook}"
    );
}

#[test]
fn never_creates_a_doc_file_that_was_not_already_present() {
    let dir = setup_stale_project();
    // No CLAUDE.md in this project — only AGENTS.md.
    assert!(!dir.path().join("CLAUDE.md").exists());

    run(dir.path(), false);

    assert!(
        !dir.path().join("CLAUDE.md").exists(),
        "--update-docs must not create integrations that were never there"
    );
}

#[test]
fn is_idempotent_on_a_second_run() {
    let dir = setup_stale_project();
    assert_eq!(run(dir.path(), false), 0);
    let first = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert_eq!(run(dir.path(), false), 0);
    let second = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert_eq!(first, second);
}
