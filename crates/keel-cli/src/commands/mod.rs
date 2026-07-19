pub mod analyze;
pub mod audit;
pub mod call_resolve;
pub mod check;
pub mod checkpoint;
pub mod compile;
pub mod compile_lock;
pub mod compile_metrics;
pub mod compile_sync;
pub mod completion;
pub mod config;
pub mod context;
pub mod deinit;
pub mod discover;
pub mod explain;
pub mod fix;
pub mod focus;
pub mod init;
pub mod input_detect;
pub(crate) mod json_helpers;
pub mod login;
pub mod logout;
pub mod map;
pub mod map_baml;
pub mod map_cached;
pub mod map_helpers;
pub mod map_lang_resolve;
pub(crate) mod map_passes;
pub mod map_resolve;
pub mod map_tier3;
pub mod name;
pub mod parse_util;
pub mod push;
pub mod search;
pub mod serve;
pub mod skeleton;
pub mod stats;
pub mod upgrade;
pub mod validate_plan;
pub mod watch;
pub mod where_cmd;

/// Resolve the current directory and open the repo's graph database for `cmd`.
///
/// Centralizes the "resolve cwd -> locate the worktree-aware `.keel` dir -> open
/// `graph.db`" preamble that every graph-backed command shares. On any failure it
/// prints the same `keel <cmd>: ...` diagnostic the commands printed inline and
/// returns `Err(2)` (keel's internal-error exit code). Callers bind:
/// `let (cwd, store) = match commands::open_store("cmd") { Ok(x) => x, Err(code) => return code };`
pub(crate) fn open_store(
    cmd: &str,
) -> Result<(std::path::PathBuf, keel_core::sqlite::SqliteGraphStore), i32> {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel {cmd}: failed to get current directory: {e}");
            return Err(2);
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel {cmd}: not initialized. Run `keel init` first.");
        return Err(2);
    }

    let db_path = keel_dir.join("graph.db");
    match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(store) => Ok((cwd, store)),
        Err(e) => {
            eprintln!("keel {cmd}: failed to open graph database: {e}");
            Err(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    /// Serializes the tests that mutate the process-wide current directory.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn open_store_errors_when_uninitialized() {
        // A fresh temp dir has no `.keel`, so open_store must fail with exit 2.
        let dir = tempfile::tempdir().unwrap();
        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = super::open_store("test");
        std::env::set_current_dir(prev).unwrap();
        assert!(matches!(result, Err(2)));
    }
}
