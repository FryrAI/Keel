//! Pre-edit risk assessment: "Is it safe to change this? What should I know?"

use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

use crate::engine::{node_to_info, EnforcementEngine};
use crate::types::{
    CheckCalleeRef, CheckCallerRef, CheckResult, CheckSuggestion, RiskAssessment, Violation,
};

impl EnforcementEngine {
    /// Perform a pre-edit risk assessment for a node.
    pub fn check(&self, hash: &str) -> Option<CheckResult> {
        let node = self.store.get_node(hash)?;
        let target = node_to_info(&node);

        // Collect callers
        let incoming = self.store.get_edges(node.id, EdgeDirection::Incoming);
        let caller_edges: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();

        let mut callers = Vec::new();
        let mut cross_file_callers = 0u32;
        let mut cross_module_callers = 0u32;

        for edge in &caller_edges {
            if let Some(caller) = self.store.get_node_by_id(edge.source_id) {
                if caller.file_path != node.file_path {
                    cross_file_callers += 1;
                }
                if caller.module_id != node.module_id {
                    cross_module_callers += 1;
                }
                callers.push(CheckCallerRef {
                    hash: caller.hash.clone(),
                    name: caller.name.clone(),
                    file: caller.file_path.clone(),
                    line: edge.line,
                });
            }
        }

        // Collect callees
        let outgoing = self.store.get_edges(node.id, EdgeDirection::Outgoing);
        let callee_edges: Vec<_> = outgoing
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();

        let mut callees = Vec::new();
        let mut local_callees = 0u32;

        for edge in &callee_edges {
            if let Some(callee) = self.store.get_node_by_id(edge.target_id) {
                if callee.file_path == node.file_path {
                    local_callees += 1;
                }
                callees.push(CheckCalleeRef {
                    hash: callee.hash.clone(),
                    name: callee.name.clone(),
                    file: callee.file_path.clone(),
                    line: edge.line,
                });
            }
        }

        let caller_count = callers.len() as u32;
        let callee_count = callees.len() as u32;
        let is_public_api = node.is_public;

        // Summarize callers when >= 20
        let caller_summary = if caller_count >= 20 {
            let unique_files: std::collections::HashSet<&str> =
                callers.iter().map(|c| c.file.as_str()).collect();
            Some(format!(
                "{} callers across {} files",
                caller_count,
                unique_files.len()
            ))
        } else {
            None
        };

        // Compute risk level
        let level = compute_risk_level(
            caller_count,
            cross_file_callers,
            cross_module_callers,
            is_public_api,
        );

        // Gather existing violations for this node
        let violations = self.gather_node_violations(&node);

        let health = if violations.is_empty() {
            "clean".to_string()
        } else {
            "issues".to_string()
        };

        let risk = RiskAssessment {
            level,
            health,
            caller_count,
            cross_file_callers,
            cross_module_callers,
            callee_count,
            local_callees,
            is_public_api,
            callers,
            callees,
            caller_summary,
        };

        // Generate suggestions
        let suggestions =
            self.generate_suggestions(&node, caller_count, cross_module_callers, &risk.callees);

        // Module context
        let module_context = self.build_module_context(node.module_id);

        Some(CheckResult {
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: "check".to_string(),
            target,
            risk,
            violations,
            suggestions,
            module_context,
        })
    }

