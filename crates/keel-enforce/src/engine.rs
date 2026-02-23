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
    pub(crate) enforce_config: keel_core::config::EnforceConfig,
}

impl EnforcementEngine {
    /// Create an enforcement engine with default configuration.
    pub fn new(store: Box<dyn GraphStore + Send>) -> Self {
        Self {
            store,
            circuit_breaker: CircuitBreaker::new(),
            batch_state: None,
            suppressions: SuppressionManager::new(),
            enforce_config: keel_core::config::EnforceConfig::default(),
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
            enforce_config: config.enforce.clone(),
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
            // Pre-fetch existing nodes once — used by E001, E004, and hash tracking
            let existing_nodes = self.store.get_nodes_in_file(&file.file_path);

            let mut file_violations = Vec::new();

            // E001: broken callers (uses cached nodes)
            file_violations.extend(violations::check_broken_callers_with_cache(
                file,
                &*self.store,
                &existing_nodes,
            ));
            // E002: missing type hints (gated by config)
            if self.enforce_config.type_hints {
                file_violations.extend(violations::check_missing_type_hints(file));
            }
            // E003: missing docstring (gated by config)
            if self.enforce_config.docstrings {
                file_violations.extend(violations::check_missing_docstring(file));
            }
            // E004: function removed (uses cached nodes)
            file_violations.extend(violations::check_removed_functions_with_cache(
                file,
                &*self.store,
                &existing_nodes,
            ));
            // E005: arity mismatch
            file_violations.extend(violations::check_arity_mismatch(file, &*self.store));
            // W001: placement (gated by config)
            if self.enforce_config.placement {
                file_violations.extend(violations::check_placement(file, &*self.store));
            }
            // W002: duplicate names
            file_violations.extend(violations::check_duplicate_names(file, &*self.store));

            // Fixup: use graph-stored hashes so `keel explain <hash>` works.
            // Some checks (E002, E003, W001, W002) compute hashes freshly, which
            // may differ from the graph when map used disambiguation for collisions.
            for v in &mut file_violations {
                if let Some(node) = existing_nodes
                    .iter()
                    .find(|n| n.file_path == v.file && n.line_start == v.line)
                {
                    v.hash = node.hash.clone();
                }
            }

            // Downgrade low-confidence violations (dynamic dispatch) to WARNING
            file_violations = Self::apply_dynamic_dispatch_threshold(file_violations);

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
                // Also check disambiguated hash (map may have used it for collisions)
                let new_hash_disambiguated = keel_core::hash::compute_hash_disambiguated(
                    &def.signature,
                    &def.body_text,
                    def.docstring.as_deref().unwrap_or(""),
                    &file.file_path,
                );
                if let Some(node) = existing_nodes.iter().find(|n| n.name == def.name) {
                    if node.hash != new_hash && node.hash != new_hash_disambiguated {
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
                    let deferred = self.batch_state.take().unwrap().drain();
                    Self::partition_violations(deferred, &mut all_errors, &mut all_warnings);
                    // Non-deferred violations from this file
                    Self::partition_violations(file_violations, &mut all_errors, &mut all_warnings);
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
            version: env!("CARGO_PKG_VERSION").to_string(),
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
            version: env!("CARGO_PKG_VERSION").to_string(),
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

    /// Import circuit breaker state (e.g. loaded from SQLite).
    pub fn import_circuit_breaker(&mut self, state: &[(String, String, u32, bool)]) {
        self.circuit_breaker.import_state(state);
    }

    /// Export circuit breaker state for persistence.
    pub fn export_circuit_breaker(&self) -> Vec<(String, String, u32, bool)> {
        self.circuit_breaker.export_state()
    }

    /// Get circuit breaker failure count for a specific error+hash+file combination.
    pub fn circuit_breaker_failures(&self, error_code: &str, hash: &str, file_path: &str) -> u32 {
        self.circuit_breaker
            .failure_count(error_code, hash, file_path)
    }

    /// Suppress a specific error/warning code.
    pub fn suppress(&mut self, code: &str) {
        self.suppressions.suppress(code);
    }

    // -- Private helpers --

    /// Dynamic dispatch threshold: violations with confidence below 0.7
    /// are downgraded from ERROR to WARNING (trait dispatch, interface methods).
    const DYNAMIC_DISPATCH_THRESHOLD: f64 = 0.7;

    /// Downgrade low-confidence violations (below 0.7) from ERROR to WARNING.
    pub fn apply_dynamic_dispatch_threshold(violations: Vec<Violation>) -> Vec<Violation> {
        violations
            .into_iter()
            .map(|mut v| {
                if v.severity == "ERROR" && v.confidence < Self::DYNAMIC_DISPATCH_THRESHOLD {
                    v.severity = "WARNING".to_string();
                    v.fix_hint = Some(format!(
                        "{} (low confidence {:.0}% — likely dynamic dispatch)",
                        v.fix_hint.unwrap_or_default(),
                        v.confidence * 100.0,
                    ));
                }
                v
            })
            .collect()
    }

    /// Apply circuit breaker escalation (fix hint, wider context, or downgrade) to ERROR violations.
    pub(crate) fn apply_circuit_breaker(&mut self, violations: Vec<Violation>) -> Vec<Violation> {
        violations
            .into_iter()
            .map(|mut v| {
                if v.severity == "ERROR" {
                    let action = self
                        .circuit_breaker
                        .record_failure(&v.code, &v.hash, &v.file);
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

    /// Split violations into errors and warnings based on severity.
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

/// Convert a `GraphNode` into a `NodeInfo` struct for serialized output.
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
