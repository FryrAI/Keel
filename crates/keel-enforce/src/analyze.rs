//! File analysis: structure, code smells, and refactoring opportunities.

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::types::{
    AnalyzeResult, CodeSmell, FileStructure, RefactorKind, RefactorOpportunity, SmellKind,
    StructureEntry,
};

/// Analyze a file for structure, smells, and refactoring opportunities.
/// Reads from stored graph data only, no re-parsing.
pub fn analyze_file(store: &dyn GraphStore, file_path: &str) -> Option<AnalyzeResult> {
    let nodes = store.get_nodes_in_file(file_path);
    if nodes.is_empty() {
        return None;
    }

    let module = nodes.iter().find(|n| n.kind == NodeKind::Module)?;
    let line_count = module.line_end.saturating_sub(module.line_start) + 1;

    let mut functions = Vec::new();
    let mut classes = Vec::new();

    for node in &nodes {
        if node.kind == NodeKind::Module {
            continue;
        }

        let callers = store
            .get_edges(node.id, EdgeDirection::Incoming)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count() as u32;
        let callees = store
            .get_edges(node.id, EdgeDirection::Outgoing)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count() as u32;
        let lines = node.line_end.saturating_sub(node.line_start) + 1;

        let entry = StructureEntry {
            name: node.name.clone(),
            hash: node.hash.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            lines,
            callers,
            callees,
            is_public: node.is_public,
        };

        match node.kind {
            NodeKind::Function => functions.push(entry),
            NodeKind::Class => classes.push(entry),
            _ => {}
        }
    }

    let structure = FileStructure {
        line_count,
        function_count: functions.len() as u32,
        class_count: classes.len() as u32,
        functions: functions.clone(),
        classes: classes.clone(),
    };

    let smells = detect_smells(line_count, &functions, &classes, &nodes, store);
    let refactor_ops = detect_refactoring(file_path, line_count, &functions, &nodes, store);

    Some(AnalyzeResult {
        version: "0.1.0".to_string(),
        command: "analyze".to_string(),
        file: file_path.to_string(),
        structure,
        smells,
        refactor_opportunities: refactor_ops,
    })
}

fn detect_smells(
    line_count: u32,
    functions: &[StructureEntry],
    _classes: &[StructureEntry],
    nodes: &[keel_core::types::GraphNode],
    store: &dyn GraphStore,
) -> Vec<CodeSmell> {
    let mut smells = Vec::new();

    // Oversized file: >400 lines
    if line_count > 400 {
        smells.push(CodeSmell {
            kind: SmellKind::Oversized,
            severity: "WARNING".to_string(),
            message: format!("File has {} lines (>400) — consider splitting", line_count),
            target: None,
        });
    }

    for f in functions {
        // Oversized function: >100 lines
        if f.lines > 100 {
            smells.push(CodeSmell {
                kind: SmellKind::Oversized,
                severity: "WARNING".to_string(),
                message: format!("`{}` is {} lines (>100)", f.name, f.lines),
                target: Some(f.name.clone()),
            });
        }

        // High fan-in: >10 callers
        if f.callers > 10 {
            smells.push(CodeSmell {
                kind: SmellKind::HighFanIn,
                severity: "INFO".to_string(),
                message: format!("`{}` has {} callers (>10) — high impact", f.name, f.callers),
                target: Some(f.name.clone()),
            });
        }

        // High fan-out: >10 callees
        if f.callees > 10 {
            smells.push(CodeSmell {
                kind: SmellKind::HighFanOut,
                severity: "WARNING".to_string(),
                message: format!("`{}` calls {} functions (>10) — monolithic", f.name, f.callees),
                target: Some(f.name.clone()),
            });
        }

        // Monolith: >100 lines AND >5 callees
        if f.lines > 100 && f.callees > 5 {
            smells.push(CodeSmell {
                kind: SmellKind::Monolith,
                severity: "WARNING".to_string(),
                message: format!(
                    "`{}` is {} lines with {} callees — monolithic function",
                    f.name, f.lines, f.callees,
                ),
                target: Some(f.name.clone()),
            });
        }
    }

    // Check for isolated module (all outgoing calls target same file)
    let non_module: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .collect();
    let all_local = non_module.iter().all(|n| {
        store
            .get_edges(n.id, EdgeDirection::Outgoing)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .all(|e| {
                store
                    .get_node_by_id(e.target_id)
                    .map(|t| t.file_path == n.file_path)
                    .unwrap_or(true)
            })
    });
    if all_local && !non_module.is_empty() {
        let has_any_calls = non_module.iter().any(|n| {
            store
                .get_edges(n.id, EdgeDirection::Outgoing)
                .iter()
                .any(|e| e.kind == EdgeKind::Calls)
        });
        if has_any_calls {
            smells.push(CodeSmell {
                kind: SmellKind::Isolated,
                severity: "INFO".to_string(),
                message: "All call edges are local — module is self-contained".to_string(),
                target: None,
            });
        }
    }

    // No-docstring and no-type-hints on public functions
    for n in &non_module {
        if n.kind == NodeKind::Function && n.is_public {
            if !n.has_docstring {
                smells.push(CodeSmell {
                    kind: SmellKind::NoDocstring,
                    severity: "INFO".to_string(),
                    message: format!("Public function `{}` has no docstring", n.name),
                    target: Some(n.name.clone()),
                });
            }
            if !n.type_hints_present {
                smells.push(CodeSmell {
                    kind: SmellKind::NoTypeHints,
                    severity: "INFO".to_string(),
                    message: format!("Public function `{}` has no type hints", n.name),
                    target: Some(n.name.clone()),
                });
            }
        }
    }

    smells
}

