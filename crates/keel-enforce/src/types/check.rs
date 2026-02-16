use serde::{Serialize, Deserialize};
use super::{NodeInfo, Violation, ModuleContext};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub version: String,
    pub command: String,
    pub target: NodeInfo,
    pub risk: RiskAssessment,
    pub violations: Vec<Violation>,
    pub suggestions: Vec<CheckSuggestion>,
    pub module_context: ModuleContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub level: String,  // "low" | "medium" | "high" — structural exposure
    pub health: String, // "clean" | "issues" — code quality (violations present)
    pub caller_count: u32,
    pub cross_file_callers: u32,
    pub cross_module_callers: u32,
    pub callee_count: u32,
    pub local_callees: u32,
    pub is_public_api: bool,
    pub callers: Vec<CheckCallerRef>,
    pub callees: Vec<CheckCalleeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckCallerRef {
    pub hash: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckCalleeRef {
    pub hash: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSuggestion {
    pub kind: String,  // "inline_candidate" | "high_fan_in" | "cross_module_impact"
    pub message: String,
    pub related_hash: Option<String>,
}
