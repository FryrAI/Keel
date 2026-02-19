use super::PressureLevel;
use serde::{Deserialize, Serialize};

/// Identity key for diffing violations between compiles.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViolationKey {
    pub code: String,
    pub hash: String,
    pub file: String,
    pub line: u32,
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
