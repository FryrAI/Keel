//! Economy checks: keep the codebase lean.
//!
//! - W005 `dead_code` — private function with no callers or value usages in
//!   the graph.
//! - W006 `duplicate_implementation` — body identical (whitespace-normalized,
//!   "Type-1") or structurally identical after identifier/literal
//!   normalization ("Type-2", lower confidence) to a function in another file.
//! - W007 `oversized_file` — file exceeds the configured line budget and grew.
//!
//! All three are WARNING severity, deferrable in batch mode, and gated by
//! `enforce.{dead_code, duplication, oversized_files}` in keel.json.

use std::collections::{HashMap, HashSet};

use keel_core::config::EnforceConfig;
use keel_core::hash_t2;
use keel_core::store::GraphStore;
use keel_core::types::{BodyIndexEntry, EdgeDirection, EdgeKind, GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex};

use crate::types::Violation;
use crate::violations_util::{bind_to_node, is_bench_file, is_stub_file, is_test_file};

/// Bodies shorter than this (whitespace-normalized) are too trivial to call
/// duplicates — one-line getters and delegations legitimately repeat.
const MIN_DUPLICATE_BODY_LEN: usize = 60;

// Anything this check wants to match must have been indexed at map time,
// which gates on the (lower) MIN_INDEXED_BODY_LEN.
const _: () = assert!(MIN_DUPLICATE_BODY_LEN >= keel_core::hash::MIN_INDEXED_BODY_LEN);

/// Floor for the Type-2 token stream, higher than its Type-1 counterpart —
/// see [`keel_core::hash_t2::MIN_T2_NORMALIZED_LEN`] for why.
const MIN_T2_NORMALIZED_LEN: usize = hash_t2::MIN_T2_NORMALIZED_LEN;
const _: () = assert!(MIN_T2_NORMALIZED_LEN > MIN_DUPLICATE_BODY_LEN);

/// Confidence for a Type-1 duplicate — the same body, modulo whitespace.
const DUPLICATE_T1_CONFIDENCE: f64 = 0.85;
/// Confidence for a Type-2-only near-clone: the same structure with renamed
/// identifiers and different literals. A heuristic about shape, not evidence
/// of a literal copy, so it sits below the "attempt one fix" threshold.
const DUPLICATE_T2_CONFIDENCE: f64 = 0.6;

/// Names that are entrypoints or conventionally uncalled in every language —
/// never dead.
///
/// Reached through `is_exempt_dead_name`, which `crate::quality`'s
/// `dead_private_fns` metric shares: a trend line that includes `main` is
/// measuring keel's exemption list, not the codebase.
const ENTRYPOINT_NAMES: &[&str] = &["main", "new", "default", "drop", "fmt"];

/// True when a function's NAME alone exempts it from dead-code analysis:
/// `_`-prefixed (deliberately unused), `bench_*` (criterion benches outside
/// `benches/`, which no test-context marking covers), a `.`-qualified name (a
/// method reached through a receiver), or an `ENTRYPOINT_NAMES` entry.
///
/// One definition, because the stored-graph twin of this check —
/// `crate::quality`'s `dead_private_fns` metric, which has only a name and a
/// path to go on — must not drift from the compile-time rule. Everything else
/// W005 exempts (decorators, trait context, `keel:keep`, test context) needs a
/// fresh parse to see and stays at the call site.
pub(crate) fn is_exempt_dead_name(name: &str) -> bool {
    name.starts_with('_')
        || name.starts_with("bench_")
        || name.contains('.')
        || ENTRYPOINT_NAMES.contains(&name)
}

/// True when a definition is an auto-invoked entrypoint and therefore never dead.
///
/// `is_exempt_dead_name` holds only names universal across languages; anything
/// language-specific (Go's `init`/`main`/`TestMain`) is carried by the parser's
/// per-language `is_auto_invoked` flag, so this check no longer re-derives the
/// language from the file path or accretes a match arm per runtime convention.
fn is_entrypoint(def: &Definition) -> bool {
    is_exempt_dead_name(&def.name) || def.is_auto_invoked
}

/// Collect every symbol name referenced anywhere in the compile batch,
/// including the final segment of qualified references (`obj.method` → both
/// `obj.method` and `method`). Used to keep W005 from flagging a function
/// whose caller is being written in the same compile.
pub fn batch_reference_names(files: &[FileIndex]) -> HashSet<String> {
    let mut names = HashSet::new();
    for file in files {
        for r in &file.references {
            names.insert(r.name.clone());
            // Final segment of a qualified reference, for both separators:
            // `obj.method` and `module::function` (the latter shows up in
            // attribute-string references like `#[serde(with = "a::b")]`).
            if let Some(last) = r.name.rsplit('.').next() {
                names.insert(last.to_string());
            }
            if let Some(last) = r.name.rsplit("::").next() {
                names.insert(last.to_string());
            }
        }
    }
    names
}

