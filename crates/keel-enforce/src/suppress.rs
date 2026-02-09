use std::collections::HashSet;

/// Manages suppressed error/warning codes.
///
/// When a code is suppressed, violations with that code are:
/// - Changed to severity "INFO" and marked suppressed=true
/// - Code changed to "S001"
/// - Given a suppress_hint explaining the suppression
#[derive(Debug)]
pub struct SuppressionManager {
    suppressed_codes: HashSet<String>,
}

impl Default for SuppressionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SuppressionManager {
    pub fn new() -> Self {
        Self {
            suppressed_codes: HashSet::new(),
        }
    }

    /// Add a code to suppress (e.g., "E002", "W001").
    pub fn suppress(&mut self, code: &str) {
        self.suppressed_codes.insert(code.to_string());
    }

    /// Check if a code is currently suppressed.
    pub fn is_suppressed(&self, code: &str) -> bool {
        self.suppressed_codes.contains(code)
    }

    /// Apply suppression to a violation, returning the modified violation.
    /// If the code is not suppressed, returns the violation unchanged.
    pub fn apply(&self, mut violation: crate::types::Violation) -> crate::types::Violation {
        if self.is_suppressed(&violation.code) {
            violation.suppress_hint = Some(format!(
                "Suppressed {} via --suppress flag",
                violation.code
            ));
            violation.suppressed = true;
            violation.code = "S001".to_string();
            violation.severity = "INFO".to_string();
        }
        violation
    }

    /// Number of active suppressions.
    pub fn count(&self) -> usize {
        self.suppressed_codes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Violation;

    fn test_violation(code: &str) -> Violation {
        Violation {
            code: code.to_string(),
            severity: "ERROR".to_string(),
            category: "test".to_string(),
            message: "test".to_string(),
            file: "a.rs".to_string(),
            line: 1,
            hash: "abc".to_string(),
            confidence: 1.0,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some("fix it".to_string()),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        }
    }

    #[test]
    fn test_suppress_and_apply() {
        let mut mgr = SuppressionManager::new();
        mgr.suppress("E002");

        let v = test_violation("E002");
        let result = mgr.apply(v);
        assert_eq!(result.code, "S001");
        assert_eq!(result.severity, "INFO");
        assert!(result.suppressed);
        assert!(result.suppress_hint.is_some());
    }

    #[test]
    fn test_unsuppressed_passthrough() {
        let mgr = SuppressionManager::new();
        let v = test_violation("E001");
        let result = mgr.apply(v);
        assert_eq!(result.code, "E001");
        assert_eq!(result.severity, "ERROR");
        assert!(!result.suppressed);
    }
}
