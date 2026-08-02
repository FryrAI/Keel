//! Economy checks: keep the codebase lean.
//!
//! - W005 `dead_code` — private function with no callers or value usages in
//!   the graph.
//! - W006 `duplicate_implementation` — body identical (whitespace-normalized)
//!   to a function in another file.
//! - W007 `oversized_file` — file exceeds the configured line budget and grew.
//!
//! All three are WARNING severity, deferrable in batch mode, and gated by
//! `enforce.{dead_code, duplication, oversized_files}` in keel.json.

use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex};

use crate::types::Violation;
use crate::violations_util::{is_bench_file, is_stub_file, is_test_file, node_hash_matches};

/// Bodies shorter than this (whitespace-normalized) are too trivial to call
/// duplicates — one-line getters and delegations legitimately repeat.
const MIN_DUPLICATE_BODY_LEN: usize = 60;

// Anything this check wants to match must have been indexed at map time,
// which gates on the (lower) MIN_INDEXED_BODY_LEN.
const _: () = assert!(MIN_DUPLICATE_BODY_LEN >= keel_core::hash::MIN_INDEXED_BODY_LEN);

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

/// Index the normalized-body hashes of every trait-context definition in the
/// compile batch, mapped to the files that define them.
///
/// W006 consults this to answer "is the function my body matches ALSO a trait
/// method?" — the graph does not persist trait-context provenance, so the
/// answer has to come from the freshly parsed batch.
pub fn batch_trait_context_bodies(files: &[FileIndex]) -> HashMap<String, HashSet<String>> {
    let mut bodies: HashMap<String, HashSet<String>> = HashMap::new();
    for file in files {
        for def in &file.definitions {
            if !def.in_trait_context || def.kind != NodeKind::Function {
                continue;
            }
            let normalized = keel_core::hash::normalize_body(&def.body_text);
            if normalized.len() < MIN_DUPLICATE_BODY_LEN {
                continue;
            }
            bodies
                .entry(keel_core::hash::hash_normalized_body(&normalized))
                .or_default()
                .insert(file.file_path.clone());
        }
    }
    bodies
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

/// W006: function bodies identical (whitespace-normalized) to another
/// function — either one already in the graph's body index (populated at
/// `keel map` time) or one seen earlier in this same compile batch.
///
/// `seen_bodies` maps body_hash → (file, name, line) across the batch and
/// must be shared by every file of one compile call.
pub fn check_duplicate_implementation(
    file: &FileIndex,
    store: &dyn GraphStore,
    seen_bodies: &mut HashMap<String, (String, String, u32)>,
    trait_bodies: &HashMap<String, HashSet<String>>,
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
        let normalized = keel_core::hash::normalize_body(&def.body_text);
        if normalized.len() < MIN_DUPLICATE_BODY_LEN {
            continue;
        }
        let body_hash = keel_core::hash::hash_normalized_body(&normalized);

        // Both sides on a trait contract surface: two implementors of the same
        // trait legitimately share a body shape (`fn as_str`, `fn fmt`, a
        // defaulted trait method copied into an override). Deduping them is not
        // possible — each impl must supply its own. Only skip when the
        // COUNTERPART is also a trait method: a trait impl that duplicates a
        // free function's body is still a real "extract a helper" finding.
        if def.in_trait_context
            && trait_bodies
                .get(&body_hash)
                .is_some_and(|files| files.iter().any(|f| f != &file.file_path))
        {
            continue;
        }

        // Cross-FILE matches only: identical siblings within one file are
        // usually a deliberate dispatch pattern (trait impls, format_* fan-out),
        // while a copy in another file is the drift risk worth flagging.
        // Prefer a graph-index match; fall back to a batch-local one.
        let graph_match = store
            .find_body_matches(&body_hash)
            .into_iter()
            .filter(|m| m.file_path != file.file_path)
            .find(|m| !is_test_file(&m.file_path));
        let local_match = seen_bodies
            .get(&body_hash)
            .filter(|(f, _, _)| f != &file.file_path)
            .cloned();

        let duplicate = graph_match
            .map(|m| (m.name, m.file_path, m.line))
            .or_else(|| local_match.map(|(f, n, l)| (n, f, l)));

        if let Some((other_name, other_file, other_line)) = duplicate {
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
                confidence: 0.85,
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
        }

        seen_bodies
            .entry(body_hash)
            .or_insert_with(|| (file.file_path.clone(), def.name.clone(), def.line_start));
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
