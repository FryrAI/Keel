use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_parsers::resolver::FileIndex;

use crate::batch::is_deferrable;
use crate::circuit_breaker::{BreakerAction, CircuitBreaker};
use crate::progressive::apply_progressive_adoption;
use crate::suppress::SuppressionManager;
use crate::types::{CompileInfo, CompileResult, Violation};
use crate::violations;
use crate::violations_economy;

/// Safety bound on the re-baseline disambiguation walk (see `compile`). Far
/// above any real file's same-named-definition count; it exists only so a
/// pathological store state cannot spin the loop forever.
const MAX_DISAMBIGUATION_ORDINAL: u32 = 64;

/// Core enforcement engine. Owns a GraphStore and orchestrates validation.
pub struct EnforcementEngine {
    pub(crate) store: Box<dyn GraphStore + Send>,
    pub(crate) circuit_breaker: CircuitBreaker,
    /// `Some(queue)` while this engine is in batch mode; the queue holds the
    /// violations deferred so far. Inactivity expiry lives entirely on
    /// [`PersistentBatch`](crate::batch::PersistentBatch) (checked by the CLI
    /// before it ever enters batch mode), so this carries no clock of its own.
    pub(crate) batch_state: Option<Vec<Violation>>,
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
        // Hashes claimed by re-baselined nodes earlier in THIS compile pass.
        // Mirrors the map pass's assigned-hash set (see `map_passes.rs`): the
        // store alone can't arbitrate, because these updates are batched and
        // not visible to `get_node` until the transaction runs.
        let mut assigned_hashes: HashSet<String> = HashSet::new();

        // Build batch file set: callers in the same compile batch are skipped
        // by E001/E004 since the user likely updated them in the same commit.
        let batch_file_set: HashSet<&str> = files.iter().map(|f| f.file_path.as_str()).collect();
        // Economy-check state shared across the batch: names referenced
        // anywhere in it (W005) and body hashes seen so far (W006).
        let referenced_names = violations_economy::batch_reference_names(files);
        let trait_bodies = violations_economy::batch_trait_context_bodies(files);
        let mut seen_bodies: HashMap<String, (String, String, u32)> = HashMap::new();

