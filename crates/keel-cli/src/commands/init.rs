//! `keel init` command — detect languages, create .keel/ directory, write config,
//! detect AI coding tools, and generate appropriate hook configs and instruction files.

mod generators;
mod hook_script;
mod merge;
mod templates;

use std::fs;
use std::path::Path;

use keel_core::config::KeelConfig;
use keel_output::OutputFormatter;

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
    if root.join(".github/copilot-instructions.md").exists() || root.join(".github").is_dir() {
        tools.push(DetectedTool::Copilot);
    }
    if root.join(".github/workflows").is_dir() {
        tools.push(DetectedTool::GitHubActions);
    }

    tools
}

/// Run `keel init` — detect languages, create .keel/ directory, write config,
/// detect tools, and generate configs.
///
/// When `merge` is true and `.keel/` already exists, re-initialize while
/// preserving existing configuration (deep-merged with new defaults).
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, merge: bool, yes: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel init: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if keel_dir.exists() && !merge {
        eprintln!("keel init: .keel/ directory already exists (use --merge to re-initialize)");
        return 2;
    }

    // Create .keel directory structure
    if let Err(e) = fs::create_dir_all(keel_dir.join("cache")) {
        eprintln!("keel init: failed to create .keel/cache: {}", e);
        return 2;
    }

    // Detect languages present in the repo
    let languages = detect_languages(&cwd);

    // Detect monorepo layout
    let layout = keel_parsers::monorepo::detect_monorepo(&cwd);
    let monorepo_config = if layout.kind != keel_parsers::monorepo::MonorepoKind::None {
        keel_core::config::MonorepoConfig {
            enabled: true,
            kind: Some(format!("{:?}", layout.kind)),
            packages: layout.packages.iter().map(|p| p.name.clone()).collect(),
        }
    } else {
        keel_core::config::MonorepoConfig::default()
    };

    let config_path = cwd.join(".keel/keel.json");

    if merge && config_path.exists() {
        // Merge mode: read existing config and deep-merge with new defaults
        let existing_json = fs::read_to_string(&config_path).unwrap_or_default();
        let existing: serde_json::Value = serde_json::from_str(&existing_json)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let new_config = KeelConfig {
            version: "0.1.0".to_string(),
            languages: languages.clone(),
            monorepo: monorepo_config.clone(),
            ..KeelConfig::default()
        };
        let new_json: serde_json::Value = serde_json::to_value(&new_config)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        // Deep merge: new values fill in missing keys, existing values preserved
        let merged = merge::json_deep_merge(&new_json, &existing);
        match fs::write(&config_path, serde_json::to_string_pretty(&merged).unwrap()) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("keel init: failed to write merged config: {}", e);
                return 2;
            }
        }
        if verbose {
            eprintln!("keel init --merge: config merged");
        }
    } else {
        // Fresh init: write new config
        let config = KeelConfig {
            version: "0.1.0".to_string(),
            languages: languages.clone(),
            monorepo: monorepo_config.clone(),
            ..KeelConfig::default()
        };
        match fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("keel init: failed to write config: {}", e);
                return 2;
            }
        }
    }

    // Open (or create) the graph database.
    // On merge: reset circuit breaker state.
    let db_path = cwd.join(".keel/graph.db");
    match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(store) => {
            if merge {
                // Reset circuit breaker state on merge
                if let Err(e) = store.save_circuit_breaker(&[]) {
                    if verbose {
                        eprintln!(
                            "keel init --merge: warning: failed to reset circuit breaker: {}",
                            e
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("keel init: failed to create graph database: {}", e);
            return 2;
        }
    }

    // Install hooks
    hook_script::install_git_hook(&cwd, verbose);
    hook_script::install_post_edit_hook(&cwd, verbose);

    // Create .keelignore
    create_keelignore(&cwd, verbose);

    // Update .gitignore with keel entries
    update_gitignore(&cwd, verbose);

    // Detect and generate tool configs
    let detected_tools = detect_tools(&cwd);
    let mut tool_file_count = 0;

    // Build the list of agent tools to generate configs for
    let selected_tools: Vec<&DetectedTool> = if yes {
        // --yes: skip prompt, use detected agents only
        detected_tools
            .iter()
            .filter(|t| **t != DetectedTool::GitHubActions)
            .collect()
    } else {
        // Interactive multi-select: all agents listed, detected ones pre-checked
        let all_agents = DetectedTool::all_agents();
        let defaults: Vec<bool> = all_agents
            .iter()
            .map(|t| detected_tools.contains(t))
            .collect();

        let items: Vec<&str> = all_agents.iter().map(|t| t.name()).collect();

        let selections = dialoguer::MultiSelect::new()
            .with_prompt("Select agents to generate hook configs for")
            .items(&items)
            .defaults(&defaults)
            .interact()
            .unwrap_or_else(|_| {
                // Non-interactive (piped stdin) — fall back to detected agents only
                all_agents
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| detected_tools.contains(t))
                    .map(|(i, _)| i)
                    .collect()
            });

        selections.iter().map(|&i| &all_agents[i]).collect()
    };

    for tool in &selected_tools {
        let files = match tool {
            DetectedTool::ClaudeCode => generators::generate_claude_code(&cwd),
            DetectedTool::Cursor => generators::generate_cursor(&cwd),
            DetectedTool::GeminiCli => generators::generate_gemini_cli(&cwd),
            DetectedTool::Windsurf => generators::generate_windsurf(&cwd),
            DetectedTool::LettaCode => generators::generate_letta_code(&cwd),
            DetectedTool::Antigravity => generators::generate_antigravity(&cwd),
            DetectedTool::Aider => generators::generate_aider(&cwd),
            DetectedTool::Copilot => generators::generate_copilot(&cwd),
            DetectedTool::Codex => generators::generate_codex(&cwd),
            DetectedTool::GitHubActions => generators::generate_github_actions(&cwd),
        };
        tool_file_count += generators::write_files(&files, verbose);
    }

    // GitHub Actions is CI, not an interactive agent — generate if detected, regardless of prompt
    if detected_tools.contains(&DetectedTool::GitHubActions) {
        let files = generators::generate_github_actions(&cwd);
        tool_file_count += generators::write_files(&files, verbose);
    }

    // Always generate AGENTS.md (universal fallback)
    let agents_files = generators::generate_agents_md(&cwd);
    tool_file_count += generators::write_files(&agents_files, verbose);

    // Count files for the summary
    let file_count = keel_parsers::walker::FileWalker::new(&cwd).walk().len();

    eprintln!(
        "keel initialized. {} language(s) detected, {} files indexed.",
        languages.len(),
        file_count
    );

    if monorepo_config.enabled {
        eprintln!(
            "  monorepo: {} ({} packages)",
            monorepo_config.kind.as_deref().unwrap_or("unknown"),
            monorepo_config.packages.len()
        );
        if verbose {
            for pkg in &monorepo_config.packages {
                eprintln!("    - {}", pkg);
            }
        }
    }

    if !selected_tools.is_empty() {
        let names: Vec<&str> = selected_tools.iter().map(|t| t.name()).collect();
        eprintln!("  agent configs generated: {}", names.join(", "));
        eprintln!("  {} config file(s) written", tool_file_count);
    }

    if verbose {
        eprintln!("  languages: {:?}", languages);
        eprintln!("  config: .keel/keel.json");
        eprintln!("  database: .keel/graph.db");
    }

    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  keel map       Build the structural graph");
    eprintln!("  keel compile   Validate contracts");
    eprintln!();
    eprintln!("Telemetry is enabled by default (privacy-safe, no code/paths collected).");
    eprintln!("  Opt out: keel config telemetry.remote false");
    eprintln!();
    eprintln!("Tip: If keel saves you time \u{2192}  gh star FryrAI/Keel");

    let _ = formatter; // Will be used for JSON/LLM output in future
    0
}

/// Detect which languages are present by scanning file extensions.
fn detect_languages(root: &Path) -> Vec<String> {
    let mut langs = std::collections::HashSet::new();

    let walker = keel_parsers::walker::FileWalker::new(root);
    for entry in walker.walk() {
        if !langs.contains(&entry.language) {
            langs.insert(entry.language);
        }
    }

    let mut result: Vec<String> = langs.into_iter().collect();
    result.sort();
    result
}

/// Ensure `.gitignore` contains keel-specific entries.
fn update_gitignore(root: &Path, verbose: bool) {
    let gitignore_path = root.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();

    let entries = [
        ".keel/graph.db",
        ".keel/telemetry.db",
        ".keel/session.json",
        ".keel/cache/",
    ];

    let mut missing: Vec<&str> = Vec::new();
    for entry in &entries {
        if !existing.lines().any(|line| line.trim() == *entry) {
            missing.push(entry);
        }
    }

    if missing.is_empty() {
        return;
    }

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str("\n# keel (generated by keel init)\n");
    for entry in &missing {
        content.push_str(entry);
        content.push('\n');
    }

    match fs::write(&gitignore_path, content) {
        Ok(_) => {
            if verbose {
                eprintln!(
                    "keel init: updated .gitignore with {} keel entries",
                    missing.len()
                );
            }
        }
        Err(e) => {
            eprintln!("keel init: warning: failed to update .gitignore: {}", e);
        }
    }
}

/// Create a default .keelignore file if one doesn't exist.
fn create_keelignore(root: &Path, verbose: bool) {
    let ignore_path = root.join(".keelignore");
    if ignore_path.exists() {
        return;
    }

    let default_patterns = "\
node_modules/
__pycache__/
target/
dist/
build/
.next/
vendor/
.venv/
";

    match fs::write(&ignore_path, default_patterns) {
        Ok(_) => {
            if verbose {
                eprintln!("keel init: created .keelignore");
            }
        }
        Err(e) => {
            eprintln!("keel init: warning: failed to create .keelignore: {}", e);
        }
    }
}
