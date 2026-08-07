use std::collections::HashSet;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};
use keel_parsers::resolver::FileIndex;

use crate::types::{AffectedNode, Violation};
use crate::violations_util::{
    extract_prefix, is_cargo_binary_root, is_test_file, normalize_signature, parse_signature,
    receiver_is_type_like,
};

/// Check E004: function_removed — a function was removed but callers still exist.
/// Compares existing nodes in the store against current file definitions.
pub fn check_removed_functions(file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
    let nodes = store.get_nodes_in_file(&file.file_path);
    let empty: HashSet<&str> = HashSet::new();
    check_removed_functions_with_cache(file, store, &nodes, &empty)
}

/// E004 implementation using pre-fetched nodes to avoid redundant queries.
/// When `batch_files` is non-empty, callers whose source file is in the batch
/// are skipped — the user likely updated or removed the call in the same commit.
pub fn check_removed_functions_with_cache(
    file: &FileIndex,
    store: &dyn GraphStore,
    stored_nodes: &[keel_core::types::GraphNode],
    batch_files: &HashSet<&str>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let current_names: std::collections::HashSet<&str> =
        file.definitions.iter().map(|d| d.name.as_str()).collect();

    for node in stored_nodes {
        if node.kind != NodeKind::Function {
            continue;
        }
        if current_names.contains(node.name.as_str()) {
            continue;
        }

        // Function was removed — check for callers not in the compile batch
        let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
        let callers: Vec<_> = incoming
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .filter_map(|e| store.get_node_by_id(e.source_id))
            .filter(|c| !batch_files.contains(c.file_path.as_str()))
            .collect();

        if callers.is_empty() {
            continue; // No callers, safe to remove
        }

        let affected: Vec<AffectedNode> = callers
            .iter()
            .map(|c| AffectedNode {
                hash: c.hash.clone(),
                name: c.name.clone(),
                file: c.file_path.clone(),
                line: c.line_start,
            })
            .collect();

        violations.push(Violation {
            code: "E004".to_string(),
            severity: "ERROR".to_string(),
            category: "function_removed".to_string(),
            message: format!(
                "Function `{}` was removed but has {} caller(s)",
                node.name,
                callers.len()
            ),
            file: file.file_path.clone(),
            line: node.line_start,
            hash: node.hash.clone(),
            confidence: 0.95,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Remove or update callers of `{}` before deleting it — e.g. {}:{}",
                node.name, callers[0].file_path, callers[0].line_start
            )),
            suppressed: false,
            suppress_hint: None,
            affected,
            suggested_module: None,
            existing: None,
        });
    }

    violations
}