/// The two fingerprints of one function body, `None` per tier when the
/// normalized form is under that tier's floor.
struct BodyFingerprints {
    /// Whitespace-normalized body hash — the Type-1 identity.
    t1: Option<String>,
    /// Identifier/literal-normalized token-stream hash — the Type-2 identity.
    t2: Option<String>,
}

impl BodyFingerprints {
    /// Fingerprint `body` in both tiers.
    ///
    /// The only place a body is normalized during a compile. Both tiers are
    /// computed together because a body that fires Type-1 is still the Type-2
    /// twin a later file in the batch may be looking for, so there is no
    /// branch on which one is skippable.
    fn of(body: &str, lang: &str) -> Self {
        let normalized = keel_core::hash::normalize_body(body);
        let t2_normalized = hash_t2::normalize_body_t2(body, lang);
        BodyFingerprints {
            t1: (normalized.len() >= MIN_DUPLICATE_BODY_LEN)
                .then(|| keel_core::hash::hash_normalized_body(&normalized)),
            t2: (t2_normalized.len() >= MIN_T2_NORMALIZED_LEN)
                .then(|| keel_core::hash::hash_string(&t2_normalized)),
        }
    }
}

/// What W006 precomputes for one compile batch: every function body's
/// fingerprints, and which of those bodies sit on a trait contract surface.
///
/// The trait maps answer "is the function my body matches ALSO a trait
/// method?" — the graph persists no trait-context provenance, so the answer
/// has to come from the freshly parsed batch. They are built from the same
/// pass that fingerprints the bodies: keeping them separate meant every trait
/// method was normalized twice per compile, with the two normalizations held
/// in step by hand.
#[derive(Default)]
pub struct DuplicateIndex {
    /// `file_path` → `(line_start, name)` → fingerprints. Neither key is
    /// unique alone: a name repeats within a file (free fn + method), and two
    /// definitions can start on one physical line in minified sources — a
    /// line-only key would hand the first def the last def's fingerprints.
    per_def: HashMap<String, HashMap<(u32, String), BodyFingerprints>>,
    /// Type-1 fingerprint → the files defining it on a trait surface.
    trait_t1: HashMap<String, HashSet<String>>,
    /// Type-2 fingerprint → the same, keyed on the other tier's fingerprint.
    trait_t2: HashMap<String, HashSet<String>>,
}

impl DuplicateIndex {
    /// Fingerprint every function body in `files`.
    pub fn new(files: &[FileIndex]) -> Self {
        let mut index = DuplicateIndex::default();
        for file in files {
            index.add_file(file);
        }
        index
    }

    /// Fingerprint one file's function bodies into the index. Idempotent — a
    /// second pass over the same file recomputes identical entries.
    fn add_file(&mut self, file: &FileIndex) {
        // Must be the same language string `keel map` indexed under, or no
        // stored Type-2 fingerprint ever matches. `FileIndex::language` is
        // that one derivation.
        let lang = file.language();
        for def in &file.definitions {
            if def.kind != NodeKind::Function {
                continue;
            }
            let fingerprints = BodyFingerprints::of(&def.body_text, lang);
            if def.in_trait_context {
                for (fingerprint, map) in [
                    (&fingerprints.t1, &mut self.trait_t1),
                    (&fingerprints.t2, &mut self.trait_t2),
                ] {
                    if let Some(f) = fingerprint {
                        map.entry(f.clone())
                            .or_default()
                            .insert(file.file_path.clone());
                    }
                }
            }
            self.per_def
                .entry(file.file_path.clone())
                .or_default()
                .insert((def.line_start, def.name.clone()), fingerprints);
        }
    }

    /// The fingerprints of the definition named `name` starting at `line`.
    fn get(&self, file: &str, line: u32, name: &str) -> Option<&BodyFingerprints> {
        self.per_def.get(file)?.get(&(line, name.to_string()))
    }
}

/// The bodies already visited in this batch, per tier: fingerprint →
/// `(file, name, line)` of the first definition that carried it.
#[derive(Default)]
pub struct SeenBodies {
    t1: HashMap<String, (String, String, u32)>,
    t2: HashMap<String, (String, String, u32)>,
}

