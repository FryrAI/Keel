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
//! - W005-W007: economy checks (dead code, duplicate impls, oversized files)
//! - W009: new cross-boundary dependency (+ opt-in E006 layer violation)
//! - W010/P003: advisory-only semantic-reuse candidates

pub mod analyze;
pub mod audit;
pub mod batch;
pub mod check;
pub mod checkpoint;
pub mod circuit_breaker;
pub mod engine;
pub mod file_class;
pub mod fix_generator;
pub mod focus;
pub mod gitdiff;
pub mod hash_diff;
pub mod map;
pub mod naming;
pub mod parse_util;
pub mod progressive;
pub mod quality;
pub mod queries;
pub mod review;
pub mod semantic;
pub mod skeleton;
pub mod snapshot;
pub mod suppress;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub mod types;
pub mod validate_plan;
pub mod validate_plan_findings;
mod validate_plan_reuse;
pub mod violations;
pub mod violations_boundary;
pub mod violations_economy;
pub mod violations_extended;
pub mod violations_util;
