//! Honest "after every edit" instruction text for generated agent config files.
//!
//! The on-edit hook defaults OFF (see `HookSelection::default`), so by
//! default nothing runs `keel compile` for the agent after an edit. Every
//! per-tool instruction template is written as if the hook IS installed
//! (the accurate case); this module rewrites that claim into an honest,
//! "run it yourself" instruction whenever `on_edit` is false, so the agent
//! is never told validation happened when it didn't and skips the manual
//! `keel compile` it should be running instead.

/// Honest replacement instruction for a missing on-edit hook (list-item form).
const HONEST_NOTE: &str =
    "- Run `keel compile <file>` after editing — the on-edit hook is not installed for \
     this project. (Re-run `keel init` and select the on-edit hook to automate this.)";

/// Every (false_claim, honest_instruction) pair across all tool templates.
///
/// One merged table is safe because `str::replace` no-ops on claims absent
/// from a given template, and no claim is a substring of another (the
/// trailing periods keep "Edit/Write." distinct from "Edit/Write/MultiEdit.").
const REPLACEMENTS: &[(&str, &str)] = &[
    // Claude Code / Letta Code
    (
        "- `keel compile` runs automatically via hooks after every Edit/Write/MultiEdit.",
        HONEST_NOTE,
    ),
    // Gemini CLI
    (
        "- `keel compile` runs automatically via hooks after every Edit/Write.",
        HONEST_NOTE,
    ),
    // Cursor rules header
    (
        "Hooks handle automatic validation. Follow this workflow for proactive checks:",
        "No on-edit hook is installed for this project — run `keel compile` manually \
         after edits. Follow this workflow for proactive checks:",
    ),
    // Windsurf rules header
    (
        "Hooks handle automatic validation via PreToolUse. Follow this workflow:",
        "No on-edit hook is installed for this project — run `keel compile` manually \
         after edits. Follow this workflow:",
    ),
    // Shared by Cursor and Windsurf rules bodies
    (
        "2. After EVERY file edit, `keel compile` runs automatically via hooks",
        "2. After EVERY file edit, run `keel compile <file>` manually (on-edit hook not installed)",
    ),
    (
        "- `keel compile <file>` — validate (auto-runs via hooks, can also run manually)",
        "- `keel compile <file>` — validate changes (run manually after every edit; \
         on-edit hook not installed)",
    ),
];

/// Rewrite instruction text claiming automatic on-edit compilation into an
/// honest instruction when the on-edit hook was NOT installed; left untouched
/// when `on_edit` is true (the claim holds).
pub fn apply_honest_compile_note(content: &str, on_edit: bool) -> String {
    if on_edit {
        return content.to_string();
    }
    let mut out = content.to_string();
    for (claim, honest) in REPLACEMENTS {
        out = out.replace(claim, honest);
    }
    out
}

#[cfg(test)]
#[path = "compile_note_tests.rs"]
mod tests;
