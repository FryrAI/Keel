//! Result types for `keel focus` (issue #20) — the minimal context set for
//! safely modifying a target.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A symbol's role relative to the focus target.
///
/// Replaces the earlier stringly `"target"|"callee"|"caller"` fields so a typo
/// can no longer slip past a `_` match arm. The wire format is unchanged:
/// `rename_all = "lowercase"` serializes to exactly those strings, and
/// [`Relation::as_str`]/[`fmt::Display`] render them for the text formatters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Target,
    Callee,
    Caller,
}

impl Relation {
    /// The lowercase wire string for this relation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Relation::Target => "target",
            Relation::Callee => "callee",
            Relation::Caller => "caller",
        }
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    /// Role of this file's nearest symbol (target / callee / caller).
    pub role: Relation,
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
    /// This symbol's role relative to the target.
    pub relation: Relation,
}
