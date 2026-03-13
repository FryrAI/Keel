//! Audit module — AI-readiness scorecard for codebases.
//!
//! Scores a codebase across 4 dimensions: Structure, Discoverability,
//! Navigation, and Agent Config. Each dimension scored 0–3 (max total: 12).

pub mod agent_config;
pub mod discoverability;
pub mod navigation;
pub mod structure;

use keel_core::store::GraphStore;

use crate::types::{
    AuditDimension, AuditOptions, AuditResult, AuditSeverity, compute_dimension_score,
};

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
            .map_or(true, |d| d.eq_ignore_ascii_case(name))
    };

    if run_dim("structure") {
        let findings = structure::check_structure(store, files);
        let score = compute_dimension_score(&findings);
        dimensions.push(AuditDimension {
            name: "structure".into(),
            score,
            max_score: 3,
            findings,
        });
    }

    if run_dim("discoverability") {
        let findings = discoverability::check_discoverability(store, root_dir, files);
        let score = compute_dimension_score(&findings);
        dimensions.push(AuditDimension {
            name: "discoverability".into(),
            score,
            max_score: 3,
            findings,
        });
    }

    if run_dim("navigation") {
        let findings = navigation::check_navigation(store, files);
        let score = compute_dimension_score(&findings);
        dimensions.push(AuditDimension {
            name: "navigation".into(),
            score,
            max_score: 3,
            findings,
        });
    }

    if run_dim("config") {
        let findings = agent_config::check_agent_config(root_dir);
        let score = compute_dimension_score(&findings);
        dimensions.push(AuditDimension {
            name: "config".into(),
            score,
            max_score: 3,
            findings,
        });
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

/// Returns true if the audit result should cause a non-zero exit.
pub fn should_fail(result: &AuditResult, options: &AuditOptions) -> bool {
    if options.strict {
        let has_fail = result.dimensions.iter().any(|d| {
            d.findings
                .iter()
                .any(|f| f.severity == AuditSeverity::Fail)
        });
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
