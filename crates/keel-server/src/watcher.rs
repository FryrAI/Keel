//! File watcher that triggers incremental compile on changes.
//!
//! Uses the `notify` crate with debouncing (100ms) to watch source files.
//! Ignores `.keel/`, `node_modules/`, `.git/`, and common build directories.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use keel_core::sqlite::SqliteGraphStore;

type SharedStore = Arc<Mutex<SqliteGraphStore>>;

/// Directories to ignore when watching for file changes.
const IGNORED_DIRS: &[&str] = &[
    ".keel",
    ".git",
    "node_modules",
    "__pycache__",
    "target",
    "dist",
    "build",
    ".next",
];

/// File extensions to watch.
const WATCHED_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "py", "go", "rs"];

/// Start watching the given root directory for file changes.
///
/// Returns a channel receiver that emits batches of changed file paths
/// (debounced at 100ms intervals).
pub fn start_watching(
    root: &Path,
    _store: SharedStore,
) -> Result<(RecommendedWatcher, mpsc::Receiver<Vec<PathBuf>>), notify::Error> {
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>(64);
    let root = root.to_path_buf();

    // Debounce: collect events for 100ms then flush
    let (event_tx, mut event_rx) = mpsc::channel::<PathBuf>(256);

    // Spawn debounce task
    tokio::spawn(async move {
        let mut batch: Vec<PathBuf> = Vec::new();
        let debounce = Duration::from_millis(100);

        loop {
            match tokio::time::timeout(debounce, event_rx.recv()).await {
                Ok(Some(path)) => {
                    if !batch.contains(&path) {
                        batch.push(path);
                    }
                }
                Ok(None) => break, // channel closed
                Err(_) => {
                    // Timeout — flush batch
                    if !batch.is_empty() {
                        let flushed = std::mem::take(&mut batch);
                        if tx.send(flushed).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Create the file watcher
    let root_clone = root.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                for path in event.paths {
                    if should_watch(&root_clone, &path) {
                        let _ = event_tx.blocking_send(path);
                    }
                }
            }
        }
    })?;

    watcher.watch(&root, RecursiveMode::Recursive)?;

    Ok((watcher, rx))
}

/// Check if a path should trigger a recompile.
fn should_watch(root: &Path, path: &Path) -> bool {
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| WATCHED_EXTENSIONS.contains(&e))
        .unwrap_or(false);

    if !ext_ok {
        return false;
    }

    if let Ok(rel) = path.strip_prefix(root) {
        for component in rel.components() {
            if let std::path::Component::Normal(name) = component {
                if let Some(name_str) = name.to_str() {
                    if IGNORED_DIRS.contains(&name_str) {
                        return false;
                    }
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_should_watch_valid_file() {
        let root = PathBuf::from("/project");
        assert!(should_watch(&root, &PathBuf::from("/project/src/foo.ts")));
        assert!(should_watch(&root, &PathBuf::from("/project/lib/bar.py")));
        assert!(should_watch(&root, &PathBuf::from("/project/main.rs")));
    }

    #[test]
    fn test_should_ignore_wrong_extension() {
        let root = PathBuf::from("/project");
        assert!(!should_watch(&root, &PathBuf::from("/project/src/foo.md")));
        assert!(!should_watch(&root, &PathBuf::from("/project/img.png")));
    }

    #[test]
    fn test_should_ignore_excluded_dirs() {
        let root = PathBuf::from("/project");
        assert!(!should_watch(
            &root,
            &PathBuf::from("/project/node_modules/foo.ts")
        ));
        assert!(!should_watch(
            &root,
            &PathBuf::from("/project/.git/hooks/pre-commit.py")
        ));
        assert!(!should_watch(
            &root,
            &PathBuf::from("/project/target/debug/main.rs")
        ));
    }
}
