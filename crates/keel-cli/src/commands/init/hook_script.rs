//! Hook script installation for keel init.
//!
//! Installs:
//! - `.keel/hooks/post-edit.sh` — shared hook for Tier 1 tools (opt-in)
//! - `.keel/hooks/plan-check.sh` — Claude Code `ExitPlanMode` advisory hook
//! - `.git/hooks/pre-commit` — git pre-commit hook

use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::templates;
use super::HookSelection;

/// Install the shared post-edit hook script to `.keel/hooks/post-edit.sh`.
pub fn install_post_edit_hook(root: &Path, verbose: bool) {
    write_hook(root, "post-edit.sh", templates::POST_EDIT_HOOK, verbose);
}

/// Install the Claude Code plan-check hook to `.keel/hooks/plan-check.sh`.
///
/// Claude Code only, by design: the `ExitPlanMode` payload shape is verified
/// there, and a hook wired to an unverified payload would silently no-op —
/// worse than no hook, because the absence of findings reads as a clean bill of
/// health. The script itself is advisory (always exits 0).
pub fn install_plan_check_hook(root: &Path, verbose: bool) {
    write_hook(root, "plan-check.sh", templates::PLAN_CHECK_HOOK, verbose);
}

/// Write `name` into `.keel/hooks/` with content `body`, marking it executable
/// on unix.
fn write_hook(root: &Path, name: &str, body: &str, verbose: bool) {
    let hooks_dir = root.join(".keel/hooks");
    if let Err(e) = fs::create_dir_all(&hooks_dir) {
        eprintln!("keel init: warning: failed to create .keel/hooks: {}", e);
        return;
    }
    let hook_path = hooks_dir.join(name);
    match fs::write(&hook_path, body) {
        Ok(_) => {
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755));
            }
            if verbose {
                eprintln!("keel init: installed .keel/hooks/{}", name);
            }
        }
        Err(e) => {
            eprintln!("keel init: warning: failed to install {}: {}", name, e);
        }
    }
}

/// Install a git pre-commit hook based on hook selection.
pub fn install_git_hook(root: &Path, verbose: bool, hooks: &HookSelection) {
    if !hooks.pre_commit && !hooks.pre_commit_audit {
        return; // No pre-commit hooks selected
    }

    let hooks_dir = root.join(".git/hooks");
    if !hooks_dir.exists() {
        if verbose {
            eprintln!("keel init: no .git/hooks directory, skipping hook install");
        }
        return;
    }

    let hook_path = hooks_dir.join("pre-commit");
    if hook_path.exists() {
        eprintln!("keel init: pre-commit hook already exists, not overwriting");
        return;
    }

    let mut hook_content = String::from("#!/bin/bash\nset -e\n# Installed by keel init\n");
    if hooks.pre_commit {
        hook_content.push_str("keel compile --changed\n");
    }
    if hooks.pre_commit_audit {
        hook_content.push_str("keel audit --changed || true\n");
    }

    match fs::write(&hook_path, &hook_content) {
        Ok(_) => {
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755));
            }
            if verbose {
                eprintln!("keel init: installed pre-commit hook");
            }
        }
        Err(e) => {
            eprintln!(
                "keel init: warning: failed to install pre-commit hook: {}",
                e
            );
        }
    }
}
