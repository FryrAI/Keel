//! MCP and HTTP server for keel.
//!
//! Provides two server modes:
//! - **MCP** (`keel serve --mcp`): Model Context Protocol over stdin/stdout for IDE integration
//! - **HTTP** (`keel serve --http`): REST API on localhost for tooling integration
//!
//! Also includes a file watcher for automatic re-compilation on changes.

pub mod http;
pub mod mcp;
mod mcp_analyze;
mod mcp_check;
mod mcp_compile;
mod mcp_context;
mod mcp_fix;
mod mcp_name;
mod mcp_search;
mod parse_shared;
pub mod watcher;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;
use keel_enforce::engine::EnforcementEngine;

use crate::http::SharedEngine;

/// Shared server state wrapping the enforcement engine.
///
/// Uses `std::sync::Mutex` because `rusqlite::Connection` is `!Send`.
/// All DB access goes through `engine.lock()` — keep critical sections short.
pub struct KeelServer {
    pub engine: SharedEngine,
    pub root_dir: PathBuf,
}

impl KeelServer {
    /// Create a new server instance from an existing database path.
    pub fn open(db_path: &str, root_dir: PathBuf) -> Result<Self, keel_core::types::GraphError> {
        let store = SqliteGraphStore::open(db_path)?;
        let keel_dir = root_dir.join(".keel");
        let config = keel_core::config::KeelConfig::load(&keel_dir);
        let engine = EnforcementEngine::with_config(Box::new(store), &config);
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            root_dir,
        })
    }

    /// Create a server with an in-memory store (testing).
    pub fn in_memory(root_dir: PathBuf) -> Result<Self, keel_core::types::GraphError> {
        let store = SqliteGraphStore::in_memory()?;
        let engine = EnforcementEngine::new(Box::new(store));
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            root_dir,
        })
    }
}
