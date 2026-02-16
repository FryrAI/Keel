use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub version: String,
    pub command: String,
    pub status: String,  // "ok" | "error" | "warning"
    pub files_analyzed: Vec<String>,
    pub errors: Vec<Violation>,
    pub warnings: Vec<Violation>,
    pub info: CompileInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub code: String,
    pub severity: String,  // "ERROR" | "WARNING" | "INFO"
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
    pub suggested_module: Option<String>,  // W001 only
    pub existing: Option<ExistingNode>,    // W002 only
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
    pub kind: String,  // "import", "call", "type_ref", "re_export"
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

// --- Fix command types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub version: String,
    pub command: String,
    pub violations_addressed: u32,
    pub files_affected: u32,
    pub plans: Vec<FixPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixPlan {
    pub code: String,
    pub hash: String,
    pub category: String,
    pub target_name: String,
    pub cause: String,
    pub actions: Vec<FixAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixAction {
    pub file: String,
    pub line: u32,
    pub old_text: String,
    pub new_text: String,
    pub description: String,
}

// --- Fix apply result types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixApplyResult {
    pub version: String,
    pub command: String,
    pub actions_applied: u32,
    pub actions_failed: u32,
    pub files_modified: Vec<String>,
    pub recompile_clean: bool,
    pub recompile_errors: u32,
    pub details: Vec<FixApplyDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixApplyDetail {
    pub file: String,
    pub line: u32,
    pub status: String,  // "applied" | "failed"
    pub error: Option<String>,
}

// --- Name command types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameResult {
    pub version: String,
    pub command: String,
    pub description: String,
    pub suggestions: Vec<NameSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameSuggestion {
    pub location: String,
    pub score: f64,
    pub keywords: Vec<String>,
    pub alternatives: Vec<NameAlternative>,
    pub insert_after: Option<String>,
    pub insert_line: Option<u32>,
    pub convention: String,
    pub suggested_name: String,
    pub likely_imports: Vec<String>,
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameAlternative {
    pub location: String,
    pub score: f64,
    pub keywords: Vec<String>,
}

// --- Backpressure types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressureLevel {
    Low,
    Med,
    High,
}

impl std::fmt::Display for PressureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PressureLevel::Low => write!(f, "LOW"),
            PressureLevel::Med => write!(f, "MED"),
            PressureLevel::High => write!(f, "HIGH"),
        }
    }
}

impl PressureLevel {
    pub fn from_error_count(errors: usize) -> Self {
        match errors {
            0..=2 => PressureLevel::Low,
            3..=5 => PressureLevel::Med,
            _ => PressureLevel::High,
        }
    }

    pub fn budget_directive(&self) -> &'static str {
        match self {
            PressureLevel::Low => "keep_going",
            PressureLevel::Med => "fix_before_adding_more",
            PressureLevel::High => "stop_generating",
        }
    }
}
