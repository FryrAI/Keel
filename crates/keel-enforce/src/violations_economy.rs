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
use std::path::Path;

use keel_core::config::EnforceConfig;
use keel_core::hash_t2;
use keel_core::store::GraphStore;
use keel_core::types::{BodyIndexEntry, EdgeDirection, EdgeKind, GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex};
use keel_parsers::treesitter::detect_language;

use crate::types::Violation;
use crate::violations_util::{is_bench_file, is_stub_file, is_test_file, node_hash_matches};

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

/// Index the body fingerprints of every trait-context definition in the
/// compile batch, mapped to the files that define them: `(type_1, type_2)`.
///
/// W006 consults this to answer "is the function my body matches ALSO a trait
/// method?" — the graph does not persist trait-context provenance, so the
/// answer has to come from the freshly parsed batch. Both tiers need their own
/// map because they key on different fingerprints of the same body.
pub fn batch_trait_context_bodies(
    files: &[FileIndex],
) -> (
    HashMap<String, HashSet<String>>,
    HashMap<String, HashSet<String>>,
) {
    let mut bodies: HashMap<String, HashSet<String>> = HashMap::new();
    let mut bodies_t2: HashMap<String, HashSet<String>> = HashMap::new();
    for file in files {
        let lang = detect_language(Path::new(&file.file_path)).unwrap_or("");
        for def in &file.definitions {
            if !def.in_trait_context || def.kind != NodeKind::Function {
                continue;
            }
            let normalized = keel_core::hash::normalize_body(&def.body_text);
            if normalized.len() >= MIN_DUPLICATE_BODY_LEN {
                bodies
                    .entry(keel_core::hash::hash_normalized_body(&normalized))
                    .or_default()
                    .insert(file.file_path.clone());
            }
            let t2_normalized = hash_t2::normalize_body_t2(&def.body_text, lang);
            if t2_normalized.len() >= MIN_T2_NORMALIZED_LEN {
                bodies_t2
                    .entry(keel_core::hash::hash_string(&t2_normalized))
                    .or_default()
                    .insert(file.file_path.clone());
            }
        }
    }
    (bodies, bodies_t2)
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
    trait_bodies: HashMap<String, HashSet<String>>,
    trait_bodies_t2: HashMap<String, HashSet<String>>,
    seen_bodies: HashMap<String, (String, String, u32)>,
    seen_bodies_t2: HashMap<String, (String, String, u32)>,
}

