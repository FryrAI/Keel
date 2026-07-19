//! `keel checkpoint` — compact, compaction-resilient session-state summary.
//!
//! Everything here is deterministic and derived from git plus the stored
//! graph — there is **no** persisted session state. The core assembly
//! ([`build_checkpoint`]) and the git helpers are shared by the CLI command
//! and the MCP `keel/checkpoint` tool so the two interfaces never diverge.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::gitdiff::{self, DiffMode};
use crate::types::CompileResult;

/// A symbol reference: its name and content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolRef {
    pub name: String,
    pub hash: String,
}

/// Per-file symbol delta between the working tree and the stored graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDelta {
    pub file: String,
    pub added: Vec<SymbolRef>,
    pub changed: Vec<SymbolRef>,
    pub removed: Vec<SymbolRef>,
}

/// A caller in the stored graph, with its location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallerRef {
    pub name: String,
    pub file: String,
    pub line: u32,
}

/// The stored callers of a changed/removed symbol — the structural impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedCaller {
    /// The changed or removed symbol whose callers are listed.
    pub symbol: String,
    pub callers: Vec<CallerRef>,
}

/// A compact outstanding-violation entry (a subset of a full `Violation`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointViolation {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub file: String,
    pub line: u32,
}

/// The full checkpoint payload, rendered by the output formatters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointResult {
    pub version: String,
    pub command: String,
    /// Human-readable description of the diff range.
    pub range: String,
    pub files: Vec<FileDelta>,
    pub affected_callers: Vec<AffectedCaller>,
    pub violations: Vec<CheckpointViolation>,
    pub error_count: usize,
    pub warning_count: usize,
    pub commits: Vec<String>,
}

/// Which git diff a checkpoint summarizes.
#[derive(Debug, Clone)]
pub enum CheckpointMode {
    /// Working tree vs a base commit (default base: `HEAD`).
    Since(Option<String>),
    /// Staged (index) changes.
    Staged,
}

/// Run a git command in `dir`, returning stdout on success.
fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// List repo-relative paths of source files changed for the given mode.
///
/// Delegates to the shared [`gitdiff::changed_files`] (supported-language filter
/// on), which adds the initial-commit fallback checkpoint's inline version
/// lacked.
pub fn changed_files(dir: &Path, mode: &CheckpointMode) -> Vec<String> {
    let diff_mode = match mode {
        CheckpointMode::Staged => DiffMode::Staged,
        CheckpointMode::Since(base) => DiffMode::Since(base.clone()),
    };
    gitdiff::changed_files(dir, &diff_mode, true)
}

/// Recent commit subjects (`git log --oneline`) for the given mode.
pub fn commit_subjects(dir: &Path, mode: &CheckpointMode) -> Vec<String> {
    let raw = match mode {
        CheckpointMode::Staged => run_git(dir, &["log", "--oneline", "-n", "5"]),
        CheckpointMode::Since(Some(base)) => {
            run_git(dir, &["log", "--oneline", &format!("{}..HEAD", base)])
        }
        CheckpointMode::Since(None) => run_git(dir, &["log", "--oneline", "-n", "5"]),
    };
    raw.map(|t| t.lines().map(|l| l.to_string()).collect())
        .unwrap_or_default()
}

/// A human-readable label for the diff range.
pub fn range_label(mode: &CheckpointMode) -> String {
    match mode {
        CheckpointMode::Staged => "staged".to_string(),
        CheckpointMode::Since(Some(b)) => format!("since {} (working tree)", b),
        CheckpointMode::Since(None) => "since HEAD (working tree)".to_string(),
    }
}

/// Compute the (un-disambiguated) content hash of a parsed definition, the
/// same way `keel map` does for a non-colliding symbol.
fn def_hash(def: &keel_parsers::resolver::Definition) -> String {
    def.hash()
}

/// Collect the stored callers of a symbol node.
pub(crate) fn callers_of(store: &dyn GraphStore, node: &GraphNode) -> Vec<CallerRef> {
    store
        .get_edges(node.id, EdgeDirection::Incoming)
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .filter_map(|e| {
            let src = store.get_node_by_id(e.source_id)?;
            Some(CallerRef {
                name: src.name,
                file: src.file_path,
                line: src.line_start,
            })
        })
        .collect()
}

