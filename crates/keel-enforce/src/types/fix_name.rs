use serde::{Deserialize, Serialize};

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
    pub status: String, // "applied" | "failed"
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
    /// Determine the pressure level based on the number of active errors.
    pub fn from_error_count(errors: usize) -> Self {
        match errors {
            0..=2 => PressureLevel::Low,
            3..=5 => PressureLevel::Med,
            _ => PressureLevel::High,
        }
    }

    /// Return the LLM budget directive string for this pressure level.
    pub fn budget_directive(&self) -> &'static str {
        match self {
            PressureLevel::Low => "keep_going",
            PressureLevel::Med => "fix_before_adding_more",
            PressureLevel::High => "stop_generating",
        }
    }
}
