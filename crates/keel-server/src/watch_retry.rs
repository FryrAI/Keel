//! Retain and merge watcher batches while another graph writer is active.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use keel_core::graph_lock::GraphLockError;
use tokio::sync::mpsc;

use crate::watcher::{apply_batch, log_outcome, ChangeKind, WatchBatch};
use crate::writer::SharedEngine;

fn merge(pending: &mut WatchBatch, next: WatchBatch) {
    let mut paths = HashMap::new();
    for batch in [std::mem::take(pending), next] {
        for path in batch.changed {
            paths.insert(path, ChangeKind::Changed);
        }
        for path in batch.removed {
            paths.insert(path, ChangeKind::Removed);
        }
    }
    for (path, kind) in paths {
        match kind {
            ChangeKind::Changed => pending.changed.push(path),
            ChangeKind::Removed => pending.removed.push(path),
        }
    }
}

/// Apply queued batches, retaining busy work until the graph lock is available.
pub(crate) async fn run_batches(
    engine: SharedEngine,
    root: PathBuf,
    mut rx: mpsc::Receiver<WatchBatch>,
) {
    let mut pending = WatchBatch::default();
    let mut closed = false;
    loop {
        if pending.is_empty() {
            if closed {
                break;
            }
            match rx.recv().await {
                Some(batch) => pending = batch,
                None => break,
            }
        }
        match apply_batch(&engine, &root, &pending) {
            Ok(outcome) => {
                log_outcome(&pending, &outcome);
                pending = WatchBatch::default();
            }
            Err(GraphLockError::Busy) => {
                tokio::select! {
                    next = rx.recv(), if !closed => match next {
                        Some(batch) => merge(&mut pending, batch),
                        None => closed = true,
                    },
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {},
                }
            }
            Err(error) => {
                eprintln!("[keel watch] {error}; dropping batch");
                pending = WatchBatch::default();
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_retry_tests.rs"]
mod tests;
