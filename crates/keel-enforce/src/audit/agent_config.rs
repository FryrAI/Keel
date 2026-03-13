//! Agent Config dimension — checks for CLAUDE.md, .claude/, hooks, commands.

use std::path::Path;

use crate::types::{AuditFinding, AuditSeverity};

pub fn check_agent_config(root_dir: &Path) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    // CLAUDE.md existence
    let claude_md = root_dir.join("CLAUDE.md");
    if claude_md.exists() {
        // Check size
        if let Ok(content) = std::fs::read_to_string(&claude_md) {
            let line_count = content.lines().count();
            if line_count > 500 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Fail,
                    check: "claude_md_size".into(),
                    message: format!("CLAUDE.md is {} lines (>500)", line_count),
                    tip: Some(
                        "Split into focused sections or decompose into .claude/ directory".into(),
                    ),
                    file: Some("CLAUDE.md".into()),
                    count: None,
                });
            } else if line_count > 300 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "claude_md_size".into(),
                    message: format!("CLAUDE.md is {} lines (>300)", line_count),
                    tip: Some("Consider splitting to keep agent context lean".into()),
                    file: Some("CLAUDE.md".into()),
                    count: None,
                });
            }
        }
    } else {
        findings.push(AuditFinding {
            severity: AuditSeverity::Fail,
            check: "no_claude_md".into(),
            message: "No CLAUDE.md found".into(),
            tip: Some(
                "Create CLAUDE.md with build commands, repo map, and conventions".into(),
            ),
            file: None,
            count: None,
        });
    }

    // .claude/ directory
    let claude_dir = root_dir.join(".claude");
    if !claude_dir.exists() {
        findings.push(AuditFinding {
            severity: AuditSeverity::Tip,
            check: "no_claude_dir".into(),
            message: "No .claude/ directory found".into(),
            tip: Some(
                "Create .claude/ for settings, hooks, and slash commands".into(),
            ),
            file: None,
            count: None,
        });
    } else {
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
