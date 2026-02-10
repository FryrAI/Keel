use std::collections::HashMap;

/// Tracks consecutive failures per (error_code, hash) pair.
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

    /// Record a failure and return the recommended action.
    pub fn record_failure(&mut self, error_code: &str, hash: &str) -> BreakerAction {
        let key = (error_code.to_string(), hash.to_string());
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

    /// Record a success — resets the counter for this (error_code, hash).
    pub fn record_success(&mut self, error_code: &str, hash: &str) {
        let key = (error_code.to_string(), hash.to_string());
        self.state.remove(&key);
    }

    /// Check if a (error_code, hash) pair has been downgraded.
    pub fn is_downgraded(&self, error_code: &str, hash: &str) -> bool {
        let key = (error_code.to_string(), hash.to_string());
        self.state
            .get(&key)
            .is_some_and(|s| s.downgraded)
    }

    /// Get the current failure count for a (error_code, hash) pair.
    pub fn failure_count(&self, error_code: &str, hash: &str) -> u32 {
        let key = (error_code.to_string(), hash.to_string());
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
        assert_eq!(cb.record_failure("E001", "abc"), BreakerAction::FixHint);
        assert_eq!(cb.record_failure("E001", "abc"), BreakerAction::WiderContext);
        assert_eq!(cb.record_failure("E001", "abc"), BreakerAction::Downgrade);
        assert!(cb.is_downgraded("E001", "abc"));
    }

    #[test]
    fn test_success_resets() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc");
        cb.record_failure("E001", "abc");
        cb.record_success("E001", "abc");
        assert_eq!(cb.failure_count("E001", "abc"), 0);
        assert!(!cb.is_downgraded("E001", "abc"));
    }

    #[test]
    fn test_independent_keys() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc");
        cb.record_failure("E002", "abc");
        assert_eq!(cb.failure_count("E001", "abc"), 1);
        assert_eq!(cb.failure_count("E002", "abc"), 1);
    }

    #[test]
    fn test_export_import_roundtrip() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "abc");
        cb.record_failure("E001", "abc"); // 2 failures
        cb.record_failure("E002", "def");
        cb.record_failure("E002", "def");
        cb.record_failure("E002", "def"); // 3 failures → downgraded

        let state = cb.export_state();
        assert_eq!(state.len(), 2);

        let mut cb2 = CircuitBreaker::new();
        cb2.import_state(&state);

        assert_eq!(cb2.failure_count("E001", "abc"), 2);
        assert!(!cb2.is_downgraded("E001", "abc"));
        assert_eq!(cb2.failure_count("E002", "def"), 3);
        assert!(cb2.is_downgraded("E002", "def"));
    }

    #[test]
    fn test_sqlite_full_roundtrip() {
        // Full integration: CB → export → SQLite → load → new CB
        let store = keel_core::sqlite::SqliteGraphStore::in_memory().unwrap();

        let mut cb = CircuitBreaker::new();
        cb.record_failure("E001", "hash1");
        cb.record_failure("E001", "hash1");
        cb.record_failure("E005", "hash2");
        cb.record_failure("E005", "hash2");
        cb.record_failure("E005", "hash2"); // downgraded

        // Persist to SQLite
        let state = cb.export_state();
        store.save_circuit_breaker(&state).unwrap();

        // Load from SQLite into a new CircuitBreaker
        let loaded = store.load_circuit_breaker().unwrap();
        let mut cb2 = CircuitBreaker::new();
        cb2.import_state(&loaded);

        assert_eq!(cb2.failure_count("E001", "hash1"), 2);
        assert!(!cb2.is_downgraded("E001", "hash1"));
        assert_eq!(cb2.failure_count("E005", "hash2"), 3);
        assert!(cb2.is_downgraded("E005", "hash2"));

        // Verify next failure on the restored CB works correctly
        let action = cb2.record_failure("E001", "hash1");
        assert_eq!(action, BreakerAction::Downgrade);
        assert!(cb2.is_downgraded("E001", "hash1"));
    }
}
