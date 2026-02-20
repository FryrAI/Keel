use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub version: String,
    pub command: String,
    pub file: String,
    pub structure: FileStructure,
    pub smells: Vec<CodeSmell>,
    pub refactor_opportunities: Vec<RefactorOpportunity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStructure {
    pub line_count: u32,
    pub function_count: u32,
    pub class_count: u32,
    pub functions: Vec<StructureEntry>,
    pub classes: Vec<StructureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureEntry {
    pub name: String,
    pub hash: String,
    pub line_start: u32,
    pub line_end: u32,
    pub lines: u32,
    pub callers: u32,
    pub callees: u32,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSmell {
    pub kind: SmellKind,
    pub severity: String, // "INFO" | "WARNING"
    pub message: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmellKind {
    Monolith,
    Oversized,
    Isolated,
    HighFanIn,
    HighFanOut,
    NoDocstring,
    NoTypeHints,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorOpportunity {
    pub kind: RefactorKind,
    pub message: String,
    pub target: Option<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorKind {
    ExtractFunction,
    MoveToModule,
    InlineFunction,
    SplitFile,
    StabilizeApi,
}
