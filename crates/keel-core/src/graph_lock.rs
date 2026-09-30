//! Kernel-backed exclusion for disk-backed graph writers.
//!
//! The persistent `compile.lock` file carries a diagnostic PID only. Ownership
//! belongs to the open handle and ends on drop or process death.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// An exclusive graph lock, released when its file handle closes.
#[derive(Debug)]
pub struct GraphLock {
    file: File,
}

/// Contention is distinct from a failure to open or lock the file.
#[derive(Debug, thiserror::Error)]
pub enum GraphLockError {
    /// Another handle owns the graph lock.
    #[error("graph busy: another keel process holds the graph lock")]
    Busy,
    /// The lock file could not be opened, locked, or updated.
    #[error("graph lock I/O error: {0}")]
    Io(#[from] io::Error),
}

/// Acquire the graph lock without waiting; never interpret the diagnostic PID.
///
/// A second handle, including one in this process, cannot acquire a held lock.
/// The caller must create `keel_dir` first and must never remove the lock file
/// during normal graph operations.
pub fn try_acquire(keel_dir: &Path) -> Result<GraphLock, GraphLockError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(keel_dir.join("compile.lock"))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(GraphLockError::Busy),
        Err(TryLockError::Error(e)) => return Err(GraphLockError::Io(e)),
    }
    file.set_len(0)?;
    write!(file, "{}", std::process::id())?;
    Ok(GraphLock { file })
}

/// Poll for the graph lock every 100 ms, bounded by `timeout`.
///
/// This synchronous wait belongs only on CLI or blocking threads. Async writers
/// must poll `try_acquire` with an asynchronous sleep instead.
pub fn acquire(keel_dir: &Path, timeout: Duration) -> Result<GraphLock, GraphLockError> {
    let deadline = Instant::now() + timeout;
    loop {
        match try_acquire(keel_dir) {
            Err(GraphLockError::Busy) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(GraphLockError::Busy);
                }
                std::thread::sleep(Duration::from_millis(100).min(remaining));
            }
            result => return result,
        }
    }
}

impl Drop for GraphLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
#[path = "graph_lock_tests.rs"]
mod tests;
