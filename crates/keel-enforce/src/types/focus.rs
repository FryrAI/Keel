//! Result types for `keel focus` (issue #20) — the minimal context set for
//! safely modifying a target.

use serde::{Deserialize, Serialize};

/// Minimal context set for safely modifying a target: the files to read
/// (ranked), the transitive callers at risk, and a suggested read order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusResult {
    pub version: String,
    pub command: String,
    /// What the focus was requested for (hash or file path, as typed).
    pub target: String,
    pub depth: u32,
    /// Files to read, ranked by graph distance then caller count.
    pub files: Vec<FocusFile>,
    /// Transitive callers — the symbols at risk when the target changes.
    pub callers: Vec<FocusSymbol>,
    /// Suggested read order (file paths), dependencies-first.
    pub read_order: Vec<String>,
}

/// One file in the focus set, with the symbols it contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusFile {
    pub path: String,
    /// "target" | "callee" | "caller" — role of this file's nearest symbol.
    pub role: String,
    /// Minimum graph distance among this file's symbols (0 = target file).
    pub distance: u32,
    pub symbols: Vec<FocusSymbol>,
}

/// A single symbol in the focus set with its adjacency counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSymbol {
    pub name: String,
    pub hash: String,
    pub file: String,
    pub line: u32,
    pub callers: u32,
    pub callees: u32,
    /// BFS distance from the target (0 = target).
    pub distance: u32,
    /// "target" | "caller" | "callee".
    pub relation: String,
}
