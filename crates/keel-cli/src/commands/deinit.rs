use std::fs;

use keel_output::OutputFormatter;

/// Run `keel deinit` — remove all keel-generated files.
pub fn run(_formatter: &dyn OutputFormatter, _verbose: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel deinit: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel deinit: no .keel/ directory found — nothing to remove");
        return 0;
    }

    let _lock = match keel_core::graph_lock::acquire(&keel_dir, std::time::Duration::from_secs(2)) {
        Ok(lock) => Some(lock),
        Err(keel_core::graph_lock::GraphLockError::Busy) => {
            eprintln!("keel deinit: another keel process holds the graph lock, refusing");
            return 2;
        }
        Err(error) => {
            eprintln!("keel deinit: {error}; proceeding with cleanup");
            None
        }
    };

    match fs::remove_dir_all(&keel_dir) {
        Ok(_) => {
            eprintln!("keel deinit: removed {}", keel_dir.display());
            0
        }
        Err(e) => {
            eprintln!("keel deinit: failed to remove .keel/: {}", e);
            2
        }
    }
}
