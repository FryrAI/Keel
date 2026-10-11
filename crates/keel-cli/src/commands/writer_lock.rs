//! CLI writer boundaries; the lock mechanism lives in keel-core.

use std::path::Path;
use std::time::Duration;

use keel_core::graph_lock::{self, GraphLock, GraphLockError};

/// The generated post-edit hook matches this line to report an unchecked edit.
pub(super) const LOCK_SKIP: &str =
    "keel compile: another keel process holds the graph lock, skipping";

/// Hold the graph lock for a CLI writer, preserving compile's skip-success policy.
/// `graph_dir` must exist (see [`super::ensure_graph_dir`]).
pub(super) fn acquire(cmd: &str, graph_dir: &Path) -> Result<GraphLock, i32> {
    let timeout = Duration::from_secs(if cmd == "map" { 10 } else { 2 });
    graph_lock::acquire(graph_dir, timeout).map_err(|error| {
        match error {
            GraphLockError::Busy if cmd == "compile" => {
                eprintln!("{LOCK_SKIP}");
                return 0;
            }
            GraphLockError::Busy if cmd == "map" => eprintln!(
                "keel map: failed to acquire graph lock within 10s: {}",
                graph_dir.join("compile.lock").display()
            ),
            GraphLockError::Busy => {
                eprintln!("keel {cmd}: another keel process holds the graph lock, refusing")
            }
            GraphLockError::Io(e) => eprintln!("keel {cmd}: graph lock I/O error: {e}"),
        }
        2
    })
}

/// Resolve and acquire before a command opens its writer store.
pub(super) fn for_command(cmd: &str) -> Result<GraphLock, i32> {
    let cwd = std::env::current_dir().map_err(|e| {
        eprintln!("keel {cmd}: failed to get current directory: {e}");
        2
    })?;
    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel {cmd}: not initialized. Run `keel init` first.");
        return Err(2);
    }
    acquire(cmd, &super::ensure_graph_dir(cmd, &cwd)?)
}
