use super::{PressureLevel, Violation};
use serde::{Deserialize, Serialize};

/// Identity key for diffing violations between compiles.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViolationKey {
    pub code: String,
    pub hash: String,
    pub file: String,
    /// Display only — deliberately **not** part of the diffing identity. See
    /// `ViolationKey::stable`.
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

    /// The identity that survives a pure line shift: `(code, hash, file)`.
    ///
    /// Every delta keel ships across two runs of the *same* revision —
    /// `compile --delta` and the adoption metrics behind it — compares
    /// violations with this key. Including `line` made inserting a single line
    /// at the top of a file report every violation below it as brand new,
    /// which is exactly the noise that makes a delta unusable. The hash is
    /// AST-derived from the signature, normalized body and docstring, so it is
    /// stable under reformatting and travels with the code it describes.
    ///
    /// Diffing across two *revisions* is a different question with a different
    /// key — see `crate::review::baseline`.
    ///
    /// Violations that carry **no** hash — W006, and W005/W007 on code the
    /// graph has never stored — collapse onto one key per `(code, file)`.
    /// Those are by construction findings on unmapped code; under-counting
    /// them is the deliberate trade for not resurrecting line sensitivity.
    pub fn stable(&self) -> (&str, &str, &str) {
        (&self.code, &self.hash, &self.file)
    }
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
