pub mod http;
pub mod mcp;
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
        let engine = EnforcementEngine::new(Box::new(store));
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
