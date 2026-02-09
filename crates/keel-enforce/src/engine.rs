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
    store: Box<dyn GraphStore + Send>,
    circuit_breaker: CircuitBreaker,
    batch_state: Option<BatchState>,
    suppressions: SuppressionManager,
}

impl EnforcementEngine {
    pub fn new(store: Box<dyn GraphStore + Send>) -> Self {
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
    use keel_core::types::{EdgeChange, GraphEdge, GraphNode};
    use keel_parsers::resolver::Definition;

    fn make_node(id: u64, hash: &str, name: &str, sig: &str, file: &str) -> GraphNode {
        GraphNode {
            id,
            hash: hash.to_string(),
            kind: NodeKind::Function,
            name: name.to_string(),
            signature: sig.to_string(),
            file_path: file.to_string(),
            line_start: 10,
            line_end: 20,
            docstring: Some(format!("Doc for {}", name)),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        }
    }

    fn make_call_edge(id: u64, src: u64, tgt: u64, file: &str) -> GraphEdge {
        GraphEdge {
            id,
            source_id: src,
            target_id: tgt,
            kind: EdgeKind::Calls,
            file_path: file.to_string(),
            line: 15,
        }
    }

    fn make_definition(name: &str, sig: &str, body: &str, file: &str) -> Definition {
        Definition {
            name: name.to_string(),
            kind: NodeKind::Function,
            signature: sig.to_string(),
            file_path: file.to_string(),
            line_start: 10,
            line_end: 20,
            docstring: Some(format!("Doc for {}", name)),
            is_public: true,
            type_hints_present: true,
            body_text: body.to_string(),
        }
    }

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

    // --- Integration tests with real graph data ---

    #[test]
    fn test_where_hash_found() {
        let store = SqliteGraphStore::in_memory().unwrap();
        store
            .insert_node(&make_node(1, "abc12345678", "foo", "fn foo()", "src/lib.rs"))
            .unwrap();
        let engine = EnforcementEngine::new(Box::new(store));

        let result = engine.where_hash("abc12345678");
        assert!(result.is_some());
        let (file, line) = result.unwrap();
        assert_eq!(file, "src/lib.rs");
        assert_eq!(line, 10);
    }

    #[test]
    fn test_discover_with_callers_and_callees() {
        let mut store = SqliteGraphStore::in_memory().unwrap();

        // Create nodes: caller -> target -> callee
        let caller = make_node(1, "cal11111111", "caller_fn", "fn caller_fn()", "src/a.rs");
        let target = make_node(2, "tgt11111111", "target_fn", "fn target_fn(x: i32)", "src/b.rs");
        let callee = make_node(3, "cle11111111", "callee_fn", "fn callee_fn()", "src/c.rs");

        store.insert_node(&caller).unwrap();
        store.insert_node(&target).unwrap();
        store.insert_node(&callee).unwrap();

        // caller calls target, target calls callee
        store
            .update_edges(vec![
                EdgeChange::Add(make_call_edge(1, 1, 2, "src/a.rs")),
                EdgeChange::Add(make_call_edge(2, 2, 3, "src/b.rs")),
            ])
            .unwrap();

        let engine = EnforcementEngine::new(Box::new(store));
        let result = engine.discover("tgt11111111", 1).unwrap();

        assert_eq!(result.target.name, "target_fn");
        assert_eq!(result.target.hash, "tgt11111111");
        assert_eq!(result.upstream.len(), 1);
        assert_eq!(result.upstream[0].name, "caller_fn");
        assert_eq!(result.downstream.len(), 1);
        assert_eq!(result.downstream[0].name, "callee_fn");
    }

    #[test]
    fn test_explain_with_edges() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let node = make_node(1, "abc12345678", "foo", "fn foo()", "src/lib.rs");
        let callee = make_node(2, "def11111111", "bar", "fn bar()", "src/bar.rs");
        store.insert_node(&node).unwrap();
        store.insert_node(&callee).unwrap();

        store
            .update_edges(vec![EdgeChange::Add(make_call_edge(1, 1, 2, "src/lib.rs"))])
            .unwrap();

        let engine = EnforcementEngine::new(Box::new(store));
        let result = engine.explain("E001", "abc12345678").unwrap();

        assert_eq!(result.error_code, "E001");
        assert_eq!(result.hash, "abc12345678");
        assert_eq!(result.confidence, 0.92);
        assert!(!result.resolution_chain.is_empty());
        assert_eq!(result.resolution_chain[0].kind, "call");
    }

