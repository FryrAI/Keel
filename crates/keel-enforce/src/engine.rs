use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::batch::BatchState;
use crate::circuit_breaker::{BreakerAction, CircuitBreaker};
use crate::suppress::SuppressionManager;
use crate::types::{
    CalleeInfo, CallerInfo, CompileInfo, CompileResult, DiscoverResult, ExplainResult,
    ModuleContext, NodeInfo, ResolutionStep, Violation,
};
use crate::violations;

/// Core enforcement engine. Owns a GraphStore and orchestrates validation.
pub struct EnforcementEngine {
    store: Box<dyn GraphStore>,
    circuit_breaker: CircuitBreaker,
    batch_state: Option<BatchState>,
    suppressions: SuppressionManager,
}

impl EnforcementEngine {
    pub fn new(store: Box<dyn GraphStore>) -> Self {
        Self {
            store,
            circuit_breaker: CircuitBreaker::new(),
            batch_state: None,
            suppressions: SuppressionManager::new(),
        }
    }

    /// Compile (validate) a set of files. Returns violations.
    pub fn compile(&mut self, files: &[FileIndex]) -> CompileResult {
        let mut all_errors = Vec::new();
        let mut all_warnings = Vec::new();
        let mut hashes_changed = Vec::new();
        let mut nodes_updated: u32 = 0;
        let edges_updated: u32 = 0;
        let file_paths: Vec<String> = files.iter().map(|f| f.file_path.clone()).collect();

        for file in files {
            let mut file_violations = Vec::new();

            // E001: broken callers
            file_violations.extend(violations::check_broken_callers(file, &*self.store));
            // E002: missing type hints
            file_violations.extend(violations::check_missing_type_hints(file));
            // E003: missing docstring
            file_violations.extend(violations::check_missing_docstring(file));
            // E004: function removed
            file_violations.extend(violations::check_removed_functions(file, &*self.store));
            // E005: arity mismatch
            file_violations.extend(violations::check_arity_mismatch(file, &*self.store));
            // W001: placement
            file_violations.extend(violations::check_placement(file, &*self.store));
            // W002: duplicate names
            file_violations.extend(violations::check_duplicate_names(file, &*self.store));

            // Apply circuit breaker
            file_violations = self.apply_circuit_breaker(file_violations);

            // Apply suppressions
            file_violations = file_violations
                .into_iter()
                .map(|v| self.suppressions.apply(v))
                .collect();

            // Track changed hashes
            for def in &file.definitions {
                let new_hash = keel_core::hash::compute_hash(
                    &def.signature,
                    &def.body_text,
                    def.docstring.as_deref().unwrap_or(""),
                );
                let existing = self.store.get_nodes_in_file(&file.file_path);
                if let Some(node) = existing.iter().find(|n| n.name == def.name) {
                    if node.hash != new_hash {
                        hashes_changed.push(node.hash.clone());
                        nodes_updated += 1;
                    }
                } else {
                    nodes_updated += 1; // New node
                }
            }

            // Handle batch mode: defer non-structural violations
            if let Some(batch) = &mut self.batch_state {
                if batch.is_expired() {
                    // Auto-expire: flush deferred
                    let deferred = self.batch_state.take()
                        .unwrap()
                        .drain();
                    Self::partition_violations(deferred, &mut all_errors, &mut all_warnings);
                    // Non-deferred violations from this file
                    Self::partition_violations(
                        file_violations,
                        &mut all_errors,
                        &mut all_warnings,
                    );
                } else {
                    batch.touch();
                    let (immediate, deferred): (Vec<_>, Vec<_>) = file_violations
                        .into_iter()
                        .partition(|v| !BatchState::is_deferrable(&v.code));
                    for d in deferred {
                        batch.defer(d);
                    }
                    Self::partition_violations(immediate, &mut all_errors, &mut all_warnings);
                }
            } else {
                Self::partition_violations(file_violations, &mut all_errors, &mut all_warnings);
            }
        }

        let status = if !all_errors.is_empty() {
            "error"
        } else if !all_warnings.is_empty() {
            "warning"
        } else {
            "ok"
        };

        CompileResult {
            version: "0.1.0".to_string(),
            command: "compile".to_string(),
            status: status.to_string(),
            files_analyzed: file_paths,
            errors: all_errors,
            warnings: all_warnings,
            info: CompileInfo {
                nodes_updated,
                edges_updated,
                hashes_changed,
            },
        }
    }

