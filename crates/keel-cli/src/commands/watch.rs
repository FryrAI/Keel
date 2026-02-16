use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};

const WATCHED_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "py", "go", "rs"];
const IGNORED_DIRS: &[&str] = &[".keel", ".git", "node_modules", "__pycache__", "target", "dist", "build"];
const DEBOUNCE_MS: u64 = 200;

fn is_watched(path: &Path) -> bool {
    // Reject paths containing ignored directories
    for component in path.components() {
        if let std::path::Component::Normal(s) = component {
            if IGNORED_DIRS.contains(&s.to_str().unwrap_or("")) {
                return false;
            }
        }
    }
    // Accept only files with watched extensions
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| WATCHED_EXTENSIONS.contains(&e))
        .unwrap_or(false)
}

/// Run `keel watch` -- watch source files and auto-compile on changes.
pub fn run(verbose: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[keel watch] failed to get current directory: {}", e);
            return 2;
        }
    };

    if !cwd.join(".keel").exists() {
        eprintln!("[keel watch] not initialized. Run `keel init` first.");
        return 2;
    }

    let (tx, rx) = mpsc::channel::<Event>();
    let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[keel watch] failed to create watcher: {}", e);
            return 2;
        }
    };

    if let Err(e) = watcher.watch(&cwd, RecursiveMode::Recursive) {
        eprintln!("[keel watch] failed to watch directory: {}", e);
        return 2;
    }

    let mut total_compiles = 0u32;
    eprintln!("[keel watch] Watching for changes... (Ctrl+C to stop)");

    let run_compile = |files: &[String], verbose: bool| -> bool {
        eprintln!("[keel watch] Compiling: {}", files.join(" "));
        let mut cmd = std::process::Command::new("keel");
        cmd.arg("compile").arg("--delta");
        if verbose {
            cmd.arg("--verbose");
        }
        cmd.args(files);
        match cmd.status() {
            Ok(status) => status.success(),
            Err(e) => {
                eprintln!("[keel watch] failed to run keel compile: {}", e);
                false
            }
        }
    };

    while let Ok(event) = rx.recv() {
        let mut changed = std::collections::HashSet::new();
        // Collect paths from first event
        for p in &event.paths {
            if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) && is_watched(p) {
                if let Some(s) = p.to_str() {
                    changed.insert(s.to_string());
                }
            }
        }

        // Debounce: drain events for DEBOUNCE_MS
        while let Ok(ev) = rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS)) {
            if matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                for p in &ev.paths {
                    if is_watched(p) {
                        if let Some(s) = p.to_str() {
                            changed.insert(s.to_string());
                        }
                    }
                }
            }
        }

        if !changed.is_empty() {
            let files: Vec<String> = changed.into_iter().collect();
            run_compile(&files, verbose);
            total_compiles += 1;
        }
    }

    eprintln!("[keel watch] Stopped. Total compiles: {}", total_compiles);
    0
}
