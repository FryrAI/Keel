use std::collections::HashMap;

/// Tracks consecutive failures per (error_code, hash) pair.
/// After 3 consecutive failures:
///   attempt 1 = fix_hint
///   attempt 2 = wider discover context
///   attempt 3 = auto-downgrade to WARNING
#[derive(Debug)]
pub struct CircuitBreaker {
    state: HashMap<(String, String), FailureState>,
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

        match entry.consecutive {
            1 => BreakerAction::FixHint,
            2 => BreakerAction::WiderContext,
            _ => {
                entry.downgraded = true;
                BreakerAction::Downgrade
            }
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
}
