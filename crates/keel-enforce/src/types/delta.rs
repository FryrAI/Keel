use super::{PressureLevel, Violation};
use serde::{Deserialize, Serialize};

/// Identity key for diffing violations between compiles.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViolationKey {
    pub code: String,
    pub hash: String,
    pub file: String,
    /// Part of the diffing identity only for a hash-less violation; display
    /// only for one that carries a hash. See `stable_identity`.
    pub line: u32,
}

impl ViolationKey {
    /// The key for a violation, keeping its line for display.
    pub fn from_violation(v: &Violation) -> Self {
        Self {
            code: v.code.clone(),
            hash: v.hash.clone(),
            file: v.file.clone(),
            line: v.line,
        }
    }

    /// The identity that survives a pure line shift — see `stable_identity`.
    pub fn stable(&self) -> (&str, &str, &str, u32) {
        stable_identity(&self.code, &self.hash, &self.file, self.line)
    }
}

/// The identity a violation keeps across two runs of the same revision.
///
/// `(code, hash, file)` for a violation that carries a hash, plus `line` for
/// one that does not.
///
/// Every delta keel ships across two runs of the *same* revision —
/// `compile --delta` and the adoption metrics behind it — compares violations
/// with this key. Including `line` unconditionally made inserting a single line
/// at the top of a file report every violation below it as brand new, which is
/// exactly the noise that makes a delta unusable. The hash is AST-derived from
/// the signature, normalized body and docstring, so it is stable under
/// reformatting and travels with the code it describes — hash-bearing
/// violations are therefore line-independent here.
///
/// Violations that carry **no** hash — W006, W005 and W007 on code the graph
/// has no node for, which is every finding in a file created since the last
/// `keel map` — have nothing else to tell them apart, so `line` is the
/// discriminator. Without it every same-code hash-less finding in one file
/// collapses onto a single key: `--delta` counts one where there are five, and
/// `--format github` marks all five new the moment any one of them is. The
/// trade is the historical pre-v0.5 behavior for exactly those findings: a pure
/// line shift of a hash-less finding reads as one new plus one resolved.
///
/// Diffing across two *revisions* is a different question with a different key
/// — see `crate::review::baseline`.
pub fn stable_identity<'a>(
    code: &'a str,
    hash: &'a str,
    file: &'a str,
    line: u32,
) -> (&'a str, &'a str, &'a str, u32) {
    (code, hash, file, if hash.is_empty() { line } else { 0 })
}

/// Delta between two compile runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDelta {
    pub new_errors: Vec<ViolationKey>,
    pub resolved_errors: Vec<ViolationKey>,
    pub new_warnings: Vec<ViolationKey>,
    pub resolved_warnings: Vec<ViolationKey>,
    pub net_errors: i32,
    pub net_warnings: i32,
    pub pressure: PressureLevel,
    pub total_errors: u32,
    pub total_warnings: u32,
}

/// Code snippet context for discover --context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyContext {
    pub lines: String,
    pub line_count: u32,
    pub truncated: bool,
}
