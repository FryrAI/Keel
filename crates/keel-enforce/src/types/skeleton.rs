//! Result types for `keel skeleton` (issue #21) — a compressed, signature-only
//! view of a single file.

use serde::{Deserialize, Serialize};

/// Compressed signature-only view of a file: imports plus function/class
/// signatures (no bodies), ranked public-first then by line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletonResult {
    pub version: String,
    pub command: String,
    pub file: String,
    pub language: String,
    /// Import source specifiers in source order (deduped).
    pub imports: Vec<String>,
    /// Function/class signatures, ranked public-first then by line.
    pub symbols: Vec<SkeletonSymbol>,
}

/// A single signature entry in a skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletonSymbol {
    /// "function" | "class".
    pub kind: String,
    pub name: String,
    /// Canonical signature (no body).
    pub signature: String,
    pub is_public: bool,
    pub line: u32,
    /// Present only when `--docs` was requested and a docstring exists.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub docstring: Option<String>,
}