/// Check E005: arity_mismatch — caller passes wrong number of arguments.
/// Compares the parser-counted call-site arguments against the target
/// definition's parameter count.
///
/// Honest by construction — the comparison only runs when every input is
/// precisely known: the reference must be resolved (`resolved_to`, set by the
/// compile pipeline's pre-enforcement resolution pass at error-tier confidence
/// only), the call site must carry a syntactic argument count (`call_arity` is
/// `None` for splats/spreads and call-shaped non-calls), and the signature
/// must be exactly countable (`parse_signature` returns `None` for variadic,
/// defaulted, or optional parameter lists).
///
/// **Call-side receivers.** `parse_signature` strips a definition-side
/// `self`/`cls`/`this` (never written at an ordinary call site). The one call
/// shape that *does* write the receiver as its first argument goes through
/// the type — `Base.__init__(self, x)`, `Rc::clone(&x)` — so when the
/// reference's receiver segment is type-like and the target can take an
/// explicit receiver, one extra argument is tolerated. `registry.register
/// (self)` (a value receiver passing `self` as a genuine argument) gets no
/// tolerance and is compared exactly.
///
/// `batch_files` maps the file paths of this compile batch to their freshly
/// parsed indices: when the target lives in the batch, its *current*
/// signature wins over the stored (pre-edit) one, so changing a signature and
/// its callers in the same compile is judged against the new contract.
pub fn check_arity_mismatch(
    file: &FileIndex,
    store: &dyn GraphStore,
    batch_files: &std::collections::HashMap<&str, &FileIndex>,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    // hash -> stored node memo: a file typically calls the same target many
    // times, and each store lookup is several SQLite statements.
    let mut node_memo: std::collections::HashMap<&str, Option<keel_core::types::GraphNode>> =
        std::collections::HashMap::new();

    for reference in &file.references {
        if reference.kind != keel_parsers::resolver::ReferenceKind::Call {
            continue;
        }
        let Some(target_hash) = &reference.resolved_to else {
            continue;
        };
        let Some(call_arity) = reference.call_arity else {
            continue;
        };
        if !node_memo.contains_key(target_hash.as_str()) {
            node_memo.insert(target_hash.as_str(), store.get_node(target_hash));
        }
        let Some(target_node) = node_memo.get(target_hash.as_str()).and_then(|n| n.as_ref()) else {
            continue;
        };

        // The freshest facts available: the batch's parsed definition if the
        // target's file was compiled alongside, else the stored node. A batch
        // file that no longer defines the target means the function was
        // removed — that is E004's finding, not an arity comparison.
        let (signature, is_associated) = match batch_files.get(target_node.file_path.as_str()) {
            Some(target_file) => {
                // Same-named defs can swap lines in one edit: prefer the
                // exact (name, line) match, then fall back to name — but only
                // when the name is UNIQUE in the file. Two same-named defs
                // make the fallback a coin flip, and a wrong signature here
                // becomes a wrong ERROR (same refusal rule as the resolution
                // pass's ambiguity guards).
                let defs = &target_file.definitions;
                let def = defs
                    .iter()
                    .find(|d| d.name == target_node.name && d.line_start == target_node.line_start)
                    .or_else(|| {
                        let mut same_named = defs.iter().filter(|d| d.name == target_node.name);
                        match (same_named.next(), same_named.next()) {
                            (Some(only), None) => Some(only),
                            _ => None,
                        }
                    });
                match def {
                    Some(def) => (def.signature.as_str(), def.is_associated),
                    None => continue,
                }
            }
            None => (target_node.signature.as_str(), target_node.is_associated),
        };
        let Some(sig) = parse_signature(signature) else {
            continue;
        };

        // `is_associated` widens the receiver tolerance ONLY for Go: a Go
        // method's stored signature carries no receiver token, so
        // `has_receiver` cannot see it, yet a method expression `T.Scan(t, x)`
        // legally writes the receiver first. For every other language an
        // associated function without a receiver in its signature really
        // takes none — `Foo::new(bogus, x)` is a genuine extra argument and
        // must keep firing.
        let go_method = is_associated
            && keel_parsers::treesitter::detect_language(std::path::Path::new(
                &target_node.file_path,
            )) == Some("go");
        let call_arity = call_arity as usize;
        let explicit_receiver_ok = (sig.has_receiver || go_method)
            && receiver_is_type_like(&reference.name)
            && call_arity == sig.arity + 1;
        if call_arity == sig.arity || explicit_receiver_ok {
            continue;
        }

        violations.push(Violation {
            code: "E005".to_string(),
            severity: "ERROR".to_string(),
            category: "arity_mismatch".to_string(),
            message: format!(
                "Call to `{}` passes {} arg(s) but function expects {}",
                target_node.name, call_arity, sig.arity
            ),
            file: file.file_path.clone(),
            line: reference.line,
            hash: target_node.hash.clone(),
            confidence: 0.85,
            resolution_tier: "tree-sitter".to_string(),
            fix_hint: Some(format!(
                "Update call to `{}` to pass {} argument(s)",
                target_node.name, sig.arity
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

/// Check W001: placement — function may be in wrong module.
/// Uses indexed SQL query instead of scanning all modules. O(F) not O(F*M).
pub fn check_placement(file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
    let mut violations = Vec::new();

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }

        let fn_prefix = extract_prefix(&def.name);
        if fn_prefix.is_empty() {
            continue;
        }

        // Single SQL query per function — finds modules with matching prefix
        let matching = store.find_modules_by_prefix(&fn_prefix, &file.file_path);
        if let Some(profile) = matching.first() {
            violations.push(Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: format!(
                    "Function `{}` may belong in module `{}`",
                    def.name, profile.path
                ),
                file: file.file_path.clone(),
                line: def.line_start,
                hash: def.hash(),
                confidence: 0.6,
                resolution_tier: "heuristic".to_string(),
                fix_hint: Some(format!(
                    "Consider moving `{}` to `{}`",
                    def.name, profile.path
                )),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: Some(profile.path.clone()),
                existing: None,
            });
        }
    }

    violations
}

/// Check W002: duplicate_name — same function name AND same (whitespace-
/// normalized) signature exists in **exactly one** other module.
///
/// Matching by name alone over-fires on idiomatic namespacing — e.g. every
/// CLI subcommand module defining its own `run(args) -> Result<()>` isn't a
/// naming collision, it's the pattern working as intended. Requiring the
/// signature to match too keeps the warning for genuine accidental
/// duplicates (same name, same shape) while staying silent on same-name/
/// different-signature functions that just happen to share a verb.
///
/// Name+signature equality alone still isn't enough, though: trait/interface
/// conformance produces the exact same signature at every impl site by
/// construction (e.g. four `LanguageResolver` impls all defining an
/// identically-shaped `parse_file`, or every type's `Default`/`fmt` impl).
/// That's a deliberate pattern, not a collision. The discriminator is count:
/// an accidental duplicate comes in a **pair** (one copy-pasted from the
/// other); three or more identical sites is conformance to a shared
/// contract. So this only fires when exactly one other matching site
/// exists — 2 total copies. 3+ copies stay silent here (W006's body-hash
/// check still catches genuine multi-copy duplication independent of name).
///
/// One more pair is exempt regardless of count: `fn main()` across two Cargo
/// compilation roots (`build.rs`, `src/main.rs`, `src/bin/*.rs`,
/// `examples/*.rs`). Cargo compiles each as its own independent crate, so a
/// `main` in one is invisible to every other — there is no ambiguity to
/// rename away (issue #62). Gated strictly on `def.name == "main"` so it
/// cannot mask an accidental duplicate of some other function that happens to
/// live in one of those roots.
///
/// Uses indexed SQL query instead of triple-nested loop. O(F) not O(F*M*N).
pub fn check_duplicate_names(file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Skip W002 entirely if the current file is a test file
    if is_test_file(&file.file_path) {
        return violations;
    }

    for def in &file.definitions {
        if def.kind != NodeKind::Function {
            continue;
        }
        // Trait methods MUST share their name across the declaration and every
        // implementor — that is the whole point of a trait. Renaming one is not
        // a legal fix, so flagging it is pure noise.
        //
        // Associated items generally (methods and associated functions on a
        // type — Rust `impl` blocks, class bodies, Go receiver funcs) are
        // likewise exempt: they are addressed as `Type::name` / `obj.name`, so
        // a shared bare name across unrelated types (`is_expired`, `in_memory`,
        // `as_str`) is idiomatic, not ambiguity — and W002's "rename" fix_hint
        // would push an agent toward LESS idiomatic code. Free functions that
        // collide are still reported: those genuinely share one namespace.
        //
        // The exemption is two-sided. This `def.is_associated` check covers
        // the definition in the file being compiled. The other side of the
        // comparison is a `GraphNode` loaded from storage for some other file,
        // which is just as capable of being an associated method — the
        // persisted `node.is_associated` flag (issue #46) filters those out of
        // the match set below, so a genuinely free function in the current
        // file no longer collides with a stored `impl` method of the same
        // name+signature across separate compiles.
        //
        // Test-context helpers are likewise exempt: every `#[cfg(test)] mod
        // tests` block independently defines its own `fn node(..)`/`fn root()`
        // fixture, which is the idiomatic pattern, not accidental ambiguity.
        // (`is_test_file` above only covers whole test FILES; these live in
        // inline test modules inside production files.)
        if def.in_trait_context || def.in_test_context || def.is_associated {
            continue;
        }

        // Single SQL query per function — finds same-named functions elsewhere
        let duplicates = store.find_nodes_by_name(&def.name, "function", &file.file_path);
        let normalized_def_sig = normalize_signature(&def.signature);
        let same_shape: Vec<_> = duplicates
            .iter()
            // Skip test files in results
            .filter(|node| !is_test_file(&node.file_path))
            // Stored associated methods (persisted `is_associated`, issue #46)
            // are exempt on this side too: a free function here must not
            // collide with an `impl`/class method of the same name elsewhere.
            .filter(|node| !node.is_associated)
            // Same name but a different shape is idiomatic namespacing, not
            // a duplicate — only consider sites where the signature matches.
            .filter(|node| normalize_signature(&node.signature) == normalized_def_sig)
            .collect();

        // Exactly one other matching site = suspect pair. Zero = no
        // collision. Two or more = a shared contract (trait/interface
        // conformance) — deliberate, not accidental — stay silent.
        if same_shape.len() != 1 {
            continue;
        }
        let node = same_shape[0];

        // Cargo-mandated dual `main`: build.rs vs src/main.rs, or two
        // independent binary targets (src/bin/*.rs, examples/*.rs) — each
        // compiles as its own crate, so there is no ambiguity to rename away
        // (issue #62).
        if def.name == "main"
            && is_cargo_binary_root(&file.file_path)
            && is_cargo_binary_root(&node.file_path)
        {
            continue;
        }

        violations.push(Violation {
            code: "W002".to_string(),
            severity: "WARNING".to_string(),
            category: "duplicate_name".to_string(),
            message: format!(
                "Function `{}` also exists in `{}`",
                def.name, node.file_path
            ),
            file: file.file_path.clone(),
            line: def.line_start,
            hash: def.hash(),
            confidence: 0.7,
            resolution_tier: "heuristic".to_string(),
            fix_hint: Some(format!(
                "Rename `{}` here or its duplicate in `{}` to avoid ambiguity",
                def.name, node.file_path
            )),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: Some(crate::types::ExistingNode {
                hash: node.hash.clone(),
                file: node.file_path.clone(),
                line: node.line_start,
            }),
        });
    }

    violations
}
