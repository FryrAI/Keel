//! Enforcement engine for keel structural contracts.
//!
//! Validates code against the structural graph and produces violations:
//! - E001: broken callers (signature changed, callers need updating)
//! - E002: missing type hints (Python params, JS JSDoc)
//! - E003: missing docstrings on public functions
//! - E004: function removed (callers reference deleted function)
//! - E005: arity mismatch (caller passes wrong number of arguments)
//! - W001: placement suggestion (function may belong in a different module)
//! - W002: duplicate name (same function name in multiple modules)

pub mod analyze;
pub mod batch;
pub mod check;
pub mod circuit_breaker;
pub mod engine;
pub mod fix_generator;
pub mod hash_diff;
pub mod naming;
pub mod snapshot;
pub mod suppress;
pub mod types;
pub mod violations;
pub mod violations_extended;
pub mod violations_util;
