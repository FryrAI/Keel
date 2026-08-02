//! Violation snapshot for compile delta diffing.
//!
//! Every compile saves a lightweight violation snapshot to `.keel/last_compile.json`.
//! With `--delta`, we diff current result against previous snapshot.
//!
//! The diff is keyed on `ViolationKey::stable` — `(code, hash, file)`, never the
//! line — so a pure line shift is not a new violation.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::{CompileDelta, CompileResult, PressureLevel, ViolationKey};

/// A snapshot of violations from a compile run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationSnapshot {
    pub errors: Vec<ViolationKey>,
    pub warnings: Vec<ViolationKey>,
}

impl ViolationSnapshot {
    /// Build a snapshot from a CompileResult.
    pub fn from_compile_result(result: &CompileResult) -> Self {
        Self {
            errors: result
                .errors
                .iter()
                .map(ViolationKey::from_violation)
                .collect(),
            warnings: result
                .warnings
                .iter()
                .map(ViolationKey::from_violation)
                .collect(),
        }
    }

    /// Save snapshot to disk.
    pub fn save(&self, keel_dir: &Path) -> Result<(), String> {
        let path = keel_dir.join("last_compile.json");
        let json = serde_json::to_string(self)
            .map_err(|e| format!("failed to serialize snapshot: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write snapshot to {}: {}", path.display(), e))?;
        Ok(())
    }

    /// Load snapshot from disk. Returns None if file doesn't exist.
    pub fn load(keel_dir: &Path) -> Option<Self> {
        let path = keel_dir.join("last_compile.json");
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

/// Index keys by their stable identity, last one winning.
///
/// A `BTreeMap` rather than a `HashSet` so the difference below comes out in a
/// deterministic order: a delta that reshuffles between runs cannot be diffed
/// by anything downstream, and a CI comment that reorders on every push reads
/// as churn.
fn by_stable(keys: &[ViolationKey]) -> BTreeMap<(&str, &str, &str), &ViolationKey> {
    keys.iter().map(|k| (k.stable(), k)).collect()
}

/// Keys present in `left` whose stable identity is absent from `right`.
fn difference(
    left: &BTreeMap<(&str, &str, &str), &ViolationKey>,
    right: &BTreeMap<(&str, &str, &str), &ViolationKey>,
) -> Vec<ViolationKey> {
    left.iter()
        .filter(|(key, _)| !right.contains_key(*key))
        .map(|(_, v)| (*v).clone())
        .collect()
}

/// Compute the delta between a previous snapshot and the current compile result.
pub fn compute_delta(previous: &ViolationSnapshot, current: &CompileResult) -> CompileDelta {
    let current_errors: Vec<ViolationKey> = current
        .errors
        .iter()
        .map(ViolationKey::from_violation)
        .collect();
    let current_warnings: Vec<ViolationKey> = current
        .warnings
        .iter()
        .map(ViolationKey::from_violation)
        .collect();

    let cur_err = by_stable(&current_errors);
    let cur_warn = by_stable(&current_warnings);
    let prev_err = by_stable(&previous.errors);
    let prev_warn = by_stable(&previous.warnings);

    let new_errors = difference(&cur_err, &prev_err);
    let resolved_errors = difference(&prev_err, &cur_err);
    let new_warnings = difference(&cur_warn, &prev_warn);
    let resolved_warnings = difference(&prev_warn, &cur_warn);

    let net_errors = new_errors.len() as i32 - resolved_errors.len() as i32;
    let net_warnings = new_warnings.len() as i32 - resolved_warnings.len() as i32;
    let total_errors = current.errors.len() as u32;
    let total_warnings = current.warnings.len() as u32;
    let pressure = PressureLevel::from_error_count(total_errors as usize);

    CompileDelta {
        new_errors,
        resolved_errors,
        new_warnings,
        resolved_warnings,
        net_errors,
        net_warnings,
        pressure,
        total_errors,
        total_warnings,
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
