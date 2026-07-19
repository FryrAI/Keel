use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_output::OutputFormatter;

const HTTP_PORT: u16 = 4815;

/// Which components a `keel serve` flag combination selects.
///
/// Every requested component runs concurrently; this only records *which* run.
/// Pure and unit-tested for every combination so no combination can silently
/// drop a component (issue #35: combined invocations used to drop MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ServePlan {
    mcp: bool,
    http: bool,
    watch: bool,
}

impl ServePlan {
    /// Build a plan, or `None` when no component was requested.
    fn new(mcp: bool, http: bool, watch: bool) -> Option<Self> {
        if !mcp && !http && !watch {
            None
        } else {
            Some(Self { mcp, http, watch })
        }
    }

    /// MCP alone runs as a synchronous stdio loop with no tokio runtime.
    /// Any other combination (including MCP + others) takes the async path so
    /// every requested component actually runs.
    fn is_stdio_only(&self) -> bool {
        self.mcp && !self.http && !self.watch
    }
}

/// Run `keel serve` — start the requested persistent components (MCP/HTTP/watch).
///
/// Every combination runs all requested components concurrently; none is
/// silently dropped.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    mcp: bool,
    http: bool,
    watch: bool,
    no_telemetry: bool,
) -> i32 {
    let plan = match ServePlan::new(mcp, http, watch) {
        Some(p) => p,
        None => {
            eprintln!("keel serve: at least one of --mcp, --http, or --watch required");
            return 2;
        }
    };

    if verbose {
        let mut modes = Vec::new();
        if plan.mcp {
            modes.push("MCP");
        }
        if plan.http {
            modes.push("HTTP");
        }
        if plan.watch {
            modes.push("watch");
        }
        eprintln!("keel serve: starting with modes: {}", modes.join(", "));
    }

    let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let db_path = root_dir.join(".keel").join("graph.db");

    // Fast path: MCP alone is a synchronous stdio loop — no tokio needed.
    if plan.is_stdio_only() {
        return run_mcp_stdio(&root_dir, &db_path, no_telemetry);
    }

    // Any other combination needs tokio (HTTP server and/or async watcher) and
    // may still include MCP alongside them.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("keel serve: failed to create runtime: {}", e);
            return 2;
        }
    };

    rt.block_on(run_async(plan, root_dir, db_path, verbose, no_telemetry))
}

/// Run every requested component concurrently in one task.
///
/// A `select!` over the components means the first to finish (MCP client
/// disconnects, HTTP errors out) ends the session and the rest are dropped.
/// Disabled components resolve never, so they simply never win. Running them in
/// a single task (rather than `tokio::spawn`) sidesteps `Send` bounds on the
/// HTTP error type and the watcher handle.
async fn run_async(
    plan: ServePlan,
    root_dir: PathBuf,
    db_path: PathBuf,
    verbose: bool,
    no_telemetry: bool,
) -> i32 {
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

    let watch_fut = async {
        if plan.watch {
            if verbose {
                eprintln!("keel serve: file watcher started on {:?}", root_dir);
            }
            if let Err(e) =
                keel_server::watcher::watch(server.engine.clone(), root_dir.clone(), verbose).await
            {
                eprintln!("keel serve: watcher error: {}", e);
                return 2;
            }
            0
        } else {
            std::future::pending::<i32>().await
        }
    };

    let http_fut = async {
        if plan.http {
            if verbose {
                eprintln!("keel serve: HTTP on http://127.0.0.1:{}", HTTP_PORT);
            }
            match keel_server::http::serve(server.engine.clone(), HTTP_PORT).await {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("keel serve: HTTP error: {}", e);
                    2
                }
            }
        } else {
            std::future::pending::<i32>().await
        }
    };

    // MCP stdio is a blocking read loop — run it on a blocking thread so it
    // never stalls the HTTP server or watcher on the runtime's worker pool.
    let mcp_fut = async {
        if plan.mcp {
            let root = root_dir.clone();
            let db = db_path.clone();
            tokio::task::spawn_blocking(move || run_mcp_stdio(&root, &db, no_telemetry))
                .await
                .unwrap_or_else(|e| {
                    eprintln!("keel serve: MCP task panicked: {}", e);
                    2
                })
        } else {
            std::future::pending::<i32>().await
        }
    };

    tokio::select! {
        code = watch_fut => code,
        code = http_fut => code,
        code = mcp_fut => code,
    }
}

/// Open the store and run the synchronous MCP stdio loop. Returns an exit code.
fn run_mcp_stdio(root_dir: &Path, db_path: &Path, no_telemetry: bool) -> i32 {
    let store = match SqliteGraphStore::open(db_path.to_str().unwrap_or(".keel/graph.db")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel serve: failed to open store: {}", e);
            return 2;
        }
    };
    let shared_store = Arc::new(Mutex::new(store));
    let db_str = db_path.to_string_lossy().to_string();
    let keel_dir = root_dir.join(".keel");
    if let Err(e) = keel_server::mcp_stdio::run_stdio(
        shared_store,
        Some(&db_str),
        Some(&keel_dir),
        no_telemetry,
    ) {
        eprintln!("keel serve: MCP error: {}", e);
        return 2;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flags_selects_nothing() {
        assert_eq!(ServePlan::new(false, false, false), None);
    }

    #[test]
    fn every_combination_keeps_all_requested_components() {
        // The plan must preserve each flag exactly, so no requested component is
        // ever dropped by a combined invocation (issue #35).
        for mcp in [false, true] {
            for http in [false, true] {
                for watch in [false, true] {
                    match ServePlan::new(mcp, http, watch) {
                        None => assert!(!mcp && !http && !watch),
                        Some(plan) => {
                            assert_eq!(plan.mcp, mcp);
                            assert_eq!(plan.http, http);
                            assert_eq!(plan.watch, watch);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn only_mcp_alone_uses_the_stdio_fast_path() {
        assert!(ServePlan::new(true, false, false).unwrap().is_stdio_only());
        // MCP combined with anything else must take the async path so the other
        // component actually runs (issue #35: combos silently dropped MCP).
        assert!(!ServePlan::new(true, true, false).unwrap().is_stdio_only());
        assert!(!ServePlan::new(true, false, true).unwrap().is_stdio_only());
        assert!(!ServePlan::new(false, true, false).unwrap().is_stdio_only());
        assert!(!ServePlan::new(false, false, true).unwrap().is_stdio_only());
    }
}
