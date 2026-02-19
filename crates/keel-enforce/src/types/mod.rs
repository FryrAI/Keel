mod analyze;
mod check;
mod delta;
mod fix_name;

pub use analyze::*;
pub use check::*;
pub use delta::*;
pub use fix_name::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub version: String,
    pub command: String,
    pub status: String, // "ok" | "error" | "warning"
    pub files_analyzed: Vec<String>,
    pub errors: Vec<Violation>,
    pub warnings: Vec<Violation>,
    pub info: CompileInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub code: String,
    pub severity: String, // "ERROR" | "WARNING" | "INFO"
    pub category: String,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub hash: String,
    pub confidence: f64,
    pub resolution_tier: String,
    pub fix_hint: Option<String>,
    pub suppressed: bool,
    pub suppress_hint: Option<String>,
    pub affected: Vec<AffectedNode>,
    pub suggested_module: Option<String>, // W001 only
    pub existing: Option<ExistingNode>,   // W002 only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedNode {
    pub hash: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingNode {
    pub hash: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileInfo {
    pub nodes_updated: u32,
    pub edges_updated: u32,
    pub hashes_changed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResult {
    pub version: String,
    pub command: String,
    pub target: NodeInfo,
    pub upstream: Vec<CallerInfo>,
    pub downstream: Vec<CalleeInfo>,
    pub module_context: ModuleContext,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub body_context: Option<BodyContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub hash: String,
    pub name: String,
    pub signature: String,
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub docstring: Option<String>,
    pub type_hints_present: bool,
    pub has_docstring: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerInfo {
    pub hash: String,
    pub name: String,
    pub signature: String,
    pub file: String,
    pub line: u32,
    pub docstring: Option<String>,
    pub call_line: u32,
    /// BFS distance from target node (1 = direct caller)
    #[serde(default = "default_distance")]
    pub distance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalleeInfo {
    pub hash: String,
    pub name: String,
    pub signature: String,
    pub file: String,
    pub line: u32,
    pub docstring: Option<String>,
    pub call_line: u32,
    /// BFS distance from target node (1 = direct callee)
    #[serde(default = "default_distance")]
    pub distance: u32,
}

fn default_distance() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleContext {
    pub module: String,
    pub sibling_functions: Vec<String>,
    pub responsibility_keywords: Vec<String>,
    pub function_count: u32,
    pub external_endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainResult {
    pub version: String,
    pub command: String,
    pub error_code: String,
    pub hash: String,
    pub confidence: f64,
    pub resolution_tier: String,
    pub resolution_chain: Vec<ResolutionStep>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionStep {
    pub kind: String, // "import", "call", "type_ref", "re_export"
    pub file: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapResult {
    pub version: String,
    pub command: String,
    pub summary: MapSummary,
    pub modules: Vec<ModuleEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hotspots: Vec<HotspotEntry>,
    #[serde(default = "default_depth")]
    pub depth: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub functions: Vec<FunctionEntry>,
}

fn default_depth() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotEntry {
    pub path: String,
    pub name: String,
    pub hash: String,
    pub callers: u32,
    pub callees: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionEntry {
    pub hash: String,
    pub name: String,
    pub signature: String,
    pub file: String,
    pub line: u32,
    pub callers: u32,
    pub callees: u32,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSummary {
    pub total_nodes: u32,
    pub total_edges: u32,
    pub modules: u32,
    pub functions: u32,
    pub classes: u32,
    pub external_endpoints: u32,
    pub languages: Vec<String>,
    pub type_hint_coverage: f64,
    pub docstring_coverage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleEntry {
    pub path: String,
    pub function_count: u32,
    pub class_count: u32,
    pub edge_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responsibility_keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_endpoints: Option<Vec<String>>,
    /// Function names with hashes for agent-friendly module listings.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub function_names: Vec<ModuleFunctionRef>,
}

/// Lightweight function reference within a module entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleFunctionRef {
    pub name: String,
    pub hash: String,
    pub callers: u32,
    pub callees: u32,
}
