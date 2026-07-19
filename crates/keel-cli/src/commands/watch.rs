//! `keel watch` — a thin wrapper over the shared watcher in keel-server.
//!
//! The event handling, ignore lists, debounce, incremental recompile, and
//! prune-on-delete all live in [`keel_server::watcher`]; this command just
//! opens the graph and runs that loop until Ctrl+C.

/// Run `keel watch` — watch source files and auto-compile (and prune deletions).
pub fn run(verbose: bool) -> i32 {
    let root = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[keel watch] failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&root);
    if !keel_dir.exists() {
        eprintln!("[keel watch] not initialized. Run `keel init` first.");
        return 2;
    }

    let engine = match keel_server::KeelServer::open(
        keel_dir
            .join("graph.db")
            .to_str()
            .unwrap_or(".keel/graph.db"),
        root.clone(),
    ) {
        Ok(server) => server.engine,
        Err(e) => {
            eprintln!("[keel watch] failed to open graph: {}", e);
            return 2;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[keel watch] failed to create runtime: {}", e);
            return 2;
        }
    };

    eprintln!("[keel watch] Watching for changes... (Ctrl+C to stop)");
    match rt.block_on(keel_server::watcher::watch(engine, root, verbose)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[keel watch] watcher error: {}", e);
            2
        }
    }
}
