use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::types::Violation;

/// Batch-mode state persisted to `.keel/batch.json` so it survives across
/// separate `keel compile` processes.
///
/// The in-process [`BatchState`] is dropped when a CLI process exits, so a
/// `--batch-start` in one invocation, a violating compile in a second, and a
/// `--batch-end` in a third would otherwise lose every deferred violation — the
/// feature would silently no-op. This mirrors how the circuit breaker persists
/// its state to SQLite: deferred violations and the batch's start time are
/// written to disk and reloaded by the next invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentBatch {
    /// Unix seconds when the batch was last touched (start or a deferring
    /// compile). Drives the documented 60s inactivity auto-expire.
    pub started_at_unix: u64,
    /// Violations deferred so far across every compile in this batch.
    pub deferred: Vec<Violation>,
}

impl PersistentBatch {
    /// Start a fresh, empty batch stamped at the current time.
    pub fn new() -> Self {
        Self {
            started_at_unix: now_unix(),
            deferred: Vec::new(),
        }
    }

    fn file_path(keel_dir: &Path) -> std::path::PathBuf {
        keel_dir.join("batch.json")
    }

    /// Load the persisted batch, or `None` if no batch is active.
    pub fn load(keel_dir: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(Self::file_path(keel_dir)).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Persist the batch to disk.
    pub fn save(&self, keel_dir: &Path) -> Result<(), String> {
        let path = Self::file_path(keel_dir);
        let json =
            serde_json::to_string(self).map_err(|e| format!("failed to serialize batch: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write batch to {}: {}", path.display(), e))
    }

    /// Remove the persisted batch (batch mode ended or expired).
    pub fn clear(keel_dir: &Path) {
        let _ = std::fs::remove_file(Self::file_path(keel_dir));
    }

    /// Refresh the inactivity timer (called on each deferring compile).
    pub fn touch(&mut self) {
        self.started_at_unix = now_unix();
    }

    /// True once the documented 60s inactivity window has elapsed.
    pub fn is_expired(&self) -> bool {
        now_unix().saturating_sub(self.started_at_unix) > BATCH_TIMEOUT.as_secs()
    }
}

impl Default for PersistentBatch {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Codes that are deferrable in batch mode.
/// Structural errors (E001, E004, E005) fire immediately.
/// Type hints (E002), docstrings (E003), placement (W001), duplicates (W002) are deferred.
// Economy checks (W005-W007) defer too: mid-scaffold, "no callers yet" and
// "file still growing" are expected states, not violations.
const DEFERRABLE_CODES: &[&str] = &["E002", "E003", "W001", "W002", "W005", "W006", "W007"];

/// Maximum time batch mode stays active before auto-expiring.
const BATCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Tracks deferred violations during batch mode.
#[derive(Debug)]
pub struct BatchState {
    deferred: Vec<Violation>,
    started_at: Instant,
}

impl Default for BatchState {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchState {
    /// Create a new batch state with an active timeout.
    pub fn new() -> Self {
        Self {
            deferred: Vec::new(),
            started_at: Instant::now(),
        }
    }

    /// Returns true if this code should be deferred in batch mode.
    pub fn is_deferrable(code: &str) -> bool {
        DEFERRABLE_CODES.contains(&code)
    }

    /// Add a violation to the deferred queue.
    pub fn defer(&mut self, violation: Violation) {
        self.deferred.push(violation);
    }

    /// Returns true if the batch has expired (60s inactivity timeout).
    pub fn is_expired(&self) -> bool {
        self.started_at.elapsed() > BATCH_TIMEOUT
    }

    /// Refresh the timeout (called on each compile during batch).
    pub fn touch(&mut self) {
        self.started_at = Instant::now();
    }

    /// Consume this batch state and return all deferred violations.
    pub fn drain(self) -> Vec<Violation> {
        self.deferred
    }

    /// Number of deferred violations.
    pub fn deferred_count(&self) -> usize {
        self.deferred.len()
    }

    /// Create a BatchState that is already expired (for testing).
    #[cfg(test)]
    pub fn new_expired() -> Self {
        Self {
            deferred: Vec::new(),
            started_at: Instant::now() - BATCH_TIMEOUT - Duration::from_secs(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deferrable_codes() {
        assert!(BatchState::is_deferrable("E002"));
        assert!(BatchState::is_deferrable("E003"));
        assert!(BatchState::is_deferrable("W001"));
        assert!(BatchState::is_deferrable("W002"));
        // Structural errors not deferrable
        assert!(!BatchState::is_deferrable("E001"));
        assert!(!BatchState::is_deferrable("E004"));
        assert!(!BatchState::is_deferrable("E005"));
    }

    #[test]
    fn test_batch_defer_and_drain() {
        let mut batch = BatchState::new();
        let v = Violation {
            code: "E002".to_string(),
            severity: "ERROR".to_string(),
            category: "missing_type_hints".to_string(),
            message: "test".to_string(),
            file: "a.py".to_string(),
            line: 1,
            hash: "abc".to_string(),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: None,
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        };
        batch.defer(v);
        assert_eq!(batch.deferred_count(), 1);
        let drained = batch.drain();
        assert_eq!(drained.len(), 1);
    }

    #[test]
    fn test_batch_not_expired_immediately() {
        let batch = BatchState::new();
        assert!(!batch.is_expired());
    }

    #[test]
    fn test_batch_expired() {
        let batch = BatchState::new_expired();
        assert!(
            batch.is_expired(),
            "Batch with past timestamp should be expired"
        );
    }

    #[test]
    fn test_batch_touch_refreshes_timeout() {
        let mut batch = BatchState::new();
        assert!(!batch.is_expired());
        batch.touch();
        assert!(!batch.is_expired(), "Touch should refresh the timeout");
    }

    #[test]
    fn test_batch_expired_drains_deferred() {
        let mut batch = BatchState::new_expired();
        let v = Violation {
            code: "E002".to_string(),
            severity: "ERROR".to_string(),
            category: "missing_type_hints".to_string(),
            message: "test".to_string(),
            file: "a.py".to_string(),
            line: 1,
            hash: "abc".to_string(),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: None,
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        };
        batch.defer(v);
        assert!(batch.is_expired());
        assert_eq!(batch.deferred_count(), 1);
        let drained = batch.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].code, "E002");
    }

    #[test]
    fn test_e005_not_deferrable() {
        assert!(!BatchState::is_deferrable("E005"));
    }
}
