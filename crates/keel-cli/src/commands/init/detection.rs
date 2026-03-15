//! AI coding tool detection for `keel init`.

use std::path::Path;

/// Detected AI coding tool present in the repository.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectedTool {
    ClaudeCode,
    Cursor,
    GeminiCli,
    Windsurf,
    LettaCode,
    Codex,
    Antigravity,
    Aider,
    Copilot,
    GitHubActions,
}

impl DetectedTool {
    /// Human-readable name for display.
    pub fn name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::GeminiCli => "Gemini CLI",
            Self::Windsurf => "Windsurf",
            Self::LettaCode => "Letta Code",
            Self::Codex => "Codex",
            Self::Antigravity => "Antigravity",
            Self::Aider => "Aider",
            Self::Copilot => "GitHub Copilot",
            Self::GitHubActions => "GitHub Actions",
        }
    }

    /// All supported interactive agent variants (excludes GitHubActions — that's CI, not an agent).
    pub fn all_agents() -> &'static [DetectedTool] {
        &[
            Self::ClaudeCode,
            Self::Cursor,
            Self::GeminiCli,
            Self::Windsurf,
            Self::LettaCode,
            Self::Codex,
            Self::Antigravity,
            Self::Aider,
            Self::Copilot,
        ]
    }
}

/// Scan the repository root for AI coding tool directories and config files.
pub fn detect_tools(root: &Path) -> Vec<DetectedTool> {
    let mut tools = Vec::new();

    if root.join(".claude").is_dir() {
        tools.push(DetectedTool::ClaudeCode);
    }
    if root.join(".cursor").is_dir() {
        tools.push(DetectedTool::Cursor);
    }
    if root.join(".gemini").is_dir() || root.join("GEMINI.md").exists() {
        tools.push(DetectedTool::GeminiCli);
    }
    if root.join(".windsurf").is_dir() || root.join(".windsurfrules").exists() {
        tools.push(DetectedTool::Windsurf);
    }
    if root.join(".letta").is_dir() {
        tools.push(DetectedTool::LettaCode);
    }
    if root.join(".codex").is_dir() {
        tools.push(DetectedTool::Codex);
    }
    if root.join(".agent").is_dir() {
        tools.push(DetectedTool::Antigravity);
    }
    if root.join(".aider.conf.yml").exists() || root.join(".aider").is_dir() {
        tools.push(DetectedTool::Aider);
    }
    if root.join(".github/copilot-instructions.md").exists() {
        tools.push(DetectedTool::Copilot);
    }
    if root.join(".github/workflows").is_dir() {
        tools.push(DetectedTool::GitHubActions);
    }

    tools
}