    /// Look up a node's callers, callees, and module context.
    pub fn discover(&self, hash: &str, depth: u32) -> Option<DiscoverResult> {
        let node = self.store.get_node(hash)?;

        let mut upstream = Vec::new();
        let mut downstream = Vec::new();
        self.collect_adjacency(node.id, depth, &mut upstream, &mut downstream);

        let module_ctx = self.build_module_context(node.module_id);

        Some(DiscoverResult {
            version: "0.1.0".to_string(),
            command: "discover".to_string(),
            target: node_to_info(&node),
            upstream,
            downstream,
            module_context: module_ctx,
        })
    }

    /// Resolve hash to (file, line).
    pub fn where_hash(&self, hash: &str) -> Option<(String, u32)> {
        let node = self.store.get_node(hash)?;
        Some((node.file_path, node.line_start))
    }

    /// Show resolution reasoning for an error.
    pub fn explain(&self, error_code: &str, hash: &str) -> Option<ExplainResult> {
        let node = self.store.get_node(hash)?;
        let mut chain = Vec::new();

        // Build resolution chain based on edges
        let edges = self.store.get_edges(node.id, EdgeDirection::Both);
        for edge in &edges {
            let kind = match edge.kind {
                EdgeKind::Calls => "call",
                EdgeKind::Imports => "import",
                EdgeKind::Inherits => "type_ref",
                EdgeKind::Contains => "re_export",
            };
            chain.push(ResolutionStep {
                kind: kind.to_string(),
                file: edge.file_path.clone(),
                line: edge.line,
                text: format!("{} edge at {}:{}", kind, edge.file_path, edge.line),
            });
        }

        let confidence = if self.circuit_breaker.is_downgraded(error_code, hash) {
            0.5
        } else {
            0.92
        };

        Some(ExplainResult {
            version: "0.1.0".to_string(),
            command: "explain".to_string(),
            error_code: error_code.to_string(),
            hash: hash.to_string(),
            confidence,
            resolution_tier: "tree-sitter".to_string(),
            resolution_chain: chain,
            summary: format!(
                "{} on `{}` in {}:{}",
                error_code, node.name, node.file_path, node.line_start
            ),
        })
    }

    /// Enter batch mode: defer non-structural violations.
    pub fn batch_start(&mut self) {
        self.batch_state = Some(BatchState::new());
    }

    /// End batch mode: fire all deferred violations.
    pub fn batch_end(&mut self) -> CompileResult {
        let deferred = match self.batch_state.take() {
            Some(batch) => batch.drain(),
            None => Vec::new(),
        };

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        Self::partition_violations(deferred, &mut errors, &mut warnings);

        let status = if !errors.is_empty() {
            "error"
        } else if !warnings.is_empty() {
            "warning"
        } else {
            "ok"
        };

        CompileResult {
            version: "0.1.0".to_string(),
            command: "compile".to_string(),
            status: status.to_string(),
            files_analyzed: vec![],
            errors,
            warnings,
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        }
    }

    /// Suppress a specific error/warning code.
    pub fn suppress(&mut self, code: &str) {
        self.suppressions.suppress(code);
    }

    // -- Private helpers --

    fn apply_circuit_breaker(&mut self, violations: Vec<Violation>) -> Vec<Violation> {
        violations
            .into_iter()
            .map(|mut v| {
                if v.severity == "ERROR" {
                    let action = self.circuit_breaker.record_failure(&v.code, &v.hash);
                    match action {
                        BreakerAction::FixHint => {} // fix_hint already set
                        BreakerAction::WiderContext => {
                            v.fix_hint = Some(format!(
                                "{} (2nd attempt — run `keel discover {}` for context)",
                                v.fix_hint.unwrap_or_default(),
                                v.hash
                            ));
                        }
                        BreakerAction::Downgrade => {
                            v.severity = "WARNING".to_string();
                            v.fix_hint = Some(format!(
                                "{} (auto-downgraded after 3 failures)",
                                v.fix_hint.unwrap_or_default(),
                            ));
                        }
                    }
                }
                v
            })
            .collect()
    }

