use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub version: String,
    pub command: String,
    pub total_score: u32,
    pub max_score: u32,
    pub dimensions: Vec<AuditDimension>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditDimension {
    pub name: String,
    pub score: u32,
    pub max_score: u32,
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFinding {
    pub severity: AuditSeverity,
    pub check: String,
    pub message: String,
    pub tip: Option<String>,
    /// File path, if the finding is file-specific.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Count, for aggregate findings (e.g. "12 functions missing type hints").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AuditSeverity {
    Pass,
    Tip,
    Warn,
    Fail,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditSeverity::Pass => write!(f, "PASS"),
            AuditSeverity::Tip => write!(f, "TIP"),
            AuditSeverity::Warn => write!(f, "WARN"),
            AuditSeverity::Fail => write!(f, "FAIL"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditOptions {
    /// Only audit files changed in git.
    pub changed_only: bool,
    /// Exit 1 on any FAIL finding.
    pub strict: bool,
    /// Minimum total score threshold (exit 1 if below).
    pub min_score: Option<u32>,
    /// Run only a specific dimension.
    pub dimension: Option<String>,
}

/// Compute dimension score from findings.
/// 3 = 0 FAIL + 0 WARN, 2 = 0 FAIL + ≤3 WARN, 1 = ≤2 FAIL, 0 = >2 FAIL
pub fn compute_dimension_score(findings: &[AuditFinding]) -> u32 {
    let fails = findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Fail)
        .count();
    let warns = findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Warn)
        .count();

    if fails == 0 && warns == 0 {
        3
    } else if fails == 0 && warns <= 3 {
        2
    } else if fails <= 2 {
        1
    } else {
        0
    }
}
