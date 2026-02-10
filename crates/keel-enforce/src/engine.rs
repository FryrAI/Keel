use keel_core::store::GraphStore;
use keel_parsers::resolver::FileIndex;

use crate::batch::BatchState;
use crate::circuit_breaker::{BreakerAction, CircuitBreaker};
use crate::suppress::SuppressionManager;
use crate::types::{CompileInfo, CompileResult, Violation};
use crate::violations;

/// Core enforcement engine. Owns a GraphStore and orchestrates validation.
pub struct EnforcementEngine {
    pub(crate) store: Box<dyn GraphStore + Send>,
    pub(crate) circuit_breaker: CircuitBreaker,
    pub(crate) batch_state: Option<BatchState>,
    pub(crate) suppressions: SuppressionManager,
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

    /// Create an engine configured from a `KeelConfig`.
    pub fn with_config(
        store: Box<dyn GraphStore + Send>,
        config: &keel_core::config::KeelConfig,
    ) -> Self {
        Self {
            store,
            circuit_breaker: CircuitBreaker::with_max_failures(config.circuit_breaker.max_failures),
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
        let file_paths: Vec<String> = files.iter().map(|f| f.file_path.clone()).collect();
        let mut node_changes: Vec<keel_core::types::NodeChange> = Vec::new();

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

            // Track changed hashes and collect node updates for persistence
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
                        // Persist updated hash
                        let mut updated_node = node.clone();
                        updated_node.hash = new_hash;
                        updated_node.signature = def.signature.clone();
                        updated_node.docstring = def.docstring.clone();
                        updated_node.has_docstring = def.docstring.is_some();
                        updated_node.type_hints_present = def.type_hints_present;
                        updated_node.is_public = def.is_public;
                        updated_node.line_start = def.line_start;
                        updated_node.line_end = def.line_end;
                        node_changes.push(keel_core::types::NodeChange::Update(updated_node));
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

        // Persist node changes to the graph store
        if !node_changes.is_empty() {
            if let Err(e) = self.store.update_nodes(node_changes) {
                eprintln!("keel compile: failed to persist node updates: {}", e);
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
                edges_updated: 0,
                hashes_changed,
            },
        }
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

    pub(crate) fn apply_circuit_breaker(&mut self, violations: Vec<Violation>) -> Vec<Violation> {
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

    pub(crate) fn partition_violations(
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
}

/// Convert a GraphNode to a NodeInfo for output.
pub(crate) fn node_to_info(node: &keel_core::types::GraphNode) -> crate::types::NodeInfo {
    crate::types::NodeInfo {
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
#[path = "engine_tests.rs"]
mod tests;