    fn partition_violations(
        violations: Vec<Violation>,
        errors: &mut Vec<Violation>,
        warnings: &mut Vec<Violation>,
    ) {
        for v in violations {
            match v.severity.as_str() {
                "ERROR" => errors.push(v),
                _ => warnings.push(v),
            }
        }
    }

    fn collect_adjacency(
        &self,
        node_id: u64,
        depth: u32,
        upstream: &mut Vec<CallerInfo>,
        downstream: &mut Vec<CalleeInfo>,
    ) {
        if depth == 0 {
            return;
        }

        let edges = self.store.get_edges(node_id, EdgeDirection::Both);
        for edge in &edges {
            if edge.kind == EdgeKind::Calls {
                if edge.target_id == node_id {
                    // Incoming call = upstream (caller)
                    if let Some(caller) = self.store.get_node_by_id(edge.source_id) {
                        upstream.push(CallerInfo {
                            hash: caller.hash.clone(),
                            name: caller.name.clone(),
                            signature: caller.signature.clone(),
                            file: caller.file_path.clone(),
                            line: caller.line_start,
                            docstring: caller.docstring.clone(),
                            call_line: edge.line,
                        });
                    }
                } else if edge.source_id == node_id {
                    // Outgoing call = downstream (callee)
                    if let Some(callee) = self.store.get_node_by_id(edge.target_id) {
                        downstream.push(CalleeInfo {
                            hash: callee.hash.clone(),
                            name: callee.name.clone(),
                            signature: callee.signature.clone(),
                            file: callee.file_path.clone(),
                            line: callee.line_start,
                            docstring: callee.docstring.clone(),
                            call_line: edge.line,
                        });
                    }
                }
            }
        }
    }

    fn build_module_context(&self, module_id: u64) -> ModuleContext {
        let profile = self.store.get_module_profile(module_id);
        match profile {
            Some(p) => {
                let nodes = self.store.get_nodes_in_file(&p.path);
                let siblings: Vec<String> = nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::Function)
                    .map(|n| n.name.clone())
                    .collect();
                let endpoints: Vec<String> = nodes
                    .iter()
                    .flat_map(|n| n.external_endpoints.iter())
                    .map(|e| format!("{} {}", e.method, e.path))
                    .collect();
                ModuleContext {
                    module: p.path,
                    sibling_functions: siblings,
                    responsibility_keywords: p.responsibility_keywords,
                    function_count: p.function_count,
                    external_endpoints: endpoints,
                }
            }
            None => ModuleContext {
                module: String::new(),
                sibling_functions: vec![],
                responsibility_keywords: vec![],
                function_count: 0,
                external_endpoints: vec![],
            },
        }
    }
}

fn node_to_info(node: &keel_core::types::GraphNode) -> NodeInfo {
    NodeInfo {
        hash: node.hash.clone(),
        name: node.name.clone(),
        signature: node.signature.clone(),
        file: node.file_path.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        docstring: node.docstring.clone(),
        type_hints_present: node.type_hints_present,
        has_docstring: node.has_docstring,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::sqlite::SqliteGraphStore;

    #[test]
    fn test_engine_new() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let _engine = EnforcementEngine::new(Box::new(store));
    }

    #[test]
    fn test_compile_empty() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));
        let result = engine.compile(&[]);
        assert_eq!(result.status, "ok");
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_batch_mode() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));
        engine.batch_start();
        let result = engine.compile(&[]);
        assert_eq!(result.status, "ok");
        let batch_result = engine.batch_end();
        assert_eq!(batch_result.status, "ok");
    }

    #[test]
    fn test_where_hash_not_found() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let engine = EnforcementEngine::new(Box::new(store));
        assert!(engine.where_hash("nonexistent").is_none());
    }

    #[test]
    fn test_discover_not_found() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let engine = EnforcementEngine::new(Box::new(store));
        assert!(engine.discover("nonexistent", 1).is_none());
    }
}
