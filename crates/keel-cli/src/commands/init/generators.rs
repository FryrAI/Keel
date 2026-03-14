//! Per-tool config generators for keel init.
//!
//! Each generator returns a list of (path, content) pairs to write.
//! Generators use merge strategies from `merge.rs` to handle existing files.

use std::fs;
use std::path::{Path, PathBuf};

use super::merge;
use super::templates;
use super::HookSelection;

/// Inject PostToolUse hook into a JSON settings string (for on-edit mode).
fn inject_on_edit_hook(json_str: &str, matcher: &str) -> String {
    // Parse, add PostToolUse, re-serialize
    if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(json_str) {
        let hooks = val
            .as_object_mut()
            .and_then(|o| o.get_mut("hooks"))
            .and_then(|h| h.as_object_mut());
        if let Some(hooks) = hooks {
            let post_tool = serde_json::json!([{
                "matcher": matcher,
                "hooks": [{
                    "type": "command",
                    "command": ".keel/hooks/post-edit.sh"
                }]
            }]);
            hooks.insert("PostToolUse".to_string(), post_tool);
        }
        serde_json::to_string_pretty(&val).unwrap_or_else(|_| json_str.to_string())
    } else {
        json_str.to_string()
    }
}

/// Remove SessionStart hooks from a JSON settings string.
fn strip_session_start(json_str: &str) -> String {
    if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(hooks) = val
            .as_object_mut()
            .and_then(|o| o.get_mut("hooks"))
            .and_then(|h| h.as_object_mut())
        {
            hooks.remove("SessionStart");
        }
        serde_json::to_string_pretty(&val).unwrap_or_else(|_| json_str.to_string())
    } else {
        json_str.to_string()
    }
}

/// Generate Claude Code config files: `.claude/settings.json` and `CLAUDE.md`.
pub fn generate_claude_code(root: &Path, hooks: &HookSelection) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // settings.json — JSON deep merge
    let settings_path = root.join(".claude/settings.json");
    let mut template = templates::CLAUDE_CODE_SETTINGS.to_string();
    if !hooks.session_start {
        template = strip_session_start(&template);
    }
    if hooks.on_edit {
        template = inject_on_edit_hook(&template, "Edit|MultiEdit|Write");
    }
    match merge::merge_json_file(&settings_path, &template) {
        Ok(content) => files.push((settings_path, content)),
        Err(e) => eprintln!(
            "keel init: warning: Claude Code settings merge failed: {}",
            e
        ),
    }

    // CLAUDE.md — markdown marker merge
    let md_path = root.join("CLAUDE.md");
    match merge::merge_markdown_file(&md_path, templates::CLAUDE_CODE_INSTRUCTIONS) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!("keel init: warning: CLAUDE.md merge failed: {}", e),
    }

    files
}

/// Generate Cursor config files: `.cursor/hooks.json` and `.cursor/rules/keel.mdc`.
pub fn generate_cursor(root: &Path, hooks: &HookSelection) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // hooks.json — JSON deep merge
    let hooks_path = root.join(".cursor/hooks.json");
    let mut template = templates::CURSOR_HOOKS.to_string();
    if !hooks.session_start {
        template = strip_session_start(&template);
    }
    if hooks.on_edit {
        template = inject_on_edit_hook(&template, "Edit|Write|MultiEdit");
    }
    match merge::merge_json_file(&hooks_path, &template) {
        Ok(content) => files.push((hooks_path, content)),
        Err(e) => eprintln!("keel init: warning: Cursor hooks merge failed: {}", e),
    }

    // keel.mdc — write to rules directory (no merge, overwrite)
    let rules_path = root.join(".cursor/rules/keel.mdc");
    files.push((rules_path, templates::CURSOR_RULES.to_string()));

    files
}

/// Generate Gemini CLI config files: `.gemini/settings.json` and `GEMINI.md`.
pub fn generate_gemini_cli(root: &Path, hooks: &HookSelection) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // settings.json — JSON deep merge
    let settings_path = root.join(".gemini/settings.json");
    let mut template = templates::GEMINI_SETTINGS.to_string();
    if !hooks.session_start {
        template = strip_session_start(&template);
    }
    if hooks.on_edit {
        template = inject_on_edit_hook(&template, "Edit|Write");
    }
    match merge::merge_json_file(&settings_path, &template) {
        Ok(content) => files.push((settings_path, content)),
        Err(e) => eprintln!("keel init: warning: Gemini settings merge failed: {}", e),
    }

    // GEMINI.md — markdown marker merge
    let md_path = root.join("GEMINI.md");
    match merge::merge_markdown_file(&md_path, templates::GEMINI_INSTRUCTIONS) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!("keel init: warning: GEMINI.md merge failed: {}", e),
    }

    files
}