/// Batch-wide state the three economy checks share across one pass over a
/// file set, plus the config gates they run under.
///
/// Both `keel compile` and `keel review --base`'s two-sided pass drive W005/
/// W006/W007 from here, because the review's baseline diff is only meaningful
/// while the two sides run the *same* checks under the *same* gates: a check
/// one side skips manufactures a phantom "new" finding on the other.
pub struct EconomyBatch {
    referenced: HashSet<String>,
    duplicates: DuplicateIndex,
    seen: SeenBodies,
}

impl EconomyBatch {
    /// Precompute the batch-wide facts for `files`: the names referenced
    /// anywhere in the batch, and every function body's fingerprints.
    pub fn new(files: &[FileIndex]) -> Self {
        Self {
            referenced: batch_reference_names(files),
            duplicates: DuplicateIndex::new(files),
            seen: SeenBodies::default(),
        }
    }

    /// Run W005, W006 and W007 over one file, in that order, each gated by its
    /// `enforce.*` switch.
    ///
    /// `existing_nodes` are the file's stored nodes. `size_nodes` is what W007
    /// gets for the "and it grew" half of its semantics: `keel compile` passes
    /// the stored nodes, while the review's two-sided pass passes `&[]` to
    /// reduce W007 to a pure over-budget test — there the growth signal comes
    /// from the base side of the diff instead.
    pub fn check_file(
        &mut self,
        file: &FileIndex,
        store: &dyn GraphStore,
        existing_nodes: &[GraphNode],
        size_nodes: &[GraphNode],
        cfg: &EnforceConfig,
    ) -> Vec<Violation> {
        let mut out = Vec::new();
        if cfg.dead_code {
            out.extend(check_dead_code(
                file,
                store,
                existing_nodes,
                &self.referenced,
            ));
        }
        if cfg.duplication {
            out.extend(check_duplicate_implementation(
                file,
                store,
                &self.duplicates,
                &mut self.seen,
            ));
        }
        if cfg.oversized_files {
            out.extend(check_oversized_file(file, size_nodes, cfg.max_file_lines));
        }
        out
    }
}

