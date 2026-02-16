//! Violation snapshot for compile delta diffing.
//!
//! Every compile saves a lightweight violation snapshot to `.keel/last_compile.json`.
//! With `--delta`, we diff current result against previous snapshot.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::{CompileDelta, CompileResult, PressureLevel, Violation, ViolationKey};

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
            errors: result.errors.iter().map(violation_to_key).collect(),
            warnings: result.warnings.iter().map(violation_to_key).collect(),
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

/// Compute the delta between a previous snapshot and the current compile result.
pub fn compute_delta(previous: &ViolationSnapshot, current: &CompileResult) -> CompileDelta {
    let current_errors: HashSet<ViolationKey> =
        current.errors.iter().map(violation_to_key).collect();
    let current_warnings: HashSet<ViolationKey> =
        current.warnings.iter().map(violation_to_key).collect();

    let prev_errors: HashSet<ViolationKey> = previous.errors.iter().cloned().collect();
    let prev_warnings: HashSet<ViolationKey> = previous.warnings.iter().cloned().collect();

    let new_errors: Vec<ViolationKey> = current_errors
        .difference(&prev_errors)
        .cloned()
        .collect();
    let resolved_errors: Vec<ViolationKey> = prev_errors
        .difference(&current_errors)
        .cloned()
        .collect();
    let new_warnings: Vec<ViolationKey> = current_warnings
        .difference(&prev_warnings)
        .cloned()
        .collect();
    let resolved_warnings: Vec<ViolationKey> = prev_warnings
        .difference(&current_warnings)
        .cloned()
        .collect();

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

fn violation_to_key(v: &Violation) -> ViolationKey {
    ViolationKey {
        code: v.code.clone(),
        hash: v.hash.clone(),
        file: v.file.clone(),
        line: v.line,
    }
}
