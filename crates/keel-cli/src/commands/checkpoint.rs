//! `keel checkpoint` — compact, compaction-resilient session-state summary.
//!
//! Derived entirely from git + the stored graph via the shared
//! [`keel_enforce::checkpoint`] core, so the CLI and MCP tool never diverge.

use std::fs;
use std::path::PathBuf;

use keel_enforce::checkpoint::{self, CheckpointMode};
use keel_enforce::engine::EnforcementEngine;
use keel_output::OutputFormatter;

use super::parse_util::parse_files_to_indices;

/// Run `keel checkpoint`.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    since: Option<String>,
    staged: bool,
    output: Option<String>,
) -> i32 {
    // Read handle for the diff/caller lookups.
    let (cwd, store) = match super::open_store("checkpoint") {
        Ok(x) => x,
        Err(code) => return code,
    };
    let keel_dir = keel_core::paths::keel_dir(&cwd);
    let db_path = keel_dir.join("graph.db");
    let db_str = db_path.to_str().unwrap_or("");

    // Separate handle owned by the enforcement engine (mirrors `keel compile`).
    let engine_store = match keel_core::sqlite::SqliteGraphStore::open(db_str) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel checkpoint: failed to open graph database: {}", e);
            return 2;
        }
    };
    let config = keel_core::config::KeelConfig::load(&keel_dir);
    let mut engine = EnforcementEngine::with_config(Box::new(engine_store), &config);

    let mode = if staged {
        CheckpointMode::Staged
    } else {
        CheckpointMode::Since(since)
    };

    let changed = checkpoint::changed_files(&cwd, &mode);
    if verbose {
        eprintln!("keel checkpoint: {} changed file(s)", changed.len());
    }

    let paths: Vec<PathBuf> = changed.iter().map(|f| cwd.join(f)).collect();
    let file_indices = parse_files_to_indices(&paths, &cwd);

    // Diff against the PRE-edit graph before compiling: `engine.compile`
    // persists re-baselined hashes to the same database, which would erase the
    // very change we want to report.
    let diff = checkpoint::diff_changed_files(&store, &file_indices);
    let compile_result = engine.compile(&file_indices);

    let commits = checkpoint::commit_subjects(&cwd, &mode);
    let range = checkpoint::range_label(&mode);
    let result = checkpoint::build_checkpoint(diff, &compile_result, range, commits);

    let rendered = formatter.format_checkpoint(&result);

    if let Some(path) = output {
        if let Err(e) = fs::write(&path, &rendered) {
            eprintln!("keel checkpoint: failed to write {}: {}", path, e);
            return 2;
        }
        if verbose {
            eprintln!("keel checkpoint: wrote checkpoint to {}", path);
        }
    } else if !rendered.is_empty() {
        println!("{}", rendered);
    }

    0
}
