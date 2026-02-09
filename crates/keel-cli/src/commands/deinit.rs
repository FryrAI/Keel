use std::fs;

use keel_output::OutputFormatter;

/// Run `keel deinit` — remove all keel-generated files.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel deinit: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel deinit: no .keel/ directory found — nothing to remove");
        return 0;
    }

    match fs::remove_dir_all(&keel_dir) {
        Ok(_) => {
            if verbose {
                eprintln!("keel deinit: removed {}", keel_dir.display());
            }
            0
        }
        Err(e) => {
            eprintln!("keel deinit: failed to remove .keel/: {}", e);
            2
        }
    }
}
