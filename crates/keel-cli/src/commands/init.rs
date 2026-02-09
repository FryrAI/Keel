use std::fs;
use std::path::Path;

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

    // Write config
    let config = serde_json::json!({
        "version": "0.1.0",
        "languages": languages,
        "enforce": {
            "type_hints": true,
            "docstrings": true,
            "placement": true,
        }
    });

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
