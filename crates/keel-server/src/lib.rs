pub mod http;
pub mod mcp;
pub mod watcher;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use keel_core::sqlite::SqliteGraphStore;

/// Shared server state wrapping the graph store.
///
/// Uses `std::sync::Mutex` because `rusqlite::Connection` is `!Send`.
/// All DB access goes through `store.lock()` — keep critical sections short.
///
/// The `EnforcementEngine` from keel-enforce is not yet wired —
/// compile/explain/discover stubs return placeholder results until integration.
pub struct KeelServer {
    pub store: Arc<Mutex<SqliteGraphStore>>,
    pub root_dir: PathBuf,
}

impl KeelServer {
    /// Create a new server instance from an existing database path.
    pub fn open(db_path: &str, root_dir: PathBuf) -> Result<Self, keel_core::types::GraphError> {
        let store = SqliteGraphStore::open(db_path)?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            root_dir,
        })
    }

    /// Create a server with an in-memory store (testing).
    pub fn in_memory(root_dir: PathBuf) -> Result<Self, keel_core::types::GraphError> {
        let store = SqliteGraphStore::in_memory()?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            root_dir,
        })
    }
}
