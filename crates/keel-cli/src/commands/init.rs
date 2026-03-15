//! `keel init` command — detect languages, create .keel/ directory, write config,
//! detect AI coding tools, and generate appropriate hook configs and instruction files.

pub(crate) mod detection;
mod generators;
mod helpers;
mod hook_script;
mod merge;
mod templates;

use std::fs;

use keel_core::config::KeelConfig;
use keel_output::OutputFormatter;

pub use detection::{detect_tools, DetectedTool};
use helpers::{create_keelignore, detect_languages, generate_telemetry_id, update_gitignore};

/// Selected enforcement hooks to install during init.
#[derive(Debug, Clone)]
pub struct HookSelection {
    pub session_start: bool,
    pub pre_commit: bool,
    pub pre_commit_audit: bool,
    pub on_edit: bool,
}

impl Default for HookSelection {
    fn default() -> Self {
        Self {
            session_start: true,
            pre_commit: true,
            pre_commit_audit: true,
            on_edit: false,
        }
    }
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
            version: env!("CARGO_PKG_VERSION").to_string(),
            languages: languages.clone(),
            monorepo: monorepo_config.clone(),
            telemetry_id: Some(generate_telemetry_id(&cwd)),
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
            version: env!("CARGO_PKG_VERSION").to_string(),
            languages: languages.clone(),
            monorepo: monorepo_config.clone(),
            telemetry_id: Some(generate_telemetry_id(&cwd)),
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

    // Hook selection: which enforcement hooks to enable
    let hook_selection = if yes {
        HookSelection::default()
    } else {
        let hook_items = [
            "Session start \u{2014} inject structural map on session start (fast, recommended)",
            "Pre-commit \u{2014} validate on git commit (recommended)",
            "Pre-commit audit \u{2014} AI-readiness scorecard on commit",
            "On-edit \u{2014} validate after every file edit (accurate but slower, may cause issues)",
        ];
        let hook_defaults = [true, true, true, false];

        let hook_selections = dialoguer::MultiSelect::new()
            .with_prompt("Select enforcement hooks")
            .items(&hook_items)
            .defaults(&hook_defaults)
            .interact()
            .unwrap_or_else(|_| vec![0, 1, 2]); // Non-interactive default

        HookSelection {
            session_start: hook_selections.contains(&0),
            pre_commit: hook_selections.contains(&1),
            pre_commit_audit: hook_selections.contains(&2),
            on_edit: hook_selections.contains(&3),
        }
    };

    if hook_selection.on_edit {
        eprintln!(
            "  \u{26a0} On-edit hooks enabled. This runs keel compile after every file edit."
        );
        eprintln!("    If sessions become slow, re-run `keel init` and deselect this option.");
    }

    // Install hooks based on selection
    hook_script::install_git_hook(&cwd, verbose, &hook_selection);
    if hook_selection.on_edit {
        hook_script::install_post_edit_hook(&cwd, verbose);
    }

    for tool in &selected_tools {
        let files = match tool {
            DetectedTool::ClaudeCode => generators::generate_claude_code(&cwd, &hook_selection),
            DetectedTool::Cursor => generators::generate_cursor(&cwd, &hook_selection),
            DetectedTool::GeminiCli => generators::generate_gemini_cli(&cwd, &hook_selection),
            DetectedTool::Windsurf => generators::generate_windsurf(&cwd, &hook_selection),
            DetectedTool::LettaCode => generators::generate_letta_code(&cwd, &hook_selection),
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
    eprintln!("  Opt out: keel --no-telemetry <command>, KEEL_NO_TELEMETRY=1, or");
    eprintln!("           keel config telemetry.remote false");
    eprintln!();
    eprintln!("Tip: If keel saves you time \u{2192}  gh star FryrAI/Keel");

    let _ = formatter; // Will be used for JSON/LLM output in future
    0
}