impl EconomyBatch {
    /// Precompute the batch-wide facts for `files` (names referenced anywhere
    /// in the batch, and the bodies that came from a trait context).
    pub fn new(files: &[FileIndex]) -> Self {
        let (trait_bodies, trait_bodies_t2) = batch_trait_context_bodies(files);
        Self {
            referenced: batch_reference_names(files),
            trait_bodies,
            trait_bodies_t2,
            seen_bodies: HashMap::new(),
            seen_bodies_t2: HashMap::new(),
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
                &mut self.seen_bodies,
                &self.trait_bodies,
                &mut self.seen_bodies_t2,
                &self.trait_bodies_t2,
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
        let candidates: Vec<&GraphNode> = existing_nodes
            .iter()
            .filter(|n| n.name == def.name)
            .collect();
        // Same-named siblings (impl-block methods): pick the node whose hash
        // matches this def; when nothing matches unambiguously, skip rather
        // than consult the wrong node's edges.
        let node = match candidates
            .iter()
            .find(|n| node_hash_matches(n, def, &file.file_path))
        {
            Some(n) => *n,
            None if candidates.len() == 1 => candidates[0],
            None => continue,
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

/// W006: function bodies duplicated in another file — either byte-identical
/// after whitespace normalization (Type-1), or identical in structure once
/// identifiers are renamed and literals collapsed (Type-2, issue #59). The
/// counterpart is either already in the graph's body index (populated at
/// `keel map` time) or was seen earlier in this same compile batch.
///
/// Type-1 wins: a def that matches exactly never also reports the weaker
/// Type-2 finding, so one duplicated body yields exactly one violation.
///
/// `seen_bodies`/`seen_bodies_t2` map fingerprint → (file, name, line) across
/// the batch and must be shared by every file of one compile call.
pub fn check_duplicate_implementation(
    file: &FileIndex,
    store: &dyn GraphStore,
    seen_bodies: &mut HashMap<String, (String, String, u32)>,
    trait_bodies: &HashMap<String, HashSet<String>>,
    seen_bodies_t2: &mut HashMap<String, (String, String, u32)>,
    trait_bodies_t2: &HashMap<String, HashSet<String>>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    if is_test_file(&file.file_path) || is_stub_file(&file.file_path) {
        return violations;
    }
    // Must be the same language string `keel map` indexed under, or no stored
    // Type-2 fingerprint ever matches — both sides go through `detect_language`.
    let lang = detect_language(Path::new(&file.file_path)).unwrap_or("");

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
        let normalized = keel_core::hash::normalize_body(&def.body_text);
        if normalized.len() < MIN_DUPLICATE_BODY_LEN {
            continue;
        }
        let body_hash = keel_core::hash::hash_normalized_body(&normalized);
        let t2_normalized = hash_t2::normalize_body_t2(&def.body_text, lang);
        let t2_hash = (t2_normalized.len() >= MIN_T2_NORMALIZED_LEN)
            .then(|| keel_core::hash::hash_string(&t2_normalized));

        let t1_twin = if trait_pair_exempt(def, &body_hash, trait_bodies, &file.file_path) {
            None
        } else {
            cross_file_twin(
                store.find_body_matches(&body_hash),
                seen_bodies,
                &body_hash,
                &file.file_path,
            )
        };

        if let Some((other_name, other_file, other_line)) = t1_twin {
            violations.push(Violation {
                code: "W006".to_string(),
                severity: "WARNING".to_string(),
                category: "duplicate_implementation".to_string(),
                message: format!(
                    "Body of `{}` is identical to `{}` at {}:{}",
                    def.name, other_name, other_file, other_line
                ),
                file: file.file_path.clone(),
                line: def.line_start,
                hash: String::new(),
                confidence: DUPLICATE_T1_CONFIDENCE,
                resolution_tier: "heuristic".to_string(),
                fix_hint: Some(format!(
                    "Call `{}` ({}:{}) instead of duplicating it, or extract a \
                     shared helper",
                    other_name, other_file, other_line
                )),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            });
        } else if let Some(t2) = &t2_hash {
            let t2_twin = if trait_pair_exempt(def, t2, trait_bodies_t2, &file.file_path) {
                None
            } else {
                cross_file_twin(
                    store.find_t2_body_matches(t2),
                    seen_bodies_t2,
                    t2,
                    &file.file_path,
                )
            };
            if let Some((other_name, other_file, other_line)) = t2_twin {
                violations.push(Violation {
                    code: "W006".to_string(),
                    severity: "WARNING".to_string(),
                    category: "duplicate_implementation".to_string(),
                    message: format!(
                        "Body of `{}` is a near-duplicate of `{}` at {}:{} \
                         (same structure, renamed identifiers/literals)",
                        def.name, other_name, other_file, other_line
                    ),
                    file: file.file_path.clone(),
                    line: def.line_start,
                    hash: String::new(),
                    confidence: DUPLICATE_T2_CONFIDENCE,
                    resolution_tier: "heuristic".to_string(),
                    fix_hint: Some(format!(
                        "Structurally identical to `{}` ({}:{}) apart from renamed \
                         names/literals — extract a shared helper or call it directly",
                        other_name, other_file, other_line
                    )),
                    suppressed: false,
                    suppress_hint: None,
                    affected: vec![],
                    suggested_module: None,
                    existing: None,
                });
            }
        }

        // Bookkeeping runs whether or not this def fired: a def that already
        // has a Type-1 duplicate is still the Type-2 twin a later file in the
        // batch may be looking for.
        seen_bodies
            .entry(body_hash)
            .or_insert_with(|| (file.file_path.clone(), def.name.clone(), def.line_start));
        if let Some(t2) = t2_hash {
            seen_bodies_t2
                .entry(t2)
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
