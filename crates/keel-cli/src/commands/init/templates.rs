//! Compile-time embedded templates for tool integrations.
//! All templates are loaded via include_str!() from the crate's `templates/` directory.
//! Some constants are reserved for future use (e.g., GitLab CI, pre-commit hook template).
//!
//! The instruction files for tools with "full" command lists (Claude Code, Gemini CLI,
//! Copilot, Aider, Letta Code, AGENTS.md) are composed at compile time from two parts:
//! a tool-specific *head* (title, intro, and the "before/after edit" sections, which
//! contain wording `compile_note.rs` rewrites per-tool) plus one shared
//! `templates/shared/core.md` *tail* (circuit breaker, scaffolding, the full Commands
//! and MCP Tools lists, and Common Mistakes). This keeps the command lists — the thing
//! that actually drifted between templates — defined exactly once. `concat!` accepts
//! `include_str!()` arguments directly since both expand to string literals at that
//! position, so this stays a plain `&'static str` const, no runtime cost or call-site
//! changes needed.
#![allow(dead_code)]

// Inner attribute above allows dead_code for this module — some templates
// are embedded now but only used when their tool detection is implemented.

// Note: `concat!` requires each argument to itself be a literal (or a macro that
// expands to one, like `include_str!` or `env!`) — it cannot take a reference to
// another `const`. So `templates/shared/core.md` is included directly in every
// composition below rather than bound to a shared constant first; it is still
// defined exactly once on disk. Update command lists ONLY in that file.
//
// Every composed template also opens with a version-stamped `keel:start` marker
// (`keel_start_stamp!`, a macro rather than a `const` since `concat!` can't take
// a `const` either): `<!-- keel:start -->` followed by `<!-- keel:version X.Y.Z -->` where
// X.Y.Z is this build's `CARGO_PKG_VERSION`. `map`/`compile` compare that stamp
// (falling back to `.keel/keel.json`'s pinned version) against the running
// binary to detect docs that have drifted out of date — see
// `commands::version_drift_message`. The stamp is a second line *inside* the
// marked block, not part of the `<!-- keel:start -->` marker text itself, so
// `merge.rs`'s literal marker search is unaffected.
macro_rules! keel_start_stamp {
    () => {
        concat!(
            "<!-- keel:start -->\n<!-- keel:version ",
            env!("CARGO_PKG_VERSION"),
            " -->\n"
        )
    };
}

// --- Claude Code ---
pub const CLAUDE_CODE_SETTINGS: &str = include_str!("../../../templates/claude-code/settings.json");
pub const CLAUDE_CODE_INSTRUCTIONS: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/claude-code/keel-instructions.md"),
    include_str!("../../../templates/shared/core.md"),
    "<!-- keel:end -->\n"
);

// --- Cursor ---
pub const CURSOR_HOOKS: &str = include_str!("../../../templates/cursor/hooks.json");
pub const CURSOR_RULES: &str = include_str!("../../../templates/cursor/keel.mdc");

// --- Gemini CLI ---
pub const GEMINI_SETTINGS: &str = include_str!("../../../templates/gemini-cli/settings.json");
pub const GEMINI_INSTRUCTIONS: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/gemini-cli/GEMINI.md"),
    include_str!("../../../templates/shared/core.md"),
    "<!-- keel:end -->\n"
);

// --- Windsurf ---
pub const WINDSURF_HOOKS: &str = include_str!("../../../templates/windsurf/hooks.json");
pub const WINDSURF_RULES: &str = include_str!("../../../templates/windsurf/keel.windsurfrules");

// --- Copilot ---
pub const COPILOT_INSTRUCTIONS: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/copilot/copilot-instructions.md"),
    include_str!("../../../templates/shared/core.md"),
    "<!-- keel:end -->\n"
);

// --- Aider ---
pub const AIDER_CONF: &str = include_str!("../../../templates/aider/aider.conf.yml");
pub const AIDER_INSTRUCTIONS: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/aider/keel-instructions.md"),
    include_str!("../../../templates/shared/core.md"),
    "<!-- keel:end -->\n"
);

// --- Letta Code ---
pub const LETTA_SETTINGS: &str = include_str!("../../../templates/letta-code/settings.json");
pub const LETTA_INSTRUCTIONS: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/letta-code/keel-instructions.md"),
    include_str!("../../../templates/shared/core.md"),
    "<!-- keel:end -->\n"
);

// --- Codex ---
pub const CODEX_CONFIG: &str = include_str!("../../../templates/codex/config.toml");
pub const CODEX_NOTIFY: &str = include_str!("../../../templates/codex/keel-notify.py");

// --- Antigravity ---
pub const ANTIGRAVITY_RULES: &str = include_str!("../../../templates/antigravity/keel.md");
pub const ANTIGRAVITY_SKILL: &str = include_str!("../../../templates/antigravity/SKILL.md");

// --- Shared hooks ---
pub const POST_EDIT_HOOK: &str = include_str!("../../../templates/hooks/post-edit.sh");
pub const PLAN_CHECK_HOOK: &str = include_str!("../../../templates/hooks/plan-check.sh");
pub const PRE_COMMIT_HOOK: &str = include_str!("../../../templates/hooks/pre-commit.sh");

// --- CI ---
pub const GITHUB_ACTIONS: &str = include_str!("../../../templates/ci/github-actions.yml");
pub const GITLAB_CI: &str = include_str!("../../../templates/ci/gitlab-ci.yml");

// --- AGENTS.md (universal fallback) ---
pub const AGENTS_MD: &str = concat!(
    keel_start_stamp!(),
    include_str!("../../../templates/agents-md/AGENTS.md"),
    include_str!("../../../templates/shared/core.md"),
    "\n> Tip: If keel saves you time, `gh star FryrAI/Keel` helps the maintainers.\n\
     <!-- keel:end -->\n"
);

#[cfg(test)]
#[path = "templates_tests.rs"]
mod tests;
