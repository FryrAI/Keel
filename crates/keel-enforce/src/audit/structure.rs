//! Structure dimension — file size, function size, god files, monoliths.

use std::path::Path;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{AuditFinding, AuditSeverity};

pub fn check_structure(
    store: &dyn GraphStore,
    root_dir: &Path,
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
                tip: Some(format!(
                    "This file is too large for agents to reason about. Run \
                     `keel analyze {}` to identify natural split points, then extract \
                     cohesive groups into separate modules under 400 lines.",
                    module.file_path,
                )),
                file: Some(module.file_path.clone()),
                count: None,
            });
        } else if line_count > 400 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "file_size".into(),
                message: format!("{} — {} lines (>400)", module.file_path, line_count),
                tip: Some(format!(
                    "Run `keel analyze {}` to see function groupings and identify split \
                     points before this file grows past 800 lines.",
                    module.file_path,
                )),
                file: Some(module.file_path.clone()),
                count: None,
            });
        }

        // God file: >20 symbols, FAIL at >35
        let symbol_count = nodes.iter().filter(|n| n.kind != NodeKind::Module).count();
        if symbol_count > 35 {
            findings.push(AuditFinding {
                severity: AuditSeverity::Fail,
                check: "god_file".into(),
                message: format!("{} — {} symbols (>35)", module.file_path, symbol_count),
                tip: Some(format!(
                    "Run `keel discover {}` to list all {} symbols. Group related \
                     functions and extract each group into a new module of <20 symbols.",
                    module.file_path, symbol_count,
                )),
                file: Some(module.file_path.clone()),
                count: None,
            });
        } else if symbol_count > 20 {
            let stem = std::path::Path::new(&module.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("module");
            findings.push(AuditFinding {
                severity: AuditSeverity::Warn,
                check: "god_file".into(),
                message: format!("{} — {} symbols (>20)", module.file_path, symbol_count),
                tip: Some(format!(
                    "Run `keel discover {}` to see symbol groupings. Extract related \
                     functions into a new module (e.g., {}_ops.rs or {}_helpers.rs).",
                    module.file_path, stem, stem,
                )),
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
                    tip: Some(format!(
                        "Extract sub-operations from `{}`. Run `keel discover {}` to see \
                         what it calls and group related operations into helper functions.",
                        node.name, module.file_path,
                    )),
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
                    tip: Some(format!(
                        "Run `keel check {}` to assess refactoring impact, then extract \
                         sub-operations from `{}` into focused helpers.",
                        node.hash, node.name,
                    )),
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
                    tip: Some(format!(
                        "Function `{}` is both long and deeply connected ({} callees). \
                         Run `keel discover --name {}` to see its call graph, then \
                         extract sub-operations.",
                        node.name, callees, node.name,
                    )),
                    file: Some(module.file_path.clone()),
                    count: None,
                });
            }
        }
    }

    // Dev workflow tooling check
    let workflow_files: &[&str] = &[
        "Makefile",
        "makefile",
        "GNUmakefile",
        "Justfile",
        "justfile",
        "taskfile.yml",
        "Taskfile.yml",
    ];
    let has_workflow_file = workflow_files.iter().any(|f| root_dir.join(f).exists());
    let has_npm_scripts = root_dir.join("package.json").exists() && {
        std::fs::read_to_string(root_dir.join("package.json"))
            .map(|c| c.contains("\"scripts\""))
            .unwrap_or(false)
    };
    let has_scripts_dir = root_dir.join("scripts").is_dir();
    if !has_workflow_file && !has_npm_scripts && !has_scripts_dir {
        findings.push(AuditFinding {
            severity: AuditSeverity::Warn,
            check: "no_dev_workflow_tool".into(),
            message: "No dev workflow automation found (Makefile, Justfile, scripts/)".into(),
            tip: Some(
                "Create a Makefile, Justfile, or scripts/ directory with common dev commands. \
                 Example: make test, make lint, make build. Agents need discoverable CLI \
                 commands for every workflow — GUIs and tribal knowledge don't work."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    // Layer-based organization detection
    let layer_dirs: &[&str] = &[
        "models",
        "views",
        "controllers",
        "services",
        "routes",
        "handlers",
        "middleware",
        "serializers",
    ];
    let layer_count = layer_dirs
        .iter()
        .filter(|d| root_dir.join(d).is_dir())
        .count();
    if layer_count >= 3 {
        findings.push(AuditFinding {
            severity: AuditSeverity::Tip,
            check: "layer_organization".into(),
            message: format!(
                "Detected {} layer-based directories (models/, views/, etc.)",
                layer_count,
            ),
            tip: Some(
                "Layer-based organization (models/, views/, controllers/) scatters features \
                 across directories. Consider grouping by feature instead (experiments/, \
                 users/, billing/) where each feature dir contains its own models, routes, \
                 and schemas. Agents modify fewer directories per task."
                    .into(),
            ),
            file: None,
            count: None,
        });
    }

    findings
}