/// W005: private functions in this file with zero incoming `calls`/`uses`
/// edges in the graph and no reference to them anywhere in the current
/// compile batch.
///
/// Precision depends on graph freshness: edges reflect the last `keel map`.
/// Public functions, entrypoints, tests, stubs, underscore-prefixed names,
/// qualified (method-like) names, decorated functions, and `keel:keep`-marked
/// definitions are exempt.
pub fn check_dead_code(
    file: &FileIndex,
    store: &dyn GraphStore,
    existing_nodes: &[GraphNode],
    referenced: &HashSet<String>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    if is_test_file(&file.file_path)
        || is_stub_file(&file.file_path)
        || is_bench_file(&file.file_path)
    {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function
            || def.is_public
            // Symbols in a #[cfg(test)] module or a #[test]/#[tokio::test]
            // function are invoked by the harness, not by production code —
            // "no callers" is vacuously true regardless of naming convention.
            || def.in_test_context
            // Trait-impl methods are reached through static/dynamic dispatch
            // (`&dyn Trait`, generic bounds, blanket impls) that the call graph
            // resolves to the trait declaration, not to each implementor — so
            // "no callers" is an artifact of the analysis, not dead code.
            || def.in_trait_context
            // `_`-prefixed, `bench_*`, qualified and entrypoint names.
            || is_entrypoint(def)
            // A Python `@register("evt")` / `@app.route(...)`-decorated
            // function is handed to the decorator, not called by name — a
            // framework holds the reference. E002/E003 still apply.
            || def.is_decorated
            // `keel:keep` — the per-symbol escape hatch for dynamic dispatch
            // (`globals()[name]()`, a handler table) no exemption rule can see.
            || def.has_keep_marker
        {
            continue;
        }
        if referenced.contains(&def.name) {
            continue;
        }

        // Only STORED functions carry evidence (graph edges). A def with no
        // stored node has no edge history — and in the mainline hook flow
        // (one file compiled per edit) a freshly extracted helper's caller
        // often isn't written yet, so flagging it would misfire constantly.
        // New dead code is caught on the compile after the next `keel map`.
        // An undecidable pairing (same-named siblings, no hash evidence) is
        // skipped rather than resolved against the wrong node's edges.
        let Some(node) = bind_to_node(def, file, existing_nodes) else {
            continue;
        };
        // `Uses` counts as a caller: a function handed around as a value
        // (callback, handler table, `#[serde(default = "...")]`) is used, even
        // though nothing in the graph calls it by name.
        let has_callers = store
            .get_edges(node.id, EdgeDirection::Incoming)
            .iter()
            .any(|e| e.kind == EdgeKind::Calls || e.kind == EdgeKind::Uses);
        if has_callers {
            continue;
        }
        let confidence = 0.7;
        let stored = Some(node);

        violations.push(Violation {
            code: "W005".to_string(),
            severity: "WARNING".to_string(),
            category: "dead_code".to_string(),
            message: format!("Function `{}` has no callers", def.name),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: stored.map(|n| n.hash.clone()).unwrap_or_default(),
            confidence,
            resolution_tier: "heuristic".to_string(),
            fix_hint: Some(format!(
                "No callers found for `{}` — delete it, or wire it in and re-run \
                 `keel map` to refresh call edges",
                def.name
            )),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// True when `def` and the function its fingerprint matches are BOTH on a
/// trait contract surface.
///
/// Two implementors of one trait legitimately share a body shape (`fn as_str`,
/// `fn fmt`, a defaulted method copied into an override) and cannot be deduped
/// — each impl must supply its own. Only exempt when the COUNTERPART is also a
/// trait method: a trait impl that duplicates a free function's body is still a
/// real "extract a helper" finding.
fn trait_pair_exempt(
    def: &Definition,
    fingerprint: &str,
    trait_bodies: &HashMap<String, HashSet<String>>,
    own_file: &str,
) -> bool {
    def.in_trait_context
        && trait_bodies
            .get(fingerprint)
            .is_some_and(|files| files.iter().any(|f| f != own_file))
}

/// The cross-file twin of `fingerprint` as `(name, file, line)`, preferring a
/// stored-graph match over one seen earlier in this batch.
///
/// Cross-FILE only: identical siblings within one file are usually a
/// deliberate dispatch pattern (trait impls, `format_*` fan-out), while a copy
/// in another file is the drift risk worth flagging.
fn cross_file_twin(
    matches: Vec<BodyIndexEntry>,
    seen: &HashMap<String, (String, String, u32)>,
    fingerprint: &str,
    own_file: &str,
) -> Option<(String, String, u32)> {
    matches
        .into_iter()
        .filter(|m| m.file_path != own_file)
        .find(|m| !is_test_file(&m.file_path))
        .map(|m| (m.name, m.file_path, m.line))
        .or_else(|| {
            seen.get(fingerprint)
                .filter(|(f, _, _)| f != own_file)
                .map(|(f, n, l)| (n.clone(), f.clone(), *l))
        })
}

/// One W006 violation. The two tiers differ only in wording and confidence;
/// every other field is fixed here so they cannot drift apart.
fn duplicate_violation(
    def: &Definition,
    file_path: &str,
    message: String,
    fix_hint: String,
    confidence: f64,
) -> Violation {
    Violation {
        code: "W006".to_string(),
        severity: "WARNING".to_string(),
        category: "duplicate_implementation".to_string(),
        message,
        file: file_path.to_string(),
        line: def.line_start,
        hash: String::new(),
        confidence,
        resolution_tier: "heuristic".to_string(),
        fix_hint: Some(fix_hint),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

/// W006: function bodies duplicated in another file — either byte-identical
/// after whitespace normalization (Type-1), or identical in structure once
/// identifiers are renamed and literals collapsed (Type-2, issue #59). The
/// counterpart is either already in the graph's body index (populated at
/// `keel map` time) or was seen earlier in this same compile batch.
///
/// Type-1 wins: a def that matches exactly never also reports the weaker
/// Type-2 finding, so one duplicated body yields exactly one violation.
///
/// `index` holds the batch's precomputed fingerprints (a body absent from it
/// was never fingerprinted and is skipped); `seen` carries the per-tier
/// first-sighting maps and must be shared by every file of one compile call.
pub fn check_duplicate_implementation(
    file: &FileIndex,
    store: &dyn GraphStore,
    index: &DuplicateIndex,
    seen: &mut SeenBodies,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    if is_test_file(&file.file_path) || is_stub_file(&file.file_path) {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        // Inline `#[cfg(test)] mod tests` fixtures (`fn node(..)`, `fn cs(..)`)
        // are deliberately duplicated per test module so each stays readable
        // and independently editable — sharing them across crates would couple
        // unrelated test suites. `is_test_file` above only covers whole test
        // FILES; these live inside production files.
        if def.in_test_context {
            continue;
        }
        let Some(fingerprints) = index.get(&file.file_path, def.line_start, &def.name) else {
            continue;
        };
        // A body under the Type-1 floor is too trivial to judge in EITHER
        // tier: one-line getters and delegations legitimately repeat.
        let Some(body_hash) = &fingerprints.t1 else {
            continue;
        };

        let t1_twin = if trait_pair_exempt(def, body_hash, &index.trait_t1, &file.file_path) {
            None
        } else {
            cross_file_twin(
                store.find_body_matches(body_hash),
                &seen.t1,
                body_hash,
                &file.file_path,
            )
        };

        if let Some((other_name, other_file, other_line)) = t1_twin {
            violations.push(duplicate_violation(
                def,
                &file.file_path,
                format!(
                    "Body of `{}` is identical to `{}` at {}:{}",
                    def.name, other_name, other_file, other_line
                ),
                format!(
                    "Call `{}` ({}:{}) instead of duplicating it, or extract a \
                     shared helper",
                    other_name, other_file, other_line
                ),
                DUPLICATE_T1_CONFIDENCE,
            ));
        } else if let Some(t2) = &fingerprints.t2 {
            let t2_twin = if trait_pair_exempt(def, t2, &index.trait_t2, &file.file_path) {
                None
            } else {
                cross_file_twin(
                    store.find_t2_body_matches(t2),
                    &seen.t2,
                    t2,
                    &file.file_path,
                )
            };
            if let Some((other_name, other_file, other_line)) = t2_twin {
                violations.push(duplicate_violation(
                    def,
                    &file.file_path,
                    format!(
                        "Body of `{}` is a near-duplicate of `{}` at {}:{} \
                         (same structure, renamed identifiers/literals)",
                        def.name, other_name, other_file, other_line
                    ),
                    format!(
                        "Structurally identical to `{}` ({}:{}) apart from renamed \
                         names/literals — extract a shared helper or call it directly",
                        other_name, other_file, other_line
                    ),
                    DUPLICATE_T2_CONFIDENCE,
                ));
            }
        }

        // Bookkeeping runs whether or not this def fired: a def that already
        // has a Type-1 duplicate is still the Type-2 twin a later file in the
        // batch may be looking for.
        seen.t1
            .entry(body_hash.clone())
            .or_insert_with(|| (file.file_path.clone(), def.name.clone(), def.line_start));
        if let Some(t2) = &fingerprints.t2 {
            seen.t2
                .entry(t2.clone())
                .or_insert_with(|| (file.file_path.clone(), def.name.clone(), def.line_start));
        }
    }

    violations
}

/// W007: the file's extent exceeds the configured budget AND grew relative
/// to the stored graph state (shrinking an already-over file stays silent —
/// reduction is the desired direction).
///
/// Both sides of the grew-comparison use the SAME measure — the highest
/// definition end line. (Stored Module nodes record the whole file's line
/// count, a different quantity: comparing against it would mask real growth
/// behind trailing footers/test-mod declarations.) Trailing comments aren't
/// counted; that under-approximation only makes the check more conservative.
///
/// Pass an empty `existing_nodes` to get the budget test alone, with no growth
/// gate: that is how `crate::review::baseline` runs it, so the "and it grew"
/// half comes from the base side of a PR rather than from whatever commit the
/// graph was last mapped at.
pub fn check_oversized_file(
    file: &FileIndex,
    existing_nodes: &[GraphNode],
    max_lines: u32,
) -> Vec<Violation> {
    if is_test_file(&file.file_path) || is_stub_file(&file.file_path) {
        return vec![];
    }

    // Module-kind defs (`mod tests;` items end at EOF) are excluded on BOTH
    // sides — any asymmetry here manufactures phantom growth.
    let extent = file
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .map(|d| d.line_end)
        .max()
        .unwrap_or(0);
    if extent <= max_lines {
        return vec![];
    }

    let stored_extent = existing_nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .map(|n| n.line_end)
        .max()
        .unwrap_or(0);
    if extent <= stored_extent {
        return vec![];
    }

    let module_hash = existing_nodes
        .iter()
        .find(|n| n.kind == NodeKind::Module)
        .map(|n| n.hash.clone())
        .unwrap_or_default();

    vec![Violation {
        code: "W007".to_string(),
        severity: "WARNING".to_string(),
        category: "oversized_file".to_string(),
        message: format!(
            "File is ~{} lines (budget {}) and growing",
            extent, max_lines
        ),
        file: file.file_path.clone(),
        line: 1,
        hash: module_hash,
        confidence: 0.8,
        resolution_tier: "heuristic".to_string(),
        fix_hint: Some(format!(
            "Split {} into focused modules under {} lines — run `keel analyze {}` \
             for split suggestions, or delete what's no longer needed",
            file.file_path, max_lines, file.file_path
        )),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }]
}

#[cfg(test)]
#[path = "violations_economy_tests.rs"]
mod tests;
