use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_enforce::engine::EnforcementEngine;
use keel_output::OutputFormatter;

use crate::commands::parse_util;

/// Run `keel serve` — start persistent server (MCP/HTTP/watch).
/// Delegates to keel-server crate.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    mcp: bool,
    http: bool,
    watch: bool,
) -> i32 {
    if !mcp && !http && !watch {
        eprintln!("keel serve: at least one of --mcp, --http, or --watch required");
        return 2;
    }

    if verbose {
        let mut modes = Vec::new();
        if mcp {
            modes.push("MCP");
        }
        if http {
            modes.push("HTTP");
        }
        if watch {
            modes.push("watch");
        }
        eprintln!("keel serve: starting with modes: {}", modes.join(", "));
    }

    // Resolve project root and database path
    let root_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = root_dir.join(".keel").join("graph.db");

    // MCP mode runs synchronously over stdio — no tokio needed
    if mcp && !http && !watch {
        let store = match SqliteGraphStore::open(db_path.to_str().unwrap_or(".keel/graph.db")) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("keel serve: failed to open store: {}", e);
                return 2;
            }
        };
        let shared_store = Arc::new(Mutex::new(store));
        let db_str = db_path.to_string_lossy().to_string();
        if let Err(e) = keel_server::mcp::run_stdio(shared_store, Some(&db_str)) {
            eprintln!("keel serve: MCP error: {}", e);
            return 2;
        }
        return 0;
    }

    // HTTP and/or watch modes require tokio
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("keel serve: failed to create runtime: {}", e);
            return 2;
        }
    };

    rt.block_on(async {
        let server = match keel_server::KeelServer::open(
            db_path.to_str().unwrap_or(".keel/graph.db"),
            root_dir.clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("keel serve: failed to open store: {}", e);
                return 2;
            }
        };

        if watch {
            // Watcher needs a raw store — open a separate connection
            let watch_store =
                match SqliteGraphStore::open(db_path.to_str().unwrap_or(".keel/graph.db")) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("keel serve: failed to open watch store: {}", e);
                        return 2;
                    }
                };
            let shared_watch = Arc::new(Mutex::new(watch_store));
            let watch_root = root_dir.clone();

            // Create enforcement engine for watch mode (with project config)
            let watch_engine_store =
                match SqliteGraphStore::open(db_path.to_str().unwrap_or(".keel/graph.db")) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("keel serve: failed to open engine store: {}", e);
                        return 2;
                    }
                };
            let keel_dir = root_dir.join(".keel");
            let config = keel_core::config::KeelConfig::load(&keel_dir);
            let shared_engine: Arc<Mutex<EnforcementEngine>> = Arc::new(Mutex::new(
                EnforcementEngine::with_config(Box::new(watch_engine_store), &config),
            ));

            match keel_server::watcher::start_watching(&root_dir, shared_watch) {
                Ok((_watcher, mut rx)) => {
                    if verbose {
                        eprintln!("keel serve: file watcher started on {:?}", root_dir);
                    }
                    tokio::spawn(async move {
                        while let Some(changed) = rx.recv().await {
                            eprintln!("keel: {} file(s) changed, recompiling...", changed.len());
                            // Parse changed files and run incremental compile
                            let file_indices =
                                parse_util::parse_files_to_indices(&changed, &watch_root);
                            if !file_indices.is_empty() {
                                let mut engine = shared_engine.lock().unwrap();
                                let result = engine.compile(&file_indices);
                                if !result.errors.is_empty() || !result.warnings.is_empty() {
                                    eprintln!(
                                        "keel: {} error(s), {} warning(s)",
                                        result.errors.len(),
                                        result.warnings.len()
                                    );
                                    for v in &result.errors {
                                        eprintln!(
                                            "  {} {} {}:{} — {}",
                                            v.code, v.severity, v.file, v.line, v.message
                                        );
                                    }
                                } else {
                                    eprintln!("keel: clean compile");
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("keel serve: watcher failed: {}", e);
                    return 2;
                }
            }
        }

        if http {
            let port = 4815;
            if verbose {
                eprintln!("keel serve: HTTP on http://127.0.0.1:{}", port);
            }
            if let Err(e) = keel_server::http::serve(server.engine, port).await {
                eprintln!("keel serve: HTTP error: {}", e);
                return 2;
            }
        }

        0
    })
}
