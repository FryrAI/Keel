use std::collections::{HashMap, HashSet};

use crate::types::Violation;

/// Tracks consecutive failures per (error_code, identifier) pair.
/// The identifier is normally the node hash, but when hash is empty
/// (e.g. E003, W001, W002), we fall back to file_path so each file
/// gets its own counter instead of all sharing one.
///
/// After 3 consecutive failures:
///   attempt 1 = fix_hint
///   attempt 2 = wider discover context
///   attempt 3 = auto-downgrade to WARNING
#[derive(Debug)]
pub struct CircuitBreaker {
    state: HashMap<(String, String), FailureState>,
    max_failures: u32,
}

#[derive(Debug, Clone)]
pub struct FailureState {
    pub consecutive: u32,
    pub downgraded: bool,
}

/// What the circuit breaker recommends for a given failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerAction {
    /// First failure: show the fix hint.
    FixHint,
    /// Second failure: widen discover context.
    WiderContext,
    /// Third+ failure: auto-downgrade ERROR to WARNING.
    Downgrade,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the default max failures threshold (3).
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
            max_failures: 3,
        }
    }

    /// Create a circuit breaker with a custom max_failures threshold.
    pub fn with_max_failures(max_failures: u32) -> Self {
        Self {
            state: HashMap::new(),
            max_failures: max_failures.max(1), // at least 1
        }
    }

    /// Build the deduplication key. When hash is empty, fall back to
    /// file_path so that each file gets its own circuit breaker counter.
    fn make_key(error_code: &str, hash: &str, file_path: &str) -> (String, String) {
        let identifier = if hash.is_empty() {
            file_path.to_string()
        } else {
            hash.to_string()
        };
        (error_code.to_string(), identifier)
    }

    /// Record a failure and return the recommended action.
    /// `file_path` is used as fallback identifier when `hash` is empty.
    pub fn record_failure(
        &mut self,
        error_code: &str,
        hash: &str,
        file_path: &str,
    ) -> BreakerAction {
        let key = Self::make_key(error_code, hash, file_path);
        let entry = self.state.entry(key).or_insert(FailureState {
            consecutive: 0,
            downgraded: false,
        });
        entry.consecutive += 1;

        if entry.consecutive >= self.max_failures {
            entry.downgraded = true;
            BreakerAction::Downgrade
        } else if entry.consecutive == self.max_failures - 1 {
            BreakerAction::WiderContext
        } else {
            BreakerAction::FixHint
        }
    }

    /// Record a success — resets the counter for this (error_code, hash/file).
    pub fn record_success(&mut self, error_code: &str, hash: &str, file_path: &str) {
        let key = Self::make_key(error_code, hash, file_path);
        self.state.remove(&key);
    }

    /// Reset (record success for) tracked failures that have been resolved.
    ///
    /// For each of `codes`, clears any `(code, identifier)` entry whose
    /// identifier is in `scope` (i.e. it was actually checked this compile)
    /// but absent from `active` (i.e. it did not fire this time). This is
    /// what makes a fixed violation's counter clear — and any prior
    /// downgrade lift — instead of the counter only ever climbing across
    /// compiles, which previously auto-downgraded any violation that merely
    /// persisted for 3 compiles regardless of whether it was ever addressed.
    ///
    /// `scope` and `active` must use the same identifier convention as
    /// `CircuitBreaker::make_key` (hash, or file path when hash is empty).
    fn reset_resolved(
        &mut self,
        codes: &[&str],
        scope: &HashSet<String>,
        active: &HashSet<(String, String)>,
    ) {
        let stale: Vec<(String, String)> = self
            .state
            .keys()
            .filter(|(code, ident)| codes.contains(&code.as_str()) && scope.contains(ident))
            .filter(|key| !active.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            self.state.remove(&key);
        }
    }

    /// Reconcile one file's breaker state against its violations from this
    /// compile — the engine-facing wrapper around `reset_resolved`.
    ///
    /// - E001/E002/E003/E004 are scoped to hashes of nodes already in the
    ///   file (plus the file path itself, for the empty-hash fallback).
    /// - E005 is scoped to the hashes this file's call references resolve to.
    ///
    /// Disabled checks (e.g. type_hints off) need no special-casing: their
    /// counters can never fire again, so clearing them is harmless.
    pub fn reconcile_file(
        &mut self,
        file_path: &str,
        existing_hashes: impl Iterator<Item = String>,
        ref_resolved_hashes: impl Iterator<Item = String>,
        violations: &[Violation],
    ) {
        // Nothing tracked → nothing to reconcile; skip the scope-set builds
        // that would otherwise run for every file on the clean common path.
        if self.state.is_empty() {
            return;
        }

        let mut node_scope: HashSet<String> = existing_hashes.collect();
        node_scope.insert(file_path.to_string());
        let ref_scope: HashSet<String> = ref_resolved_hashes.collect();

        let active: HashSet<(String, String)> = violations
            .iter()
            .filter(|v| v.severity == "ERROR")
            .map(|v| Self::make_key(&v.code, &v.hash, &v.file))
            .collect();

        self.reset_resolved(&["E001", "E002", "E003", "E004"], &node_scope, &active);
        self.reset_resolved(&["E005"], &ref_scope, &active);
    }

    /// Check if a (error_code, hash/file) pair has been downgraded.
    pub fn is_downgraded(&self, error_code: &str, hash: &str, file_path: &str) -> bool {
        let key = Self::make_key(error_code, hash, file_path);
        self.state.get(&key).is_some_and(|s| s.downgraded)
    }

    /// Get the current failure count for a (error_code, hash/file) pair.
    pub fn failure_count(&self, error_code: &str, hash: &str, file_path: &str) -> u32 {
        let key = Self::make_key(error_code, hash, file_path);
        self.state.get(&key).map_or(0, |s| s.consecutive)
    }

    /// Export all circuit breaker state as tuples for persistence.
    /// Returns Vec of (error_code, hash, consecutive_failures, downgraded).
    pub fn export_state(&self) -> Vec<(String, String, u32, bool)> {
        self.state
            .iter()
            .map(|((code, hash), st)| (code.clone(), hash.clone(), st.consecutive, st.downgraded))
            .collect()
    }

    /// Import circuit breaker state from persistence.
    /// Each tuple is (error_code, hash, consecutive_failures, downgraded).
    pub fn import_state(&mut self, rows: &[(String, String, u32, bool)]) {
        for (code, hash, consecutive, downgraded) in rows {
            self.state.insert(
                (code.clone(), hash.clone()),
                FailureState {
                    consecutive: *consecutive,
                    downgraded: *downgraded,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escalation_sequence() {
        let mut cb = CircuitBreaker::new();
        assert_eq!(
            cb.record_failure("E001", "abc", "file.rs"),
            BreakerAction::FixHint
        );
        assert_eq!(
            cb.record_failure("E001", "abc", "file.rs"),
            BreakerAction::WiderContext
        );
        assert_eq!(
            cb.record_failure("E001", "abc", "file.rs"),
            BreakerAction::Downgrade
        );
        assert!(cb.is_downgraded("E001", "abc", "file.rs"));
    }

    #[test]
    fn test_success_resets() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc", "file.rs");
        cb.record_failure("E001", "abc", "file.rs");
        cb.record_success("E001", "abc", "file.rs");
        assert_eq!(cb.failure_count("E001", "abc", "file.rs"), 0);
        assert!(!cb.is_downgraded("E001", "abc", "file.rs"));
    }

    #[test]
    fn test_independent_keys() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc", "file.rs");
        cb.record_failure("E002", "abc", "file.rs");
        assert_eq!(cb.failure_count("E001", "abc", "file.rs"), 1);
        assert_eq!(cb.failure_count("E002", "abc", "file.rs"), 1);
    }

    #[test]
    fn test_empty_hash_uses_file_path() {
        let mut cb = CircuitBreaker::new();
        // Empty hash: should key by file_path, so different files get separate counters
        cb.record_failure("E003", "", "src/foo.py");
        cb.record_failure("E003", "", "src/foo.py");
        cb.record_failure("E003", "", "src/bar.py");

        assert_eq!(cb.failure_count("E003", "", "src/foo.py"), 2);
        assert_eq!(cb.failure_count("E003", "", "src/bar.py"), 1);
        assert!(!cb.is_downgraded("E003", "", "src/foo.py"));
        assert!(!cb.is_downgraded("E003", "", "src/bar.py"));
    }

    #[test]
    fn test_export_import_roundtrip() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc", "file.rs");
        cb.record_failure("E001", "abc", "file.rs"); // 2 failures
        cb.record_failure("E002", "def", "file.rs");
        cb.record_failure("E002", "def", "file.rs");
        cb.record_failure("E002", "def", "file.rs"); // 3 failures → downgraded

        let state = cb.export_state();
        assert_eq!(state.len(), 2);

        let mut cb2 = CircuitBreaker::new();
        cb2.import_state(&state);

        assert_eq!(cb2.failure_count("E001", "abc", "file.rs"), 2);
        assert!(!cb2.is_downgraded("E001", "abc", "file.rs"));
        assert_eq!(cb2.failure_count("E002", "def", "file.rs"), 3);
        assert!(cb2.is_downgraded("E002", "def", "file.rs"));
    }

    #[test]
    fn test_reset_resolved_clears_stale_entry_not_in_active() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E002", "hash1", "app.py");
        assert_eq!(cb.failure_count("E002", "hash1", "app.py"), 2);

        // hash1 was in scope (checked this round) but did not fire (not active) => resolved
        let scope: HashSet<String> = ["hash1".to_string()].into_iter().collect();
        let active: HashSet<(String, String)> = HashSet::new();
        cb.reset_resolved(&["E002"], &scope, &active);

        assert_eq!(cb.failure_count("E002", "hash1", "app.py"), 0);
        assert!(!cb.is_downgraded("E002", "hash1", "app.py"));
    }

    #[test]
    fn test_reset_resolved_leaves_still_active_entry() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E002", "hash1", "app.py");

        // hash1 is in scope AND still active (still firing) => must NOT be cleared
        let scope: HashSet<String> = ["hash1".to_string()].into_iter().collect();
        let active: HashSet<(String, String)> = [("E002".to_string(), "hash1".to_string())]
            .into_iter()
            .collect();
        cb.reset_resolved(&["E002"], &scope, &active);

        assert_eq!(cb.failure_count("E002", "hash1", "app.py"), 2);
    }

    #[test]
    fn test_reset_resolved_ignores_entries_outside_scope() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E002", "hash1", "app.py");

        // hash1 was not checked this round (not in scope) => leave it alone,
        // since we can't tell whether it's resolved or simply not compiled.
        let scope: HashSet<String> = HashSet::new();
        let active: HashSet<(String, String)> = HashSet::new();
        cb.reset_resolved(&["E002"], &scope, &active);

        assert_eq!(cb.failure_count("E002", "hash1", "app.py"), 2);
    }

    #[test]
    fn test_reset_resolved_only_touches_given_codes() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E003", "hash1", "app.py");

        let scope: HashSet<String> = ["hash1".to_string()].into_iter().collect();
        let active: HashSet<(String, String)> = HashSet::new();
        // Only reset E002, not E003
        cb.reset_resolved(&["E002"], &scope, &active);

        assert_eq!(cb.failure_count("E002", "hash1", "app.py"), 0);
        assert_eq!(cb.failure_count("E003", "hash1", "app.py"), 1);
    }

    #[test]
    fn test_reset_resolved_lifts_prior_downgrade() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E002", "hash1", "app.py");
        cb.record_failure("E002", "hash1", "app.py"); // downgraded
        assert!(cb.is_downgraded("E002", "hash1", "app.py"));

        let scope: HashSet<String> = ["hash1".to_string()].into_iter().collect();
        let active: HashSet<(String, String)> = HashSet::new();
        cb.reset_resolved(&["E002"], &scope, &active);

        assert!(!cb.is_downgraded("E002", "hash1", "app.py"));
        // Next failure starts a fresh escalation cycle
        assert_eq!(
            cb.record_failure("E002", "hash1", "app.py"),
            BreakerAction::FixHint
        );
    }

    #[test]
    fn test_sqlite_full_roundtrip() {
        // Full integration: CB → export → SQLite → load → new CB
        let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();

        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "hash1", "src/a.rs");
        cb.record_failure("E001", "hash1", "src/a.rs");
        cb.record_failure("E005", "hash2", "src/b.rs");
        cb.record_failure("E005", "hash2", "src/b.rs");
        cb.record_failure("E005", "hash2", "src/b.rs"); // downgraded

        // Persist to SQLite
        let state = cb.export_state();
        store.save_circuit_breaker(&state).unwrap();

        // Load from SQLite into a new CircuitBreaker
        let loaded = store.load_circuit_breaker().unwrap();
        let mut cb2 = CircuitBreaker::new();
        cb2.import_state(&loaded);

        assert_eq!(cb2.failure_count("E001", "hash1", "src/a.rs"), 2);
        assert!(!cb2.is_downgraded("E001", "hash1", "src/a.rs"));
        assert_eq!(cb2.failure_count("E005", "hash2", "src/b.rs"), 3);
        assert!(cb2.is_downgraded("E005", "hash2", "src/b.rs"));

        // Verify next failure on the restored CB works correctly
        let action = cb2.record_failure("E001", "hash1", "src/a.rs");
        assert_eq!(action, BreakerAction::Downgrade);
        assert!(cb2.is_downgraded("E001", "hash1", "src/a.rs"));
    }
}
