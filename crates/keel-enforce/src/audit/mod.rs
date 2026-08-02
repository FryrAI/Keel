//! Audit module — AI-readiness scorecard for codebases.
//!
//! Scores a codebase across 5 dimensions: Structure, Discoverability,
//! Navigation, Agent Config, and Verification. Each dimension scored 0–5 (max total: 25).

pub mod agent_config;
pub mod discoverability;
pub mod navigation;
pub mod rank;
pub mod structure;
pub mod verification;

use keel_core::store::GraphStore;

pub use rank::ranked_findings;

use crate::types::{
    compute_dimension_score, AuditDimension, AuditFinding, AuditOptions, AuditResult, AuditSeverity,
};

/// Build one scored dimension from its raw findings.
///
/// Every dimension goes through here so the two things that must happen before
/// a score exists — deduplication and ranking — cannot be forgotten by a new
/// check: duplicates would inflate the FAIL count and depress the score, and
/// scan order would decide what the reader sees first.
fn scored_dimension(name: &str, mut findings: Vec<AuditFinding>) -> AuditDimension {
    rank::dedup_findings(&mut findings);
    let score = compute_dimension_score(&findings);
    let mut dim = AuditDimension {
        name: name.into(),
        score,
        max_score: 5,
        findings,
    };
    rank::sort_dimension(&mut dim);
    dim
}

/// Run a full audit of the repository and return a scored result.
pub fn audit_repo(
    store: &dyn GraphStore,
    root_dir: &std::path::Path,
    options: &AuditOptions,
    files: Option<&[String]>,
) -> AuditResult {
    let mut dimensions = Vec::new();

    let run_dim = |name: &str| {
        options
            .dimension
            .as_ref()
            .is_none_or(|d| d.eq_ignore_ascii_case(name))
    };

    if run_dim("structure") {
        dimensions.push(scored_dimension(
            "structure",
            structure::check_structure(store, root_dir, files),
        ));
    }

    if run_dim("discoverability") {
        dimensions.push(scored_dimension(
            "discoverability",
            discoverability::check_discoverability(store, root_dir, files),
        ));
    }

    if run_dim("navigation") {
        dimensions.push(scored_dimension(
            "navigation",
            navigation::check_navigation(store, files, options.strict_cycles),
        ));
    }

    if run_dim("config") {
        dimensions.push(scored_dimension(
            "config",
            agent_config::check_agent_config(root_dir),
        ));
    }

    if run_dim("verification") {
        dimensions.push(scored_dimension(
            "verification",
            verification::check_verification(root_dir),
        ));
    }

    let total_score: u32 = dimensions.iter().map(|d| d.score).sum();
    let max_score: u32 = dimensions.iter().map(|d| d.max_score).sum();

    AuditResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "audit".to_string(),
        total_score,
        max_score,
        dimensions,
    }
}

/// Check if any subfolder (up to 2 levels) contains a CLAUDE.md file.
fn has_subfolder_claude_md(root_dir: &std::path::Path) -> bool {
    let entries = match std::fs::read_dir(root_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_str().unwrap_or("");
        if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
            continue;
        }
        if entry.path().join("CLAUDE.md").exists() {
            return true;
        }
        // Check one more level
        if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
            for sub in sub_entries.flatten() {
                if sub.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && sub.path().join("CLAUDE.md").exists()
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns true if the audit result should cause a non-zero exit.
pub fn should_fail(result: &AuditResult, options: &AuditOptions) -> bool {
    if options.strict {
        let has_fail = result
            .dimensions
            .iter()
            .any(|d| d.findings.iter().any(|f| f.severity == AuditSeverity::Fail));
        if has_fail {
            return true;
        }
    }
    if let Some(min) = options.min_score {
        if result.total_score < min {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
