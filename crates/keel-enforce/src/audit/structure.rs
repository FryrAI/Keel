//! Structure dimension — file size, function size, god files, monoliths.

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{AuditFinding, AuditSeverity};

pub fn check_structure(
    store: &dyn GraphStore,
    files: Option<&[String]>,
) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    let modules = match files {
        Some(paths) => paths
            .iter()
            .flat_map(|p| {
                let nodes = store.get_nodes_in_file(p);
                nodes
                    .into_iter()
                    .find(|n| n.kind == NodeKind::Module)
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        None => store.get_all_modules(),
    };

    for module in &modules {
        let line_count = module.line_end.saturating_sub(module.line_start) + 1;
        let nodes = store.get_nodes_in_file(&module.file_path);

        // File size checks
        if line_count > 800 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Fail,
                check: "file_size".into(),
                message: format!("{} — {} lines (>800)", module.file_path, line_count),
                tip: Some("Split into focused modules under 400 lines".into()),
                file: Some(module.file_path.clone()),
                count: None,
            });
        } else if line_count > 400 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "file_size".into(),
                message: format!("{} — {} lines (>400)", module.file_path, line_count),
                tip: Some("Consider splitting into smaller modules".into()),
                file: Some(module.file_path.clone()),
                count: None,
            });
        }

        // God file: >20 symbols
        let symbol_count = nodes
            .iter()
            .filter(|n| n.kind != NodeKind::Module)
            .count();
        if symbol_count > 20 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "god_file".into(),
                message: format!(
                    "{} — {} symbols (>20)",
                    module.file_path, symbol_count
                ),
                tip: Some("Split by responsibility into focused modules".into()),
                file: Some(module.file_path.clone()),
                count: None,
            });
        }

        // Per-function checks
        for node in &nodes {
            if node.kind != NodeKind::Function {
                continue;
            }
            let fn_lines = node.line_end.saturating_sub(node.line_start) + 1;

            if fn_lines > 200 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Fail,
                    check: "function_size".into(),
                    message: format!(
                        "`{}` in {} — {} lines (>200)",
                        node.name, module.file_path, fn_lines
                    ),
                    tip: Some("Extract sub-operations into helper functions".into()),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            } else if fn_lines > 100 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "function_size".into(),
                    message: format!(
                        "`{}` in {} — {} lines (>100)",
                        node.name, module.file_path, fn_lines
                    ),
                    tip: Some("Consider breaking into smaller functions".into()),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }

            // Monolithic function: >100 lines AND >5 callees
            let callees = store
                .get_edges(node.id, EdgeDirection::Outgoing)
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count();
            if fn_lines > 100 && callees > 5 {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Warn,
                    check: "monolithic_function".into(),
                    message: format!(
                        "`{}` in {} — {} lines, {} callees",
                        node.name, module.file_path, fn_lines, callees
                    ),
                    tip: Some(
                        "Monolithic function: extract sub-operations or split responsibilities"
                            .into(),
                    ),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }
        }
    }

    findings
}
