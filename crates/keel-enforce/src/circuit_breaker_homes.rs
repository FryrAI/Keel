//! Reconciliation for normalized expression-home scopes.

use std::collections::HashSet;

use super::CircuitBreaker;

impl CircuitBreaker {
    /// Clear resolved W011/E007 entries recorded under a normalized file scope.
    /// Other codes retain their ordinary file reconciliation, since their
    /// findings may use a lexical path that differs from this home scope.
    pub(crate) fn reconcile_home_scope(&mut self, scope: &str, active: &[(String, String)]) {
        if self.state.is_empty() {
            return;
        }
        let active: HashSet<_> = active.iter().cloned().collect();
        self.state.retain(|key, state| {
            !matches!(key.0.as_str(), "W011" | "E007")
                || state.file != scope
                || active.contains(key)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_scope_clears_only_resolved_home_codes() {
        let mut breaker = CircuitBreaker::new();
        for code in ["W011", "E007", "E003", "E005"] {
            breaker.record_failure(code, "", "first", "src/caller.rs");
        }
        breaker.record_failure("E007", "", "first", "src/other.rs");
        breaker.reconcile_home_scope("src/caller.rs", &[]);

        for code in ["W011", "E007"] {
            assert_eq!(breaker.failure_count(code, "", "src/caller.rs"), 0);
        }
        for code in ["E003", "E005"] {
            assert_eq!(breaker.failure_count(code, "", "src/caller.rs"), 1);
        }
        assert_eq!(breaker.failure_count("E007", "", "src/other.rs"), 1);
    }

    #[test]
    fn test_home_scope_preserves_active_home_entries() {
        let mut breaker = CircuitBreaker::new();
        for code in ["W011", "E007"] {
            breaker.record_failure(code, "", "first", "src/caller.rs");
        }
        let before = breaker.export_state();
        breaker.reconcile_home_scope(
            "src/caller.rs",
            &[
                ("W011".to_string(), "src/caller.rs".to_string()),
                ("E007".to_string(), "src/caller.rs".to_string()),
            ],
        );
        assert_eq!(breaker.export_state(), before);
    }
}