/// The symbol delta between the working tree and the stored graph.
///
/// Captured against the **pre-edit** graph. This MUST be computed before any
/// `engine.compile` call, because compile persists re-baselined node hashes
/// back to the store (engine.rs) — reading the diff afterwards would compare
/// the new parse against an already-updated graph and see no change.
#[derive(Debug, Clone, Default)]
pub struct CheckpointDiff {
    pub files: Vec<FileDelta>,
    pub affected_callers: Vec<AffectedCaller>,
}

/// Diff a fresh parse of the changed files against the stored graph.
///
/// Read-only. Call this before running enforcement (see [`CheckpointDiff`]).
pub fn diff_changed_files(store: &dyn GraphStore, file_indices: &[FileIndex]) -> CheckpointDiff {
    let mut files: Vec<FileDelta> = Vec::new();
    let mut affected: Vec<AffectedCaller> = Vec::new();

    for fi in file_indices {
        let stored = store.get_nodes_in_file(&fi.file_path);
        let stored_by_name: HashMap<&str, &GraphNode> = stored
            .iter()
            .filter(|n| n.kind != NodeKind::Module)
            .map(|n| (n.name.as_str(), n))
            .collect();

        // Current parse: name -> content hash (first definition wins).
        let mut current: HashMap<String, String> = HashMap::new();
        let mut current_order: Vec<String> = Vec::new();
        for def in &fi.definitions {
            if def.kind == NodeKind::Module {
                continue;
            }
            if !current.contains_key(&def.name) {
                current.insert(def.name.clone(), def_hash(def));
                current_order.push(def.name.clone());
            }
        }

        let mut added = Vec::new();
        let mut changed = Vec::new();
        for name in &current_order {
            let cur_hash = &current[name];
            match stored_by_name.get(name.as_str()) {
                None => added.push(SymbolRef {
                    name: name.clone(),
                    hash: cur_hash.clone(),
                }),
                Some(node) if &node.hash != cur_hash => changed.push(SymbolRef {
                    name: name.clone(),
                    hash: cur_hash.clone(),
                }),
                Some(_) => {}
            }
        }

        let mut removed = Vec::new();
        for node in stored.iter().filter(|n| n.kind != NodeKind::Module) {
            if !current.contains_key(&node.name) {
                removed.push(SymbolRef {
                    name: node.name.clone(),
                    hash: node.hash.clone(),
                });
            }
        }

        added.sort_by(|a, b| a.name.cmp(&b.name));
        changed.sort_by(|a, b| a.name.cmp(&b.name));
        removed.sort_by(|a, b| a.name.cmp(&b.name));

        // Structural impact: callers of changed/removed symbols.
        for sym in changed.iter().chain(removed.iter()) {
            if let Some(node) = stored_by_name.get(sym.name.as_str()) {
                let callers = callers_of(store, node);
                if !callers.is_empty() {
                    affected.push(AffectedCaller {
                        symbol: sym.name.clone(),
                        callers,
                    });
                }
            }
        }

        if !added.is_empty() || !changed.is_empty() || !removed.is_empty() {
            files.push(FileDelta {
                file: fi.file_path.clone(),
                added,
                changed,
                removed,
            });
        }
    }

    files.sort_by(|a, b| a.file.cmp(&b.file));
    CheckpointDiff {
        files,
        affected_callers: affected,
    }
}

/// Combine a pre-computed [`CheckpointDiff`], the enforcement result for the
/// changed files, and git metadata into the final checkpoint.
pub fn build_checkpoint(
    diff: CheckpointDiff,
    compile_result: &CompileResult,
    range: String,
    commits: Vec<String>,
) -> CheckpointResult {
    let violations: Vec<CheckpointViolation> = compile_result
        .errors
        .iter()
        .chain(compile_result.warnings.iter())
        .map(|v| CheckpointViolation {
            code: v.code.clone(),
            severity: v.severity.clone(),
            message: v.message.clone(),
            file: v.file.clone(),
            line: v.line,
        })
        .collect();

    CheckpointResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "checkpoint".to_string(),
        range,
        files: diff.files,
        affected_callers: diff.affected_callers,
        violations,
        error_count: compile_result.errors.len(),
        warning_count: compile_result.warnings.len(),
        commits,
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
