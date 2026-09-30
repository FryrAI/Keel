//! Shared writer context and graph-before-engine lock ordering.

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use keel_core::graph_lock::{self, GraphLock, GraphLockError};
use keel_enforce::engine::EnforcementEngine;

/// A shared engine with the lock directory of its actual disk backend.
#[derive(Clone)]
pub struct SharedEngine {
    engine: Arc<Mutex<EnforcementEngine>>,
    keel_dir: Option<PathBuf>,
}

/// Locate the lock beside a disk database; SQLite memory/temporary paths opt out.
pub(crate) fn disk_lock_dir(db_path: &str) -> Option<PathBuf> {
    if db_path == ":memory:" || db_path.is_empty() {
        return None;
    }
    Some(
        std::path::Path::new(db_path)
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf(),
    )
}

/// A writer's engine guard and kernel lock, released in that order.
pub(crate) struct WriterGuard<'a> {
    engine: MutexGuard<'a, EnforcementEngine>,
    _graph: Option<GraphLock>,
}

impl SharedEngine {
    /// Construct a context; `None` is reserved for in-memory stores.
    pub fn new(engine: EnforcementEngine, keel_dir: Option<PathBuf>) -> Self {
        Self {
            engine: Arc::new(Mutex::new(engine)),
            keel_dir,
        }
    }

    fn finish_writer(&self, graph: Option<GraphLock>) -> Result<WriterGuard<'_>, GraphLockError> {
        let engine = self
            .engine
            .lock()
            .map_err(|_| GraphLockError::Io(std::io::Error::other("Engine lock poisoned")))?;
        Ok(WriterGuard {
            engine,
            _graph: graph,
        })
    }

    /// Try once, for watcher batches that must remain pending on contention.
    pub(crate) fn try_writer(&self) -> Result<WriterGuard<'_>, GraphLockError> {
        let graph = self
            .keel_dir
            .as_deref()
            .map(graph_lock::try_acquire)
            .transpose()?;
        self.finish_writer(graph)
    }

    /// Bounded synchronous acquisition for MCP's blocking stdio thread.
    pub(crate) fn writer(&self) -> Result<WriterGuard<'_>, GraphLockError> {
        let graph = self
            .keel_dir
            .as_deref()
            .map(|dir| graph_lock::acquire(dir, Duration::from_secs(2)))
            .transpose()?;
        self.finish_writer(graph)
    }

    /// Poll asynchronously, before taking the engine mutex, for HTTP writers.
    pub(crate) async fn writer_async(&self) -> Result<WriterGuard<'_>, GraphLockError> {
        let graph = if let Some(dir) = self.keel_dir.as_deref() {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match graph_lock::try_acquire(dir) {
                    Ok(graph) => break Some(graph),
                    Err(GraphLockError::Busy) => {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            return Err(GraphLockError::Busy);
                        }
                        tokio::time::sleep(Duration::from_millis(100).min(remaining)).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        } else {
            None
        };
        self.finish_writer(graph)
    }
}

// Existing read-only handlers retain their short engine-only critical sections.
impl Deref for SharedEngine {
    type Target = Mutex<EnforcementEngine>;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

/// Wrap an existing in-memory engine, primarily for server fixtures.
impl From<Arc<Mutex<EnforcementEngine>>> for SharedEngine {
    fn from(engine: Arc<Mutex<EnforcementEngine>>) -> Self {
        Self {
            engine,
            keel_dir: None,
        }
    }
}

impl Deref for WriterGuard<'_> {
    type Target = EnforcementEngine;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl DerefMut for WriterGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}