        for file in files {
            // Pre-fetch existing nodes once — used by E001, E004, and hash tracking
            let existing_nodes = self.store.get_nodes_in_file(&file.file_path);

            let mut file_violations = Vec::new();

            // E001: broken callers (uses cached nodes, batch-aware)
            file_violations.extend(violations::check_broken_callers_with_cache(
                file,
                &*self.store,
                &existing_nodes,
                &batch_file_set,
            ));
            // E002: missing type hints (gated by config)
            if self.enforce_config.type_hints {
                file_violations.extend(violations::check_missing_type_hints(file));
            }
            // E003: missing docstring (gated by config)
            if self.enforce_config.docstrings {
                file_violations.extend(violations::check_missing_docstring(file));
            }
            // E004: function removed (uses cached nodes, batch-aware)
            file_violations.extend(violations::check_removed_functions_with_cache(
                file,
                &*self.store,
                &existing_nodes,
                &batch_file_set,
            ));
            // E005: arity mismatch
            file_violations.extend(violations::check_arity_mismatch(file, &*self.store));
            // W001: placement (gated by config)
            if self.enforce_config.placement {
                file_violations.extend(violations::check_placement(file, &*self.store));
            }
            // W002: duplicate names
            file_violations.extend(violations::check_duplicate_names(file, &*self.store));
            // W005: dead code (gated by config)
            if self.enforce_config.dead_code {
                file_violations.extend(violations_economy::check_dead_code(
                    file,
                    &*self.store,
                    &existing_nodes,
                    &referenced_names,
                ));
            }
            // W006: duplicate implementations (gated by config)
            if self.enforce_config.duplication {
                file_violations.extend(violations_economy::check_duplicate_implementation(
                    file,
                    &*self.store,
                    &mut seen_bodies,
                    &trait_bodies,
                ));
            }
            // W007: oversized files (gated by config)
            if self.enforce_config.oversized_files {
                file_violations.extend(violations_economy::check_oversized_file(
                    file,
                    &existing_nodes,
                    self.enforce_config.max_file_lines,
                ));
            }

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

            // Progressive adoption: E002/E003 on untouched functions become
            // WARNING (gated by config). Runs before the circuit breaker so
            // pre-existing debt never accumulates failure counts.
            if self.enforce_config.progressive {
                file_violations =
                    apply_progressive_adoption(file_violations, file, &existing_nodes);
            }

            // Downgrade low-confidence violations (dynamic dispatch) to WARNING
            file_violations = Self::apply_dynamic_dispatch_threshold(file_violations);

            // Reset circuit breaker state for violations that were checked again
            // this compile but no longer fire — they were resolved, so their
            // counter should clear instead of only ever climbing toward
            // auto-downgrade regardless of whether anyone fixed anything.
            self.circuit_breaker.reconcile_file(
                &file.file_path,
                existing_nodes.iter().map(|n| n.hash.clone()),
                file.references.iter().filter_map(|r| r.resolved_to.clone()),
                &file_violations,
            );

            // Apply circuit breaker. The breaker counts fix ATTEMPTS, not
            // compiles, so it needs each offending node's current body/signature
            // fingerprint — keyed by (file, line) so a violation can look up the
            // freshly-parsed hash of the def it points at. A def absent from the
            // map (e.g. E004's removed function, E005's call site) falls back to
            // the violation's own hash, which is stable across passive recompiles
            // and so likewise never marches toward auto-downgrade on its own.
            // Each def's (plain, disambiguated) content hash, computed ONCE for
            // this file (the file path is constant across the loop) and shared
            // by the circuit-breaker fingerprint map here and the hash-
            // persistence loop below — both previously normalized and hashed
            // every def independently, so each def was hashed twice per compile.
            let def_hashes: Vec<(String, String)> = file
                .definitions
                .iter()
                .map(|d| crate::violations_util::definition_hashes(d, &file.file_path))
                .collect();

            let fingerprints =
                FileFingerprints::new(&file.file_path, &file.definitions, &def_hashes);
            file_violations = self.apply_circuit_breaker(file_violations, &fingerprints);

            // Apply suppressions
            file_violations = file_violations
                .into_iter()
                .map(|v| self.suppressions.apply(v))
                .collect();

            // Defer hash persistence for any definition still carrying a live
            // broken_caller ERROR (E001). Persisting its new hash would make the
            // *next* compile see "no change" and stop re-firing — so an agent
            // that ignores E001 once never sees it again (exit 0 would lie).
            // Keeping the stored (pre-edit) hash makes E001 re-fire on every
            // recompile of the callee's file until the break is actually
            // resolved: the caller is fixed and recompiled alongside it (batch
            // skip clears E001, hash then persists) or the callee is reverted.
            // Only ERROR-severity E001 defers; a breaker auto-downgrade (WARNING)
            // means "give up" and lets the hash persist so it stops re-firing.
            let deferred_e001: HashSet<String> = file_violations
                .iter()
                .filter(|v| v.code == "E001" && v.severity == "ERROR")
                .map(|v| v.hash.clone())
                .collect();

            // Track changed hashes and collect node updates for persistence,
            // reusing the per-def hashes computed above (no re-hashing).
            //
            // Each def must bind to a DISTINCT stored node. Matching on name
            // alone bound every same-named def in a file (e.g. the two
            // `fn is_available(&self) -> bool` impls in `python/ty.rs`) to the
            // same node: that node then received two conflicting hash updates
            // while its sibling was orphaned, and the second update tried to
            // claim the hash the sibling legitimately owns — aborting the whole
            // persist with a HashCollision. Prefer an exact line match, then
            // fall back to the first still-unclaimed node of that name.
            // Bind in TWO passes, not greedily per def. A single greedy pass
            // that prefers an exact (name, line) match but falls back to the
            // first unclaimed same-named node mis-pairs when two same-named defs
            // swap lines in one edit: the first def, whose exact match is now
            // the SECOND node, is offered the first node by the name-only
            // fallback and claims it — leaving each def bound to the other's
            // node. So let every exact (name, line) match claim its node first,
            // and only then bind the leftovers by name.
            let mut claimed_nodes: HashSet<u64> = HashSet::new();
            let mut bindings: Vec<Option<&keel_core::types::GraphNode>> =
                vec![None; file.definitions.len()];
            for (slot, def) in bindings.iter_mut().zip(&file.definitions) {
                if let Some(node) = existing_nodes.iter().find(|n| {
                    n.name == def.name
                        && n.line_start == def.line_start
                        && !claimed_nodes.contains(&n.id)
                }) {
                    claimed_nodes.insert(node.id);
                    *slot = Some(node);
                }
            }
            for (slot, def) in bindings.iter_mut().zip(&file.definitions) {
                if slot.is_some() {
                    continue;
                }
                if let Some(node) = existing_nodes
                    .iter()
                    .find(|n| n.name == def.name && !claimed_nodes.contains(&n.id))
                {
                    claimed_nodes.insert(node.id);
                    *slot = Some(node);
                }
            }

            for ((def, (new_hash, new_hash_disambiguated)), matched) in
                file.definitions.iter().zip(&def_hashes).zip(&bindings)
            {
                if let Some(node) = *matched {
                    if node.hash != *new_hash && node.hash != *new_hash_disambiguated {
                        if deferred_e001.contains(&node.hash) {
                            continue; // keep pre-edit hash so E001 re-fires
                        }
                        hashes_changed.push(node.hash.clone());
                        nodes_updated += 1;
                        // Identical definitions (e.g. `fn is_available(&self)
                        // -> bool { true }` on a trait decl and two impls)
                        // normalize to the same content hash. Salt on collision
                        // exactly as the map pass does, walking an ORDINAL until
                        // the candidate is free both in the store and among the
                        // hashes claimed earlier in this same pass. Ordinal 1 is
                        // the plain file-path salt, so the common two-def case
                        // still lands on the identity `keel map` would assign;
                        // 2+ only appear once a third same-named def exists.
                        // A single non-iterating fallback left every same-file
                        // duplicate sharing ONE disambiguated hash, and
                        // `update_nodes` then aborted the whole persist with a
                        // HashCollision.
                        let mut target_hash = new_hash.clone();
                        let mut ordinal = 0u32;
                        while self
                            .store
                            .get_node(&target_hash)
                            .is_some_and(|owner| owner.id != node.id)
                            || assigned_hashes.contains(&target_hash)
                        {
                            ordinal += 1;
                            target_hash = if ordinal == 1 {
                                new_hash_disambiguated.clone()
                            } else {
                                crate::violations_util::definition_hash_salted(
                                    def,
                                    &format!("{}#{}", file.file_path, ordinal),
                                )
                            };
                            if ordinal > MAX_DISAMBIGUATION_ORDINAL {
                                break;
                            }
                        }
                        assigned_hashes.insert(target_hash.clone());
                        // Persist updated hash; record the old one so
                        // "modified since last map" survives the sync (see
                        // progressive adoption — a full map re-baselines).
                        let mut updated_node = node.clone();
                        updated_node.previous_hashes.push(node.hash.clone());
                        updated_node.hash = target_hash;
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

            // Handle batch mode: defer non-structural (deferrable) violations;
            // structural ones still fire immediately. Inactivity expiry is
            // enforced by the CLI against the persisted batch *before* it enters
            // batch mode, so the in-process state has no clock to check here.
            if let Some(batch) = &mut self.batch_state {
                let (immediate, deferred): (Vec<_>, Vec<_>) = file_violations
                    .into_iter()
                    .partition(|v| !is_deferrable(&v.code));
                batch.extend(deferred);
                Self::partition_violations(immediate, &mut all_errors, &mut all_warnings);
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
        self.batch_state = Some(Vec::new());
    }

    /// Take the violations this engine deferred during a batch-mode compile,
    /// ending its in-process batch. The CLI persists these to the SQLite-backed
    /// [`PersistentBatch`](crate::batch::PersistentBatch) so they survive to the
    /// eventual `--batch-end` in a later process.
    pub fn drain_batch_deferred(&mut self) -> Vec<Violation> {
        self.batch_state.take().unwrap_or_default()
    }

    /// Build a `CompileResult` from a set of (deferred) violations — used to
    /// fire a persisted batch at `--batch-end` or on auto-expire.
    pub fn result_from_violations(violations: Vec<Violation>) -> CompileResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        Self::partition_violations(violations, &mut errors, &mut warnings);
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

    /// End batch mode: fire all deferred violations.
    pub fn batch_end(&mut self) -> CompileResult {
        let deferred = self.batch_state.take().unwrap_or_default();

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
    pub fn import_circuit_breaker(&mut self, state: &[keel_core::sqlite::CircuitBreakerEntry]) {
        self.circuit_breaker.import_state(state);
    }

    /// Export circuit breaker state for persistence.
    pub fn export_circuit_breaker(&self) -> Vec<keel_core::sqlite::CircuitBreakerEntry> {
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

    /// Remove every node (and its edges) belonging to a deleted file.
    ///
    /// The file watcher calls this on `Remove` events so deleted files stop
    /// accreting in the shared graph between full `keel map` runs. Works
    /// entirely through the frozen [`GraphStore`] trait: collect the file's
    /// nodes, drop every edge touching them, then drop the nodes. Returns the
    /// number of nodes removed.
    pub fn prune_file(&mut self, file_path: &str) -> Result<usize, keel_core::types::GraphError> {
        use keel_core::types::{EdgeChange, EdgeDirection, NodeChange};

        let nodes = self.store.get_nodes_in_file(file_path);
        if nodes.is_empty() {
            return Ok(0);
        }

        // Drop edges first so no dangling source/target ids survive.
        let mut seen_edges = HashSet::new();
        let mut edge_changes = Vec::new();
        for node in &nodes {
            for edge in self.store.get_edges(node.id, EdgeDirection::Both) {
                if seen_edges.insert(edge.id) {
                    edge_changes.push(EdgeChange::Remove(edge.id));
                }
            }
        }
        if !edge_changes.is_empty() {
            self.store.update_edges(edge_changes)?;
        }

        let count = nodes.len();
        let node_changes = nodes
            .into_iter()
            .map(|n| NodeChange::Remove(n.id))
            .collect();
        self.store.update_nodes(node_changes)?;
        Ok(count)
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

    /// Apply circuit breaker escalation (fix hint, wider context, or downgrade)
    /// to ERROR violations. `fingerprints` supplies the freshly-parsed
    /// body/signature fingerprint the offending code has right now, so the
    /// breaker can tell a real (still-failing) fix attempt from a passive
    /// recompile of untouched code.
    pub(crate) fn apply_circuit_breaker(
        &mut self,
        violations: Vec<Violation>,
        fingerprints: &FileFingerprints,
    ) -> Vec<Violation> {
        violations
            .into_iter()
            .map(|mut v| {
                if v.severity == "ERROR" {
                    let body_hash = fingerprints.fingerprint_for(&v);
                    let action = self
                        .circuit_breaker
                        .record_failure(&v.code, &v.hash, body_hash, &v.file);
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

/// The fingerprints a compiled file offers the circuit breaker.
///
/// The breaker counts fix ATTEMPTS, not compiles: it advances only when the
/// code a violation blames actually changed since the last failure. Picking the
/// right fingerprint for a violation is therefore the whole ballgame — too
/// narrow and the counter freezes at one strike forever, too wide and unrelated
/// edits march a live ERROR toward its auto-downgrade to WARNING.
pub(crate) struct FileFingerprints {
    file_path: String,
    /// `(line_start, line_end, content hash)` per definition, in file order.
    defs: Vec<(u32, u32, String)>,
    /// The file's sorted definition NAMES, joined. Changes only when a def is
    /// added, removed, or renamed — never when a body is edited.
    name_set: String,
}

impl FileFingerprints {
    /// Build the fingerprints for one file from its parsed definitions and the
    /// `(plain, disambiguated)` hashes already computed for them.
    pub(crate) fn new(
        file_path: &str,
        definitions: &[keel_parsers::resolver::Definition],
        def_hashes: &[(String, String)],
    ) -> Self {
        let defs = definitions
            .iter()
            .zip(def_hashes)
            .map(|(d, (plain, _))| (d.line_start, d.line_end, plain.clone()))
            .collect();
        let mut names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        names.sort_unstable();
        Self {
            file_path: file_path.to_string(),
            defs,
            name_set: names.join("|"),
        }
    }

    /// The fingerprint to charge this violation's breaker attempt against.
    ///
    /// The chain, narrowest first:
    /// 1. A def starting exactly at `(file, line)` — the violation blames that
    ///    def, so its content hash is the attempt fingerprint.
    /// 2. A def whose line RANGE contains `line` — E005's shape: the violation
    ///    points at a call site living inside some def, and a fix attempt edits
    ///    that def.
    /// 3. The file's def NAME SET — E004's shape: the blamed node was removed,
    ///    so no def can contain its old line. Only a structural edit (restoring,
    ///    renaming, or deleting a def) counts as an attempt. Using the file's
    ///    full content fingerprint here was wrong: three unrelated body edits
    ///    elsewhere in the file auto-downgraded a live E004 to WARNING, flipping
    ///    exit 1 to 0 and shipping the broken callers.
    /// 4. The violation's own hash, for a file with no definitions at all.
    fn fingerprint_for<'a>(&'a self, v: &'a Violation) -> &'a str {
        if v.file == self.file_path {
            if let Some((_, _, h)) = self.defs.iter().find(|(start, _, _)| *start == v.line) {
                return h;
            }
            if let Some((_, _, h)) = self
                .defs
                .iter()
                .find(|(start, end, _)| v.line >= *start && v.line <= *end)
            {
                return h;
            }
        }
        if !self.name_set.is_empty() {
            return &self.name_set;
        }
        &v.hash
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
