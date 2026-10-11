//! `keel init --update-docs` — the human-authorized fix for the version
//! drift `map`/`compile` detect (see `commands::version_drift`).
//!
//! Rewrites the keel-managed `<!-- keel:start -->`/`<!-- keel:end -->` block
//! in every agent doc file already present, migrates generated JSON hook
//! commands, installs a missing session-start script when wired, refreshes
//! `.keel/hooks/post-edit.sh` only if it is already installed, and syncs
//! `.keel/keel.json`'s pinned version to this binary. Deliberately narrower
//! than a full `keel init --merge`:
//! non-interactive, no tool (re)detection, no config merge, no database
//! reset, and it never creates a doc file for a tool that wasn't already
//! integrated — only refreshes what `keel init` previously wrote.

use std::fs;
use std::path::Path;

use super::compile_note::apply_honest_compile_note;
use super::{generators, hook_script, merge};

/// Run `keel init --update-docs` against the project rooted at `cwd`.
pub(super) fn run(cwd: &Path, verbose: bool) -> i32 {
    let keel_dir = keel_core::paths::keel_dir(cwd);
    if !keel_dir.exists() || !keel_dir.join("keel.json").exists() {
        eprintln!("keel init --update-docs: .keel/ not initialized. Run `keel init` first.");
        return 2;
    }

    // Whether the on-edit hook is currently installed, read BEFORE it is
    // regenerated below — determines whether the refreshed docs may honestly
    // claim automatic post-edit compilation (see `apply_honest_compile_note`).
    // Checked at the path the installer writes and the tool configs reference
    // (`<cwd>/.keel/hooks`), NOT `keel_dir`: in a linked worktree `keel_dir`
    // resolves to the main checkout, and a hook installed there says nothing
    // about this worktree — inspecting one path and writing another would
    // create the very stray hook this guard exists to prevent.
    let hooks_dir = cwd.join(".keel/hooks");
    let on_edit = hooks_dir.join("post-edit.sh").exists();

    // `generators::MANAGED_DOCS` is the shared table: whatever `keel init`
    // can create, this refreshes. Hook configs are migrated separately below.
    let mut file_count = 0;
    for (rel, template) in generators::MANAGED_DOCS {
        let path = cwd.join(rel);
        if !path.exists() {
            continue; // refresh existing integrations only, never create new ones
        }
        let content = apply_honest_compile_note(template, on_edit);
        match merge::merge_markdown_file(&path, &content) {
            Ok(merged) => file_count += generators::write_files(&[(path, merged)], verbose),
            Err(e) => eprintln!(
                "keel init --update-docs: warning: {} merge failed: {}",
                path.display(),
                e
            ),
        }
    }

    migrate_hook_commands(cwd, verbose);

    // Refresh, never create: a post-edit or ExitPlanMode hook the user never
    // installed has nothing in `.claude/settings.json` pointing at it, so
    // creating one here would leave a stray, unwired script AND corrupt the
    // honest compile note on the next run (`on_edit` would then read true
    // even though nothing invokes the hook). See issue #72.
    if on_edit {
        hook_script::install_post_edit_hook(cwd, verbose);
    }
    if hooks_dir.join("plan-check.sh").exists() {
        hook_script::install_plan_check_hook(cwd, verbose);
    }
    // Appends only entries a later release added; every other line stays.
    super::helpers::update_gitignore(cwd, verbose);

    let binary_version = env!("CARGO_PKG_VERSION");
    if let Err(e) = keel_core::config::KeelConfig::sync_version(&keel_dir, binary_version) {
        eprintln!("keel init --update-docs: failed to sync keel.json version: {e}");
        return 2;
    }

    let hook_note = if on_edit { ", post-edit.sh," } else { "" };
    eprintln!(
        "keel init --update-docs: refreshed {file_count} doc file(s){hook_note} \
         and keel.json version -> {binary_version}"
    );
    0
}

pub(super) fn migrate_hook_commands(cwd: &Path, verbose: bool) {
    for rel in [
        ".claude/settings.json",
        ".cursor/hooks.json",
        ".gemini/settings.json",
        ".windsurf/hooks.json",
        ".letta/settings.json",
    ] {
        let path = cwd.join(rel);
        if !path.exists() {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("keel init --update-docs: warning: cannot read {rel}: {e}");
                continue;
            }
        };
        let mut migrated = replace_token(
            &content,
            "timeout 10 keel map --llm --cached",
            "keel hook session-start",
        );
        migrated = replace_token(
            &migrated,
            ".keel/hooks/plan-check.sh",
            "keel hook plan-check",
        );
        migrated = replace_token(&migrated, ".keel/hooks/post-edit.sh", "keel hook post-edit");
        // These client names match the five generators' explicit arguments.
        for client in [
            "claude-code",
            "cursor",
            "gemini-cli",
            "windsurf",
            "letta-code",
        ] {
            migrated = replace_token(
                &migrated,
                &format!(".keel/hooks/post-edit.sh {client}"),
                &format!("keel hook post-edit {client}"),
            );
        }
        if migrated != content {
            if let Err(e) = fs::write(&path, &migrated) {
                eprintln!("keel init --update-docs: warning: cannot write {rel}: {e}");
                continue;
            }
        }
        if migrated.contains("\"keel hook session-start\"")
            && !cwd.join(".keel/hooks/session-start.sh").exists()
        {
            hook_script::install_session_start_hook(cwd, verbose);
        }
    }
}

/// Replace each JSON string token `"from"` with `"to"`, skipping a token whose
/// opening quote is escaped (`\"`): that quote sits inside a user's own string.
fn replace_token(text: &str, from: &str, to: &str) -> String {
    let (from, to) = (format!("\"{from}\""), format!("\"{to}\""));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(&from) {
        out.push_str(&rest[..i]);
        out.push_str(if out.ends_with('\\') { &from } else { &to });
        rest = &rest[i + from.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "update_docs_tests.rs"]
mod tests;
