use std::time::{Duration, SystemTime, UNIX_EPOCH};

use keel_core::sqlite::SqliteGraphStore;
use serde::{Deserialize, Serialize};

use crate::types::Violation;

/// Batch-mode state persisted to SQLite so it survives across separate `keel
/// compile` processes.
///
/// The in-process [`BatchState`] is dropped when a CLI process exits, so a
/// `--batch-start` in one invocation, a violating compile in a second, and a
/// `--batch-end` in a third would otherwise lose every deferred violation — the
/// feature would silently no-op. So deferred violations and the batch's start
/// time are persisted next to the circuit-breaker state in `graph.db` (a single
/// `keel_meta` row) and reloaded by the next invocation. Storing it in the same
/// atomically-written store the circuit breaker uses replaces the old
/// non-atomic `.keel/batch.json` write and the silent parse-error swallow.
///
/// This type stays the seam: the CLI only ever touches
/// `new`/`load`/`save`/`clear`/`touch`/`is_expired`.
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

    /// Load the persisted batch, or `None` if no batch is active (or the stored
    /// blob is unreadable — a corrupt row is treated as no batch).
    pub fn load(store: &SqliteGraphStore) -> Option<Self> {
        let json = store.load_batch()?;
        serde_json::from_str(&json).ok()
    }

    /// Persist the batch to the store.
    pub fn save(&self, store: &SqliteGraphStore) -> Result<(), String> {
        let json =
            serde_json::to_string(self).map_err(|e| format!("failed to serialize batch: {}", e))?;
        store
            .save_batch(&json)
            .map_err(|e| format!("failed to persist batch: {}", e))
    }

    /// Remove the persisted batch (batch mode ended or expired).
    pub fn clear(store: &SqliteGraphStore) {
        let _ = store.clear_batch();
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

/// Tracks the violations deferred during an engine's in-process batch.
///
/// Inactivity expiry lives entirely on [`PersistentBatch`] (checked by the CLI
/// against the persisted state before it ever enters batch mode), so this
/// in-process state carries no clock of its own — it is just the deferred queue.
#[derive(Debug, Default)]
pub struct BatchState {
    deferred: Vec<Violation>,
}

impl BatchState {
    /// Create an empty batch state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if this code should be deferred in batch mode.
    pub fn is_deferrable(code: &str) -> bool {
        DEFERRABLE_CODES.contains(&code)
    }

    /// Add a violation to the deferred queue.
    pub fn defer(&mut self, violation: Violation) {
        self.deferred.push(violation);
    }

    /// Consume this batch state and return all deferred violations.
    pub fn drain(self) -> Vec<Violation> {
        self.deferred
    }

    /// Number of deferred violations.
    pub fn deferred_count(&self) -> usize {
        self.deferred.len()
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
    fn test_e005_not_deferrable() {
        assert!(!BatchState::is_deferrable("E005"));
    }

    fn e002(msg: &str) -> Violation {
        Violation {
            code: "E002".to_string(),
            severity: "ERROR".to_string(),
            category: "missing_type_hints".to_string(),
            message: msg.to_string(),
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
        }
    }

    #[test]
    fn test_persistent_batch_expiry_is_time_based() {
        let fresh = PersistentBatch::new();
        assert!(!fresh.is_expired(), "a just-started batch is not expired");

        let stale = PersistentBatch {
            started_at_unix: now_unix().saturating_sub(BATCH_TIMEOUT.as_secs() + 5),
            deferred: vec![],
        };
        assert!(stale.is_expired(), "a batch idle past 60s is expired");
    }

    #[test]
    fn test_persistent_batch_sqlite_roundtrip() {
        // save -> load returns the same deferred violations; clear removes it.
        let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();
        assert!(
            PersistentBatch::load(&store).is_none(),
            "no batch persisted yet"
        );

        let mut batch = PersistentBatch::new();
        batch.deferred.push(e002("deferred type hint"));
        batch.save(&store).unwrap();

        let loaded = PersistentBatch::load(&store).expect("batch persisted");
        assert_eq!(loaded.deferred.len(), 1);
        assert_eq!(loaded.deferred[0].code, "E002");
        assert_eq!(loaded.started_at_unix, batch.started_at_unix);

        PersistentBatch::clear(&store);
        assert!(
            PersistentBatch::load(&store).is_none(),
            "clear removes the persisted batch"
        );
    }
}