    #[test]
    fn test_e001_broken_caller_fires() {
        let store = SqliteGraphStore::in_memory().unwrap();
        // Store a function with old hash
        let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
        let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
        node.docstring = Some("Doc for foo".to_string());
        store.insert_node(&node).unwrap();

        // Store a caller
        let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
        store.insert_node(&caller).unwrap();

        // Edge: caller -> foo
        let mut store_mut = store;
        store_mut
            .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
            .unwrap();

        let mut engine = EnforcementEngine::new(Box::new(store_mut));

        // Compile with a changed signature for foo
        let file = FileIndex {
            file_path: "src/lib.rs".to_string(),
            content_hash: 0,
            definitions: vec![make_definition(
                "foo",
                "fn foo(x: i32, y: i32)",
                "{ x + y }",
                "src/lib.rs",
            )],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        assert!(!result.errors.is_empty());
        let e001 = result.errors.iter().find(|v| v.code == "E001");
        assert!(e001.is_some(), "E001 broken_caller should fire");
        let v = e001.unwrap();
        assert_eq!(v.category, "broken_caller");
        assert_eq!(v.affected.len(), 1);
        assert_eq!(v.affected[0].name, "bar");
    }

    #[test]
    fn test_e002_missing_type_hints() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        let mut def = make_definition("process", "def process(x)", "pass", "app.py");
        def.type_hints_present = false;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        let e002 = result.errors.iter().find(|v| v.code == "E002");
        assert!(e002.is_some(), "E002 missing_type_hints should fire");
        assert!(e002.unwrap().message.contains("process"));
    }

    #[test]
    fn test_e003_missing_docstring() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        let mut def = make_definition("handle", "fn handle()", "{}", "src/h.rs");
        def.docstring = None;

        let file = FileIndex {
            file_path: "src/h.rs".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        let e003 = result.errors.iter().find(|v| v.code == "E003");
        assert!(e003.is_some(), "E003 missing_docstring should fire");
        assert!(e003.unwrap().message.contains("handle"));
    }