    /// Gather violations relevant to a specific node (E002, E003, W001, W002).
    fn gather_node_violations(&self, node: &keel_core::types::GraphNode) -> Vec<Violation> {
        let mut violations = Vec::new();

        // E002: missing type hints
        if !node.type_hints_present && node.kind == NodeKind::Function {
            violations.push(Violation {
                code: "E002".to_string(),
                severity: "ERROR".to_string(),
                category: "missing_type_hints".to_string(),
                message: format!("`{}` is missing type annotations", node.name),
                file: node.file_path.clone(),
                line: node.line_start,
                hash: node.hash.clone(),
                confidence: 1.0,
                resolution_tier: "tree-sitter".to_string(),
                fix_hint: Some("Add type annotations to parameters and return type".to_string()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            });
        }

        // E003: missing docstring
        if !node.has_docstring && node.kind == NodeKind::Function && node.is_public {
            violations.push(Violation {
                code: "E003".to_string(),
                severity: "ERROR".to_string(),
                category: "missing_docstring".to_string(),
                message: format!("Public function `{}` has no docstring", node.name),
                file: node.file_path.clone(),
                line: node.line_start,
                hash: node.hash.clone(),
                confidence: 1.0,
                resolution_tier: "tree-sitter".to_string(),
                fix_hint: Some("Add a docstring describing the function's purpose".to_string()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            });
        }

        // W002: duplicate names
        let dupes = self
            .store
            .find_nodes_by_name(&node.name, node.kind.as_str(), &node.file_path);
        if !dupes.is_empty() {
            for dupe in &dupes {
                violations.push(Violation {
                    code: "W002".to_string(),
                    severity: "WARNING".to_string(),
                    category: "duplicate_name".to_string(),
                    message: format!(
                        "`{}` also exists in {}:{}",
                        node.name, dupe.file_path, dupe.line_start,
                    ),
                    file: node.file_path.clone(),
                    line: node.line_start,
                    hash: node.hash.clone(),
                    confidence: 0.8,
                    resolution_tier: "tree-sitter".to_string(),
                    fix_hint: Some("Consider renaming to avoid ambiguity".to_string()),
                    suppressed: false,
                    suppress_hint: None,
                    affected: vec![],
                    suggested_module: None,
                    existing: Some(crate::types::ExistingNode {
                        hash: dupe.hash.clone(),
                        file: dupe.file_path.clone(),
                        line: dupe.line_start,
                    }),
                });
            }
        }

        violations
    }

    /// Generate actionable suggestions based on graph analysis.
    fn generate_suggestions(
        &self,
        node: &keel_core::types::GraphNode,
        caller_count: u32,
        cross_module_callers: u32,
        callees: &[CheckCalleeRef],
    ) -> Vec<CheckSuggestion> {
        let mut suggestions = Vec::new();

        // Inline candidate: callee with exactly 1 caller (this node), same file
        for c in callees {
            if c.file == node.file_path {
                if let Some(callee_node) = self.store.get_node(&c.hash) {
                    let callee_callers = self
                        .store
                        .get_edges(callee_node.id, EdgeDirection::Incoming)
                        .iter()
                        .filter(|e| e.kind == EdgeKind::Calls)
                        .count();
                    if callee_callers == 1 {
                        suggestions.push(CheckSuggestion {
                            kind: "inline_candidate".to_string(),
                            message: format!(
                                "`{}` has only 1 caller (this function) — consider inlining",
                                c.name,
                            ),
                            related_hash: Some(c.hash.clone()),
                        });
                    }
                }
            }
        }

        // High fan-in warning
        if caller_count >= 4 {
            suggestions.push(CheckSuggestion {
                kind: "high_fan_in".to_string(),
                message: format!(
                    "{} callers — changes will have wide impact, test thoroughly",
                    caller_count,
                ),
                related_hash: None,
            });
        }

        // Cross-module impact
        if cross_module_callers > 0 {
            suggestions.push(CheckSuggestion {
                kind: "cross_module_impact".to_string(),
                message: format!(
                    "{} caller(s) from other modules — signature changes may break external code",
                    cross_module_callers,
                ),
                related_hash: None,
            });
        }

        suggestions
    }
}

/// Structural risk: based on how many things break if you change this node.
/// - low: 0 callers — safe to change, nothing depends on it
/// - medium: local callers only (same file)
/// - high: cross-file or cross-module callers, or high fan-in (>=4)
fn compute_risk_level(
    caller_count: u32,
    cross_file: u32,
    cross_module: u32,
    _is_public: bool,
) -> String {
    if cross_file > 0 || cross_module > 0 || caller_count >= 4 {
        "high".to_string()
    } else if caller_count >= 1 {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}
