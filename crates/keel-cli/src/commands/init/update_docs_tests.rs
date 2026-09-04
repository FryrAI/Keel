use std::fs;

use tempfile::TempDir;

use super::*;
use crate::commands::init::templates;

const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The exact claim `apply_honest_compile_note` rewrites when the on-edit
/// hook is not installed (Claude Code / Letta Code phrasing).
const AUTO_COMPILE_CLAIM: &str =
    "`keel compile` runs automatically via hooks after every Edit/Write/MultiEdit.";

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

/// A minimal initialized project integrated with `CLAUDE.md` (whose template
/// carries the automatic-compile claim `apply_honest_compile_note` toggles),
/// with no `.keel/hooks/post-edit.sh` installed.
fn setup_stale_claude_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    fs::create_dir_all(&keel_dir).unwrap();
    fs::write(
        keel_dir.join("keel.json"),
        r#"{"version": "0.1.0", "languages": ["rust"]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
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
fn refreshes_an_already_installed_hook_with_client_flag_and_5s_timeout() {
    let dir = setup_stale_project();
    // The hook was already installed (e.g. by a prior `keel init` that chose
    // the on-edit option) — --update-docs must refresh it, not skip it.
    let hooks_dir = dir.path().join(".keel/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(
        hooks_dir.join("post-edit.sh"),
        "#!/bin/sh\necho stale hook\n",
    )
    .unwrap();

    run(dir.path(), false);

    let hook = fs::read_to_string(hooks_dir.join("post-edit.sh")).unwrap();
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
fn does_not_install_the_hook_when_it_was_never_present() {
    // Regression test for #72: `--update-docs` must never create
    // `.keel/hooks/post-edit.sh` — doing so leaves a stray, unwired script
    // (nothing in `.claude/settings.json` points at it) and would falsely
    // flip `on_edit` to true on the *next* run, corrupting the honest
    // compile note in the refreshed docs.
    let dir = setup_stale_project();
    assert!(!dir.path().join(".keel/hooks/post-edit.sh").exists());

    assert_eq!(run(dir.path(), false), 0);

    assert!(
        !dir.path().join(".keel/hooks/post-edit.sh").exists(),
        "--update-docs must not create a post-edit hook that was never installed"
    );
}

#[test]
fn stays_honest_about_automatic_compile_across_repeated_runs_without_a_hook() {
    // Regression test for #72: before the fix, run 1 created an unwired
    // post-edit.sh (docs stayed honest that run, since `on_edit` was read
    // before the create); run 2 then saw the hook `on_edit.exists() ==
    // true` and falsely claimed automatic compilation. The hook must never
    // be created here, so both runs must stay honest.
    let dir = setup_stale_claude_project();

    assert_eq!(run(dir.path(), false), 0);
    assert_eq!(run(dir.path(), false), 0);

    assert!(!dir.path().join(".keel/hooks/post-edit.sh").exists());
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(
        !claude_md.contains(AUTO_COMPILE_CLAIM),
        "must not claim automatic compilation when no hook is installed: {claude_md}"
    );
}

#[test]
fn keeps_the_automatic_compile_claim_when_the_hook_is_already_installed() {
    let dir = setup_stale_claude_project();
    let hooks_dir = dir.path().join(".keel/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(
        hooks_dir.join("post-edit.sh"),
        "#!/bin/sh\necho stale hook\n",
    )
    .unwrap();

    assert_eq!(run(dir.path(), false), 0);

    let hook = fs::read_to_string(hooks_dir.join("post-edit.sh")).unwrap();
    assert_eq!(
        hook,
        templates::POST_EDIT_HOOK,
        "an already-installed hook must be refreshed to the current template"
    );
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(
        claude_md.contains(AUTO_COMPILE_CLAIM),
        "an installed hook makes the automatic-compile claim honest: {claude_md}"
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
