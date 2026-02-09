use keel_output::OutputFormatter;

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
        if mcp { modes.push("MCP"); }
        if http { modes.push("HTTP"); }
        if watch { modes.push("watch"); }
        eprintln!("keel serve: starting with modes: {}", modes.join(", "));
    }

    // Resolve project root and database path
    let root_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = root_dir.join(".keel").join("graph.db");

    // MCP mode runs synchronously over stdio — no tokio needed
    if mcp && !http && !watch {
        let server = match keel_server::KeelServer::open(
            db_path.to_str().unwrap_or(".keel/graph.db"),
            root_dir,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("keel serve: failed to open store: {}", e);
                return 2;
            }
        };
        if let Err(e) = keel_server::mcp::run_stdio(server.store) {
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
            match keel_server::watcher::start_watching(&root_dir, server.store.clone()) {
                Ok((_watcher, mut rx)) => {
                    if verbose {
                        eprintln!("keel serve: file watcher started on {:?}", root_dir);
                    }
                    // Spawn watcher consumer
                    tokio::spawn(async move {
                        while let Some(changed) = rx.recv().await {
                            eprintln!(
                                "keel: {} file(s) changed, recompiling...",
                                changed.len()
                            );
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
            if let Err(e) = keel_server::http::serve(server.store, port).await {
                eprintln!("keel serve: HTTP error: {}", e);
                return 2;
            }
        }

        0
    })
}