    #[test]
    fn test_e004_function_removed() {
        let store = SqliteGraphStore::in_memory().unwrap();
        // Store a function that will be "removed"
        let node = make_node(1, "old11111111", "deprecated_fn", "fn deprecated_fn()", "src/lib.rs");
        store.insert_node(&node).unwrap();

        // Store a caller
        let caller = make_node(2, "cal11111111", "consumer", "fn consumer()", "src/main.rs");
        store.insert_node(&caller).unwrap();

        let mut store_mut = store;
        store_mut
            .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/main.rs"))])
            .unwrap();

        let mut engine = EnforcementEngine::new(Box::new(store_mut));

        // Compile with an empty definitions list (function removed)
        let file = FileIndex {
            file_path: "src/lib.rs".to_string(),
            content_hash: 0,
            definitions: vec![],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        let e004 = result.errors.iter().find(|v| v.code == "E004");
        assert!(e004.is_some(), "E004 function_removed should fire");
        let v = e004.unwrap();
        assert!(v.message.contains("deprecated_fn"));
        assert_eq!(v.affected.len(), 1);
        assert_eq!(v.affected[0].name, "consumer");
    }

    #[test]
    fn test_clean_compile_no_violations() {
        let store = SqliteGraphStore::in_memory().unwrap();
        // Compute hash matching the definition exactly
        let hash = keel_core::hash::compute_hash("fn clean(x: i32) -> bool", "{ x > 0 }", "Doc for clean");
        let mut node = make_node(1, &hash, "clean", "fn clean(x: i32) -> bool", "src/lib.rs");
        node.docstring = Some("Doc for clean".to_string());
        store.insert_node(&node).unwrap();

        let mut engine = EnforcementEngine::new(Box::new(store));

        let file = FileIndex {
            file_path: "src/lib.rs".to_string(),
            content_hash: 0,
            definitions: vec![make_definition(
                "clean",
                "fn clean(x: i32) -> bool",
                "{ x > 0 }",
                "src/lib.rs",
            )],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "ok");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_batch_defers_e002_e003() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        engine.batch_start();

        let mut def = make_definition("process", "def process(x)", "pass", "app.py");
        def.type_hints_present = false;
        def.docstring = None;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        // During batch mode, E002/E003 should be deferred
        let result = engine.compile(&[file]);
        assert_eq!(result.status, "ok", "Deferred violations should not appear yet");
        assert!(result.errors.is_empty());

        // batch_end should fire the deferred violations
        let batch_result = engine.batch_end();
        assert!(!batch_result.errors.is_empty(), "Deferred violations should fire on batch_end");
        let codes: Vec<&str> = batch_result.errors.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"E002") || codes.contains(&"E003"));
    }

    #[test]
    fn test_suppression() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        engine.suppress("E002");

        let mut def = make_definition("process", "def process(x)", "pass", "app.py");
        def.type_hints_present = false;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        // E002 should be suppressed to S001/INFO which goes to warnings, not errors
        let e002_errors = result.errors.iter().filter(|v| v.code == "E002").count();
        assert_eq!(e002_errors, 0, "E002 should be suppressed");

        // Should appear as S001 in warnings
        let s001 = result.warnings.iter().find(|v| v.code == "S001");
        assert!(s001.is_some(), "Suppressed E002 should become S001");
        assert!(s001.unwrap().suppressed);
    }

    // --- Edge case tests ---

    #[test]
    fn test_e001_and_e002_combined_on_same_file() {
        let store = SqliteGraphStore::in_memory().unwrap();
        // Store a function with old hash (will trigger E001 when signature changes)
        let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
        let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.py");
        node.docstring = Some("Doc for foo".to_string());
        store.insert_node(&node).unwrap();

        // Store a caller
        let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.py");
        store.insert_node(&caller).unwrap();

        let mut store_mut = store;
        store_mut
            .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.py"))])
            .unwrap();

        let mut engine = EnforcementEngine::new(Box::new(store_mut));

        // File with changed foo (triggers E001) AND a new public function without type hints (E002)
        let mut changed_foo = make_definition("foo", "fn foo(x: i32, y: i32)", "{ x + y }", "src/lib.py");
        changed_foo.type_hints_present = true;

        let mut no_hints = make_definition("process", "def process(x)", "pass", "src/lib.py");
        no_hints.type_hints_present = false;

        let file = FileIndex {
            file_path: "src/lib.py".to_string(),
            content_hash: 0,
            definitions: vec![changed_foo, no_hints],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");

        let e001 = result.errors.iter().filter(|v| v.code == "E001").count();
        let e002 = result.errors.iter().filter(|v| v.code == "E002").count();
        assert!(e001 > 0, "E001 broken_caller should fire");
        assert!(e002 > 0, "E002 missing_type_hints should fire");
    }

    #[test]
    fn test_suppression_prevents_circuit_breaker_escalation() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        // Suppress E002 before compiling
        engine.suppress("E002");

        let mut def = make_definition("process", "def process(x)", "pass", "app.py");
        def.type_hints_present = false;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        // Compile 3 times — suppressed violations should become S001/INFO
        for _ in 0..3 {
            let result = engine.compile(&[file.clone()]);
            let e002_errors = result.errors.iter().filter(|v| v.code == "E002").count();
            assert_eq!(e002_errors, 0, "E002 should be suppressed in every iteration");

            let s001 = result.warnings.iter().filter(|v| v.code == "S001").count();
            assert!(s001 > 0, "Suppressed E002 should appear as S001");
        }
    }

    #[test]
    fn test_batch_expired_flushes_deferred() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        // Set batch state to already expired
        engine.batch_state = Some(crate::batch::BatchState::new_expired());

        let mut def = make_definition("process", "def process(x)", "pass", "app.py");
        def.type_hints_present = false;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        // Compile with expired batch — should flush and include E002 immediately
        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        let e002 = result.errors.iter().filter(|v| v.code == "E002").count();
        assert!(e002 > 0, "E002 should fire immediately when batch is expired");
        // Batch state should be consumed
        assert!(engine.batch_state.is_none(), "Expired batch should be consumed");
    }

    #[test]
    fn test_e003_and_e002_both_fire_for_same_function() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let mut engine = EnforcementEngine::new(Box::new(store));

        let mut def = make_definition("handler", "def handler(x)", "pass", "app.py");
        def.type_hints_present = false;
        def.docstring = None;

        let file = FileIndex {
            file_path: "app.py".to_string(),
            content_hash: 0,
            definitions: vec![def],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        let result = engine.compile(&[file]);
        assert_eq!(result.status, "error");
        let codes: Vec<&str> = result.errors.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"E002"), "E002 should fire for missing type hints");
        assert!(codes.contains(&"E003"), "E003 should fire for missing docstring");
    }

    #[test]
    fn test_circuit_breaker_downgrade() {
        let store = SqliteGraphStore::in_memory().unwrap();
        let old_hash = keel_core::hash::compute_hash("fn foo()", "{ 1 }", "Doc for foo");
        let mut node = make_node(1, &old_hash, "foo", "fn foo()", "src/lib.rs");
        node.docstring = Some("Doc for foo".to_string());
        store.insert_node(&node).unwrap();

        let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
        store.insert_node(&caller).unwrap();

        let mut store_mut = store;
        store_mut
            .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
            .unwrap();

        let mut engine = EnforcementEngine::new(Box::new(store_mut));

        let file = FileIndex {
            file_path: "src/lib.rs".to_string(),
            content_hash: 0,
            definitions: vec![make_definition("foo", "fn foo(x: i32)", "{ x }", "src/lib.rs")],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
            parse_duration_us: 0,
        };

        // First compile: E001 fires as ERROR (attempt 1 = FixHint)
        let r1 = engine.compile(&[file.clone()]);
        assert!(r1.errors.iter().any(|v| v.code == "E001" && v.severity == "ERROR"));

        // Second compile: still ERROR (attempt 2 = WiderContext)
        let r2 = engine.compile(&[file.clone()]);
        assert!(r2.errors.iter().any(|v| v.code == "E001" && v.severity == "ERROR"));

        // Third compile: should be downgraded to WARNING
        let r3 = engine.compile(&[file.clone()]);
        let e001_errors = r3.errors.iter().filter(|v| v.code == "E001").count();
        let e001_warnings = r3.warnings.iter().filter(|v| v.code == "E001").count();
        assert_eq!(e001_errors, 0, "E001 should be downgraded after 3 failures");
        assert!(e001_warnings > 0, "E001 should appear as WARNING after downgrade");
    }
}
