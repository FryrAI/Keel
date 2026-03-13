//! Discoverability dimension — file headers, naming, type hints, docstrings.

use std::path::Path;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{AuditFinding, AuditSeverity};

/// Language-aware comment prefix for file header detection.
fn comment_prefix(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    match ext {
        "py" => Some("#"),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "go" => Some("//"),
        _ => None,
    }
}

/// Check if file has a structured header (purpose/why/related or module docstring).
fn has_file_header(root_dir: &Path, file_path: &str) -> bool {
    let full_path = root_dir.join(file_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return true, // can't read = skip
    };

    let first_lines: Vec<&str> = content.lines().take(10).collect();
    let text = first_lines.join("\n").to_lowercase();

    // Check for structured header keywords
    text.contains("purpose") || text.contains("//!") || text.contains("\"\"\"")
        || text.contains("'''") || text.contains("@module")
        || text.contains("@file") || text.contains("@description")
}

/// Regex-free check for cryptic names.
fn is_cryptic_name(name: &str) -> bool {
    name.len() < 3
}

/// Check for generic module names that hurt discoverability.
fn is_generic_module(path: &str) -> bool {
    let file_stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    matches!(
        file_stem,
        "utils" | "helpers" | "common" | "misc" | "util" | "helper" | "tmp" | "temp"
    )
}

pub fn check_discoverability(
    store: &dyn GraphStore,
    root_dir: &Path,
    files: Option<&[String]>,
) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    let modules = match files {
        Some(paths) => paths
            .iter()
            .flat_map(|p| {
                store
                    .get_nodes_in_file(p)
                    .into_iter()
                    .find(|n| n.kind == NodeKind::Module)
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        None => store.get_all_modules(),
    };

    let mut missing_type_hints = 0u32;
    let mut missing_docstrings = 0u32;
    let mut missing_headers = Vec::new();

    for module in &modules {
        let path = &module.file_path;
        let nodes = store.get_nodes_in_file(path);

        // File header check
        if comment_prefix(path).is_some() && !has_file_header(root_dir, path) {
            missing_headers.push(path.clone());
        }

        // Generic module name
        if is_generic_module(path) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "generic_module".into(),
                message: format!("{} — generic module name", path),
                tip: Some(
                    "Rename to describe responsibility (e.g., string_utils.py, date_helpers.py)"
                        .into(),
                ),
                file: Some(path.clone()),
                count: None,
            });
        }

        let mut public_count = 0u32;
        let mut total_count = 0u32;

        for node in &nodes {
            if node.kind == NodeKind::Module {
                continue;
            }
            total_count += 1;
            if node.is_public {
                public_count += 1;
            }

            // Cryptic name
            if node.kind == NodeKind::Function && is_cryptic_name(&node.name) {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "cryptic_name".into(),
                    message: format!("`{}` in {} — name too short (<3 chars)", node.name, path),
                    tip: Some("Use descriptive names for agent comprehension".into()),
                    file: Some(path.clone()),
                    count: None,
                });
            }

            // Missing type hints on public functions
            if node.kind == NodeKind::Function && node.is_public && !node.type_hints_present {
                missing_type_hints += 1;
            }

            // Missing docstrings on public functions with >3 callers
            if node.kind == NodeKind::Function && node.is_public && !node.has_docstring {
                let callers = store
                    .get_edges(node.id, EdgeDirection::Incoming)
                    .iter()
                    .filter(|e| e.kind == EdgeKind::Calls)
                    .count();
                if callers > 3 {
                    missing_docstrings += 1;
                }
            }
        }

        // Public API surface ratio
        if total_count > 5 && public_count > 0 {
            let ratio = public_count as f64 / total_count as f64;
            if ratio > 0.8 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Tip,
                    check: "public_ratio".into(),
                    message: format!(
                        "{} — {:.0}% public ({}/{})",
                        path,
                        ratio * 100.0,
                        public_count,
                        total_count,
                    ),
                    tip: Some("Consider marking internal helpers as private".into()),
                    file: Some(path.clone()),
                    count: None,
                });
            }
        }
    }

    // Aggregate findings
    if !missing_headers.is_empty() {
        let count = missing_headers.len() as u32;
        let example = missing_headers.first().cloned().unwrap_or_default();
        let prefix = comment_prefix(&example).unwrap_or("#");
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "missing_file_header".into(),
            message: format!("{} source files missing header comments", count),
            tip: Some(format!(
                "Add a header block to each file:\n  {} purpose: <what this file does>\n  {} why: <why it exists>\n  {} related: <related files>",
                prefix, prefix, prefix,
            )),
            file: None,
            count: Some(count),
        });
    }

    if missing_type_hints > 0 {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "missing_type_hints".into(),
            message: format!(
                "{} public functions missing type hints",
                missing_type_hints
            ),
            tip: Some("Add type annotations to improve agent comprehension".into()),
            file: None,
            count: Some(missing_type_hints),
        });
    }

    if missing_docstrings > 0 {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "missing_docstrings".into(),
            message: format!(
                "{} high-use public functions missing docstrings (>3 callers)",
                missing_docstrings,
            ),
            tip: Some("Add docstrings to frequently-called public functions".into()),
            file: None,
            count: Some(missing_docstrings),
        });
    }

    findings
}