fn detect_refactoring(
    file_path: &str,
    line_count: u32,
    functions: &[StructureEntry],
    nodes: &[keel_core::types::GraphNode],
    store: &dyn GraphStore,
) -> Vec<RefactorOpportunity> {
    let mut ops = Vec::new();

    // SplitFile: file >400 lines
    if line_count > 400 {
        ops.push(RefactorOpportunity {
            kind: RefactorKind::SplitFile,
            message: format!(
                "File is {} lines — split into focused modules",
                line_count,
            ),
            target: None,
            rationale: "Files over 400 lines reduce readability and increase merge conflicts"
                .to_string(),
        });
    }

    for f in functions {
        // ExtractFunction: >100 lines, >5 callees
        if f.lines > 100 && f.callees > 5 {
            ops.push(RefactorOpportunity {
                kind: RefactorKind::ExtractFunction,
                message: format!(
                    "`{}` ({} lines, {} callees) — extract sub-operations",
                    f.name, f.lines, f.callees,
                ),
                target: Some(f.name.clone()),
                rationale: "Long functions with many dependencies are hard to test and modify"
                    .to_string(),
            });
        }

        // InlineFunction: callee with exactly 1 caller, both same file
        if f.callers == 1 {
            // Check if the single caller is in the same file
            if let Some(node) = nodes.iter().find(|n| n.hash == f.hash) {
                let caller_edges = store.get_edges(node.id, EdgeDirection::Incoming);
                let single_caller = caller_edges
                    .iter()
                    .filter(|e| e.kind == EdgeKind::Calls)
                    .filter_map(|e| store.get_node_by_id(e.source_id))
                    .next();
                if let Some(caller) = single_caller {
                    if caller.file_path == file_path {
                        ops.push(RefactorOpportunity {
                            kind: RefactorKind::InlineFunction,
                            message: format!(
                                "`{}` has only 1 caller (`{}`) in same file — consider inlining",
                                f.name, caller.name,
                            ),
                            target: Some(f.name.clone()),
                            rationale: "Single-use helper functions add indirection without reuse benefit".to_string(),
                        });
                    }
                }
            }
        }

        // MoveToModule: >50% callers in other files
        if f.callers >= 2 {
            if let Some(node) = nodes.iter().find(|n| n.hash == f.hash) {
                let caller_edges = store.get_edges(node.id, EdgeDirection::Incoming);
                let call_edges: Vec<_> = caller_edges
                    .iter()
                    .filter(|e| e.kind == EdgeKind::Calls)
                    .collect();
                let external = call_edges
                    .iter()
                    .filter(|e| {
                        store
                            .get_node_by_id(e.source_id)
                            .map(|n| n.file_path != file_path)
                            .unwrap_or(false)
                    })
                    .count();
                if external * 2 > call_edges.len() {
                    ops.push(RefactorOpportunity {
                        kind: RefactorKind::MoveToModule,
                        message: format!(
                            "`{}` has {}/{} callers from other files — consider moving closer",
                            f.name, external, call_edges.len(),
                        ),
                        target: Some(f.name.clone()),
                        rationale:
                            "Functions used mostly by external modules create unnecessary coupling"
                                .to_string(),
                    });
                }
            }
        }

        // StabilizeApi: public, >5 callers, no docstring
        if f.is_public && f.callers > 5 {
            if let Some(node) = nodes.iter().find(|n| n.hash == f.hash) {
                if !node.has_docstring {
                    ops.push(RefactorOpportunity {
                        kind: RefactorKind::StabilizeApi,
                        message: format!(
                            "`{}` is public with {} callers but no docstring — stabilize API",
                            f.name, f.callers,
                        ),
                        target: Some(f.name.clone()),
                        rationale:
                            "High-use public functions need clear contracts to prevent breakage"
                                .to_string(),
                    });
                }
            }
        }
    }

    ops
}
