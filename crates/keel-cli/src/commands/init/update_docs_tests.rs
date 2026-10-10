use std::fs;

use tempfile::TempDir;

use super::*;
use crate::commands::init::templates;

const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
fn migrates_exact_hook_tokens_in_all_configs_without_other_byte_changes() {
    let dir = setup_stale_project();
    let old = concat!(
        "{\r\n  \"commands\": [\"timeout 10 keel map --llm --cached\",",
        "\".keel/hooks/plan-check.sh\",\".keel/hooks/post-edit.sh\",",
        "\".keel/hooks/post-edit.sh claude-code\",\".keel/hooks/post-edit.sh cursor\",",
        "\".keel/hooks/post-edit.sh gemini-cli\",\".keel/hooks/post-edit.sh windsurf\",",
        "\".keel/hooks/post-edit.sh letta-code\"],\r\n",
        "  \"custom\": [\".keel/hooks/post-edit.sh widget\",",
        "\"timeout 10 keel map --llm --cached && echo widget\",",
        "\".keel/hooks/plan-check.sh --custom\"]\r\n}\r\n"
    );
    let expected = concat!(
        "{\r\n  \"commands\": [\"keel hook session-start\",",
        "\"keel hook plan-check\",\"keel hook post-edit\",",
        "\"keel hook post-edit claude-code\",\"keel hook post-edit cursor\",",
        "\"keel hook post-edit gemini-cli\",\"keel hook post-edit windsurf\",",
        "\"keel hook post-edit letta-code\"],\r\n",
        "  \"custom\": [\".keel/hooks/post-edit.sh widget\",",
        "\"timeout 10 keel map --llm --cached && echo widget\",",
        "\".keel/hooks/plan-check.sh --custom\"]\r\n}\r\n"
    );
    let configs = [
        ".claude/settings.json",
        ".cursor/hooks.json",
        ".gemini/settings.json",
        ".windsurf/hooks.json",
        ".letta/settings.json",
    ];
    for rel in configs {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, old).unwrap();
    }
    assert_eq!(run(dir.path(), false), 0);
    for rel in configs {
        assert_eq!(fs::read(dir.path().join(rel)).unwrap(), expected.as_bytes());
    }
    let script = dir.path().join(".keel/hooks/session-start.sh");
    assert_eq!(
        fs::read_to_string(&script).unwrap(),
        templates::SESSION_START_HOOK
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(script).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
    assert!(!dir.path().join(".keel/hooks/post-edit.sh").exists());
    assert!(!dir.path().join(".keel/hooks/plan-check.sh").exists());
}

#[test]
fn unchanged_configs_are_not_rewritten_and_missing_session_hook_is_installed() {
    let dir = setup_stale_project();
    let config = dir.path().join(".claude/settings.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let content = "{\n \"command\" : \"keel hook session-start\", \"custom\": \"widget\"\n}\n";
    fs::write(&config, content).unwrap();
    let old_time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
    fs::File::options()
        .write(true)
        .open(&config)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    assert_eq!(run(dir.path(), false), 0);
    assert_eq!(fs::read_to_string(&config).unwrap(), content);
    assert_eq!(fs::metadata(&config).unwrap().modified().unwrap(), old_time);
    let script = dir.path().join(".keel/hooks/session-start.sh");
    fs::write(&script, "custom session hook\n").unwrap();
    assert_eq!(run(dir.path(), false), 0);
    assert_eq!(fs::read_to_string(script).unwrap(), "custom session hook\n");
}

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

/// Run `git` in `dir`, panicking on failure.
fn git(args: &[&str], dir: &std::path::Path) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git not available");
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
}

#[test]
fn linked_worktree_does_not_inherit_the_main_checkouts_hook() {
    // Regression test for the #72 review finding: `keel_dir` resolves a
    // linked worktree to the MAIN checkout, but the hook is written under
    // `<cwd>/.keel/hooks`. The guard must look where the installer writes,
    // or a hook installed in main would be materialised in a worktree that
    // never wired one — and its docs would claim automatic compilation.
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main");
    fs::create_dir_all(main.join(".keel/hooks")).unwrap();
    git(&["init", "-q"], &main);
    git(&["config", "user.email", "test@test.com"], &main);
    git(&["config", "user.name", "Test"], &main);
    fs::write(main.join("f.txt"), "hi").unwrap();
    git(&["add", "."], &main);
    git(&["commit", "-q", "-m", "init"], &main);
    fs::write(
        main.join(".keel/keel.json"),
        r#"{"version": "0.1.0", "languages": ["rust"]}"#,
    )
    .unwrap();
    fs::write(
        main.join(".keel/hooks/post-edit.sh"),
        "#!/bin/sh\necho main hook\n",
    )
    .unwrap();

    let wt = dir.path().join("wt");
    git(&["worktree", "add", "-q", wt.to_str().unwrap()], &main);
    fs::write(
        wt.join("CLAUDE.md"),
        "<!-- keel:start -->\nstale content, no version stamp\n<!-- keel:end -->\n",
    )
    .unwrap();

    assert_eq!(run(&wt, false), 0);

    assert!(
        !wt.join(".keel/hooks/post-edit.sh").exists(),
        "a worktree must not inherit the main checkout's hook"
    );
    let claude_md = fs::read_to_string(wt.join("CLAUDE.md")).unwrap();
    assert!(
        !claude_md.contains(AUTO_COMPILE_CLAIM),
        "no hook is wired in this worktree, so the docs must not claim one: {claude_md}"
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

#[test]
fn escaped_quotes_inside_a_users_command_are_not_token_boundaries() {
    let dir = setup_stale_project();
    let path = dir.path().join(".claude/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = concat!(
        "[\"\\\"$CLAUDE_PROJECT_DIR/\\\".keel/hooks/post-edit.sh claude-code\",",
        "\"echo \\\".keel/hooks/plan-check.sh\",",
        "\".keel/hooks/plan-check.sh\"]\n"
    );
    let expected = content.replacen(
        ",\".keel/hooks/plan-check.sh\"]",
        ",\"keel hook plan-check\"]",
        1,
    );
    fs::write(&path, content).unwrap();
    assert_eq!(run(dir.path(), false), 0);
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}
