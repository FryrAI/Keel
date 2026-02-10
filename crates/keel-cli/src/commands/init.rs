use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use keel_core::config::KeelConfig;
use keel_output::OutputFormatter;

/// Run `keel init` — detect languages, create .keel/ directory, write config.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel init: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if keel_dir.exists() {
        eprintln!("keel init: .keel/ directory already exists");
        return 2;
    }

    // Create .keel directory structure
    if let Err(e) = fs::create_dir_all(keel_dir.join("cache")) {
        eprintln!("keel init: failed to create .keel/cache: {}", e);
        return 2;
    }

    // Detect languages present in the repo
    let languages = detect_languages(&cwd);

    // Write config using the typed KeelConfig struct
    let config = KeelConfig {
        version: "0.1.0".to_string(),
        languages: languages.clone(),
        ..KeelConfig::default()
    };

    let config_path = cwd.join(".keel/keel.json");
    match fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("keel init: failed to write config: {}", e);
            return 2;
        }
    }

    // Create empty graph database
    let db_path = cwd.join(".keel/graph.db");
    match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("keel init: failed to create graph database: {}", e);
            return 2;
        }
    }

    // Fix 6: Install git pre-commit hook
    install_git_hook(&cwd, verbose);

    // Fix 7: Create .keelignore
    create_keelignore(&cwd, verbose);

    // Fix 8: Detect and configure tool integrations
    detect_tool_integrations(&cwd, verbose);

    if verbose {
        eprintln!(
            "keel init: initialized in {} with languages: {:?}",
            cwd.display(),
            languages
        );
    }

    let _ = formatter; // Will be used for JSON/LLM output in future
    0
}

/// Install a git pre-commit hook that runs `keel compile`.
fn install_git_hook(root: &Path, verbose: bool) {
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

    let hook_content = "#!/bin/sh\n# Installed by keel init\nkeel compile \"$@\"\n";
    match fs::write(&hook_path, hook_content) {
        Ok(_) => {
            // Make executable on Unix
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755));
            }
            if verbose {
                eprintln!("keel init: installed pre-commit hook");
            }
        }
        Err(e) => {
            eprintln!("keel init: warning: failed to install pre-commit hook: {}", e);
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

/// Detect tool directories and log what was found.
fn detect_tool_integrations(root: &Path, verbose: bool) {
    let tools = [
        (".cursor", "Cursor"),
        (".windsurf", "Windsurf"),
        (".aider", "Aider"),
        (".continue", "Continue"),
    ];

    for (dir, name) in &tools {
        if root.join(dir).exists() && verbose {
            eprintln!("keel init: detected {} integration ({})", name, dir);
        }
    }
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