/// Generate Windsurf config files: `.windsurf/hooks.json` and `.windsurfrules`.
pub fn generate_windsurf(root: &Path, hooks: &HookSelection) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // hooks.json — JSON deep merge
    let hooks_path = root.join(".windsurf/hooks.json");
    let mut template = templates::WINDSURF_HOOKS.to_string();
    if !hooks.session_start {
        template = strip_session_start(&template);
    }
    if hooks.on_edit {
        template = inject_on_edit_hook(&template, "Edit|Write");
    }
    // Create .windsurf/ if it doesn't exist (may have been detected via .windsurfrules)
    match merge::merge_json_file(&hooks_path, &template) {
        Ok(content) => files.push((hooks_path, content)),
        Err(e) => eprintln!("keel init: warning: Windsurf hooks merge failed: {}", e),
    }

    // .windsurfrules — overwrite (not markdown with markers)
    let rules_path = root.join(".windsurfrules");
    files.push((rules_path, templates::WINDSURF_RULES.to_string()));

    files
}

/// Generate Letta Code config files: `.letta/settings.json` and `LETTA.md`.
pub fn generate_letta_code(root: &Path, hooks: &HookSelection) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // settings.json — JSON deep merge
    let settings_path = root.join(".letta/settings.json");
    let mut template = templates::LETTA_SETTINGS.to_string();
    if !hooks.session_start {
        template = strip_session_start(&template);
    }
    if hooks.on_edit {
        template = inject_on_edit_hook(&template, "Edit|Write|MultiEdit");
    }
    match merge::merge_json_file(&settings_path, &template) {
        Ok(content) => files.push((settings_path, content)),
        Err(e) => eprintln!("keel init: warning: Letta settings merge failed: {}", e),
    }

    // Instruction file — markdown marker merge
    let md_path = root.join("LETTA.md");
    match merge::merge_markdown_file(&md_path, templates::LETTA_INSTRUCTIONS) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!("keel init: warning: LETTA.md merge failed: {}", e),
    }

    files
}

/// Generate Copilot instruction file: `.github/copilot-instructions.md`.
pub fn generate_copilot(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    let md_path = root.join(".github/copilot-instructions.md");
    match merge::merge_markdown_file(&md_path, templates::COPILOT_INSTRUCTIONS) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!(
            "keel init: warning: copilot-instructions.md merge failed: {}",
            e
        ),
    }

    files
}

/// Generate Aider config files: `.aider.conf.yml` and `.aider/keel-instructions.md`.
pub fn generate_aider(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // Config — overwrite (YAML, no merge strategy)
    let conf_path = root.join(".aider.conf.yml");
    if !conf_path.exists() {
        files.push((conf_path, templates::AIDER_CONF.to_string()));
    }

    // Instruction file — markdown marker merge
    let md_path = root.join(".aider/keel-instructions.md");
    match merge::merge_markdown_file(&md_path, templates::AIDER_INSTRUCTIONS) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!("keel init: warning: aider instructions merge failed: {}", e),
    }

    files
}

/// Generate Codex config files: `.codex/config.toml` and `.codex/keel-notify.py`.
pub fn generate_codex(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // config.toml — only write if not present (don't clobber user config)
    let conf_path = root.join(".codex/config.toml");
    if !conf_path.exists() {
        files.push((conf_path, templates::CODEX_CONFIG.to_string()));
    }

    // Notify script
    let notify_path = root.join(".codex/keel-notify.py");
    files.push((notify_path, templates::CODEX_NOTIFY.to_string()));

    files
}

/// Generate Antigravity config files: `.agent/keel.md` and `.agent/skills/keel/SKILL.md`.
pub fn generate_antigravity(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    // Workspace rule
    let rules_path = root.join(".agent/rules/keel.md");
    files.push((rules_path, templates::ANTIGRAVITY_RULES.to_string()));

    // Skill file
    let skill_path = root.join(".agent/skills/keel/SKILL.md");
    files.push((skill_path, templates::ANTIGRAVITY_SKILL.to_string()));

    files
}

/// Generate GitHub Actions workflow: `.github/workflows/keel.yml`.
pub fn generate_github_actions(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    let workflow_path = root.join(".github/workflows/keel.yml");
    if !workflow_path.exists() {
        files.push((workflow_path, templates::GITHUB_ACTIONS.to_string()));
    }

    files
}

/// Generate AGENTS.md (universal fallback, always written).
pub fn generate_agents_md(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();

    let md_path = root.join("AGENTS.md");
    match merge::merge_markdown_file(&md_path, templates::AGENTS_MD) {
        Ok(content) => files.push((md_path, content)),
        Err(e) => eprintln!("keel init: warning: AGENTS.md merge failed: {}", e),
    }

    files
}

/// Write a list of (path, content) pairs to disk, creating parent directories.
pub fn write_files(files: &[(PathBuf, String)], verbose: bool) -> usize {
    let mut count = 0;
    for (path, content) in files {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!(
                        "keel init: warning: failed to create directory {}: {}",
                        parent.display(),
                        e
                    );
                    continue;
                }
            }
        }
        match fs::write(path, content) {
            Ok(_) => {
                count += 1;
                if verbose {
                    eprintln!("keel init: wrote {}", path.display());
                }
            }
            Err(e) => {
                eprintln!(
                    "keel init: warning: failed to write {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
    count
}
