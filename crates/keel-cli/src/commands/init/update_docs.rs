//! `keel init --update-docs` — the human-authorized fix for the version
//! drift `map`/`compile` detect (see `commands::version_drift`).
//!
//! Rewrites the keel-managed `<!-- keel:start -->`/`<!-- keel:end -->` block
//! in every agent doc file already present, refreshes
//! `.keel/hooks/post-edit.sh` only if it is already installed, and syncs
//! `.keel/keel.json`'s pinned version to this binary. Deliberately narrower
//! than a full `keel init --merge`:
//! non-interactive, no tool (re)detection, no config merge, no database
//! reset, and it never creates a doc file for a tool that wasn't already
//! integrated — only refreshes what `keel init` previously wrote.

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
    // can create, this refreshes. settings.json/hooks.json files are not on
    // it — this command only touches docs and the on-edit hook script, per its
    // own doc comment.
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

    let binary_version = env!("CARGO_PKG_VERSION");
    if let Err(e) = keel_core::config::KeelConfig::sync_version(&keel_dir, binary_version) {
        eprintln!("keel init --update-docs: warning: failed to sync keel.json version: {e}");
    }

    let hook_note = if on_edit { ", post-edit.sh," } else { "" };
    eprintln!(
        "keel init --update-docs: refreshed {file_count} doc file(s){hook_note} \
         and keel.json version -> {binary_version}"
    );
    0
}

#[cfg(test)]
#[path = "update_docs_tests.rs"]
mod tests;
