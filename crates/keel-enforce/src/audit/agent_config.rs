//! Agent Config dimension — checks for agent instruction files, config directories, hooks, commands.

use std::path::Path;

use crate::types::{AuditFinding, AuditSeverity};

/// Agent instruction files recognized across major AI coding tools.
const AGENT_INSTRUCTION_FILES: &[&str] = &[
    "CLAUDE.md",
    ".cursorrules",
    "GEMINI.md",
    "WINDSURF.md",
    ".windsurfrules",
    "AGENTS.md",
    "COPILOT.md",
    "CODERABBIT.yaml",
    ".github/copilot-instructions.md",
];

/// Agent instruction directories (may contain nested rules).
const AGENT_INSTRUCTION_DIRS: &[&str] = &[".cursor/rules"];

/// Agent config directories.
const AGENT_CONFIG_DIRS: &[&str] = &[".claude", ".cursor", ".gemini", ".windsurf", ".letta"];

pub fn check_agent_config(root_dir: &Path) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    // Check for any agent instruction file
    let found_instruction = AGENT_INSTRUCTION_FILES
        .iter()
        .find(|f| root_dir.join(f).exists());

    let found_instruction_dir = AGENT_INSTRUCTION_DIRS
        .iter()
        .any(|d| root_dir.join(d).exists());

    if let Some(&instruction_file) = found_instruction {
        // Check size of whichever instruction file was found
        let path = root_dir.join(instruction_file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let line_count = content.lines().count();
            if line_count > 300 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Fail,
                    check: "agent_instructions_size".into(),
                    message: format!("{} is {} lines (>300)", instruction_file, line_count),
                    tip: Some(
                        "This file is too large for agents to hold in context. \
                         Split into focused sections under 150 lines each, or decompose \
                         into a config directory (e.g., .claude/rules/)."
                            .into(),
                    ),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            } else if line_count > 150 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "agent_instructions_size".into(),
                    message: format!("{} is {} lines (>150)", instruction_file, line_count),
                    tip: Some(format!(
                        "Run `wc -l {}` and identify sections to extract into \
                         separate files. Keep agent instructions under 150 lines \
                         for optimal context usage.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }

            // Content quality: check for test command
            let lower = content.to_lowercase();
            let has_test_cmd = ["cargo test", "pytest", "npm test", "go test", "make test"]
                .iter()
                .any(|pat| lower.contains(pat));
            if !has_test_cmd {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "agent_instructions_no_test_cmd".into(),
                    message: format!("{} has no test command", instruction_file),
                    tip: Some(format!(
                        "Add a ## Testing section to {} with runnable test commands, e.g.:\n  \
                         cargo test\n\
                         Agents need exact commands to verify their work.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }

            // Content quality: check for build/lint command
            let has_build_cmd = [
                "cargo build",
                "cargo check",
                "cargo clippy",
                "npm run",
                "go build",
                "make",
                "ruff",
                "eslint",
            ]
            .iter()
            .any(|pat| lower.contains(pat));
            if !has_build_cmd {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "agent_instructions_no_build_cmd".into(),
                    message: format!("{} has no build/lint command", instruction_file),
                    tip: Some(format!(
                        "Add a ## Build section to {} with build/lint commands, e.g.:\n  \
                         cargo clippy --workspace\n\
                         Agents need to know how to check their changes compile.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }

            // Content quality: check for repo map / architecture section
            let has_repo_map = [
                "## architecture",
                "## project structure",
                "## crates",
                "## modules",
                "## layout",
                "## repo map",
                "crate layout",
                "directory structure",
            ]
            .iter()
            .any(|pat| lower.contains(pat));
            if !has_repo_map {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "no_repo_map".into(),
                    message: format!("{} has no architecture/repo map section", instruction_file),
                    tip: Some(format!(
                        "Add a ## Architecture or ## Project Structure section to {} showing \
                         where key modules live. Example:\n  ## Architecture\n  src/\n    \
                         api/     # HTTP handlers\n    models/  # Domain types\n    store/   \
                         # Database layer\n\
                         Agents need a map to navigate the codebase.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }

            // Content quality: check for definition of done
            let has_dod = [
                "definition of done",
                "before merging",
                "before committing",
                "checklist",
                "required before",
                "## done",
                "must pass",
            ]
            .iter()
            .any(|pat| lower.contains(pat));
            if !has_dod {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "no_definition_of_done".into(),
                    message: format!("{} has no definition of done", instruction_file),
                    tip: Some(format!(
                        "Add a ## Definition of Done section to {} specifying what must be true \
                         before the agent stops. Example:\n  ## Definition of Done\n  \
                         - All tests pass\n  - No clippy warnings\n  \
                         - PR description includes summary\n\
                         Without this, agents don't know when they're finished.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }

            // Content quality: check for constraints / "do not" list
            let has_constraints = [
                "do not",
                "don't",
                "must not",
                "never ",
                "## constraints",
                "## rules",
                "forbidden",
                "prohibited",
            ]
            .iter()
            .any(|pat| lower.contains(pat));
            if !has_constraints {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Tip,
                    check: "no_constraints_list".into(),
                    message: format!(
                        "{} has no explicit constraints or 'do not' rules",
                        instruction_file
                    ),
                    tip: Some(format!(
                        "Add explicit constraints to {} so agents know what to avoid. Example:\n  \
                         ## Constraints\n  - Do NOT modify test fixtures\n  \
                         - Never commit .env files\n  \
                         - Do not add dependencies without approval\n\
                         Constraints prevent agents from making costly mistakes.",
                        instruction_file,
                    )),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            }
        }
    } else if !found_instruction_dir {
        findings.push(AuditFinding {
            severity: AuditSeverity::Fail,
            check: "no_agent_instructions".into(),
            message: "No agent instruction file found".into(),
            tip: Some(
                "Create CLAUDE.md, .cursorrules, AGENTS.md, or similar with build commands and conventions".into(),
            ),
            file: None,
            count: None,
        });
    }

    // Check for any agent config directory
    let found_config_dir = AGENT_CONFIG_DIRS.iter().find(|d| root_dir.join(d).exists());

    if found_config_dir.is_none() {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "no_agent_dir".into(),
            message: "No agent config directory found".into(),
            tip: Some(
                "Create .claude/ or .cursor/ for settings, hooks, and commands. \
                 Example: mkdir -p .claude/commands && echo '{}' > .claude/settings.json"
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    // Claude-specific checks (only if .claude/ exists)
    let claude_dir = root_dir.join(".claude");
    if claude_dir.exists() {
        // Check for hooks in settings.json
        let settings_path = claude_dir.join("settings.json");
        if settings_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    let hooks = parsed.get("hooks");
                    if hooks.is_none() {
                        findings.push(AuditFinding {
                            severity: AuditSeverity::Warn,
                            check: "no_hooks".into(),
                            message: "No hooks configured in .claude/settings.json".into(),
                            tip: Some(
                                "Add PostToolUse hooks for automatic verification after edits. \
                                 Example in .claude/settings.json:\n  \
                                 \"hooks\": {\"PostToolUse\": [{\"matcher\": \"Edit|Write\", \
                                 \"command\": \"keel compile --changed\"}]}"
                                    .into(),
                            ),
                            file: Some(".claude/settings.json".into()),
                            count: None,
                        });
                    } else if let Some(hooks_obj) = hooks.and_then(|h| h.as_object()) {
                        // Hooks exist but check for PostToolUse-like entries
                        let has_post_tool = hooks_obj.keys().any(|k| {
                            let lower = k.to_lowercase();
                            lower.contains("posttool") || lower.contains("post_tool")
                        });
                        if !has_post_tool {
                            findings.push(AuditFinding {
                                severity: AuditSeverity::Warn,
                                check: "no_post_tool_hooks".into(),
                                message: "Hooks exist but no PostToolUse hook found".into(),
                                tip: Some(
                                    "Add a hook that auto-runs verification after edits. \
                                     Example in .claude/settings.json:\n  \
                                     \"hooks\": {\"PostToolUse\": [{\"matcher\": \"Edit|Write\", \
                                     \"command\": \"keel compile --changed\"}]}"
                                        .into(),
                                ),
                                file: Some(".claude/settings.json".into()),
                                count: None,
                            });
                        }
                    }
                }
            }
        } else {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "no_hooks".into(),
                message: "No .claude/settings.json found".into(),
                tip: Some(
                    "Create .claude/settings.json with hooks for automatic feedback loops. \
                     Example:\n  {\"hooks\": {\"PostToolUse\": [{\"matcher\": \"Edit|Write\", \
                     \"command\": \"keel compile --changed\"}]}}"
                        .into(),
                ),
                file: None,
                count: None,
            });
        }

        // Check for slash commands
        let commands_dir = claude_dir.join("commands");
        if !commands_dir.exists() {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "no_commands".into(),
                message: "No .claude/commands/ directory".into(),
                tip: Some(
                    "Create .claude/commands/ with slash commands for common workflows. \
                     Example: mkdir -p .claude/commands && echo 'Run cargo test' > \
                     .claude/commands/test.md"
                        .into(),
                ),
                file: None,
                count: None,
            });
        }
    }

    // Check for progressive disclosure (folder-level rules)
    let has_folder_rules = root_dir.join(".claude/rules").is_dir()
        || root_dir.join(".cursor/rules").is_dir()
        || has_subfolder_claude_md(root_dir);
    if !has_folder_rules {
        findings.push(AuditFinding {
            severity: AuditSeverity::Tip,
            check: "no_progressive_disclosure".into(),
            message: "No folder-level rules for progressive disclosure".into(),
            tip: Some(
                "Create .claude/rules/ with focused rule files for different parts of the \
                 codebase. Example: .claude/rules/testing.md, .claude/rules/api.md. This keeps \
                 root CLAUDE.md lean while giving agents context-specific guidance."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    findings
}

fn has_subfolder_claude_md(root_dir: &Path) -> bool {
    let entries = match std::fs::read_dir(root_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_str().unwrap_or("");
        if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
            continue;
        }
        if entry.path().join("CLAUDE.md").exists() {
            return true;
        }
        // Check one more level
        if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
            for sub in sub_entries.flatten() {
                if sub.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && sub.path().join("CLAUDE.md").exists()
                {
                    return true;
                }
            }
        }
    }
    false
}
