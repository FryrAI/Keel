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
            if line_count > 500 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Fail,
                    check: "agent_instructions_size".into(),
                    message: format!("{} is {} lines (>500)", instruction_file, line_count),
                    tip: Some(
                        "Split into focused sections or decompose into a config directory".into(),
                    ),
                    file: Some(instruction_file.to_string()),
                    count: None,
                });
            } else if line_count > 300 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "agent_instructions_size".into(),
                    message: format!("{} is {} lines (>300)", instruction_file, line_count),
                    tip: Some("Consider splitting to keep agent context lean".into()),
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
            severity: AuditSeverity::Tip,
            check: "no_agent_dir".into(),
            message: "No agent config directory found".into(),
            tip: Some(
                "Create .claude/, .cursor/, or similar for settings, hooks, and commands".into(),
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
                    let has_hooks = parsed.get("hooks").is_some();
                    if !has_hooks {
                        findings.push(AuditFinding {
                            severity: AuditSeverity::Tip,
                            check: "no_hooks".into(),
                            message: "No hooks configured in .claude/settings.json".into(),
                            tip: Some(
                                "Add PostToolUse hooks for automatic lint/compile feedback".into(),
                            ),
                            file: Some(".claude/settings.json".into()),
                            count: None,
                        });
                    }
                }
            }
        } else {
            findings.push(AuditFinding {
                severity: AuditSeverity::Tip,
                check: "no_hooks".into(),
                message: "No .claude/settings.json found".into(),
                tip: Some(
                    "Create .claude/settings.json with hooks for automatic feedback loops".into(),
                ),
                file: None,
                count: None,
            });
        }

        // Check for slash commands
        let commands_dir = claude_dir.join("commands");
        if !commands_dir.exists() {
            findings.push(AuditFinding {
                severity: AuditSeverity::Tip,
                check: "no_commands".into(),
                message: "No .claude/commands/ directory".into(),
                tip: Some("Add slash commands for common workflows (e.g., /review, /test)".into()),
                file: None,
                count: None,
            });
        }
    }

    findings
}
