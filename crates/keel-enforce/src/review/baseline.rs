//! Baseline-relative violations: what this diff *introduced*.
//!
//! A repo carrying 34,226 violations cannot be gated on its violation count —
//! any PR comment listing current findings is dead on arrival. What a reviewer
//! can act on is the far smaller set the diff *added*, so this module compiles
//! both sides of [`super::diff`]'s two-sided parse and reports only the
//! difference.
//!
//! Three properties make the difference trustworthy:
//!
//! 1. **Symmetry.** Both sides run the *same* checks over the *same* Tier-1
//!    parse against the *same* stored graph. Nothing here consults `ty`, oxc, or
//!    rust-analyzer, because a code available on one side and not the other
//!    manufactures phantom "new" findings — which is precisely why E001/E004/
//!    E005 are **not** in [`DIFFABLE_CODES`]: they depend on cross-file
//!    reference resolution the base blobs never got. Those stay head-only, on
//!    the `keel compile` surface where they already live, and the review
//!    reports the same fact structurally instead (a moved contract with callers
//!    outside the diff).
//! 2. **Line-independence.** Findings match on `(code, file, symbol)`, never on
//!    a line, so reformatting a file with existing violations introduces none.
//!    This is the cross-*revision* sibling of `ViolationKey::stable`, which
//!    keys on `(code, hash, file)` for `compile --delta`: there, the two sides
//!    are the same revision plus one edit and the AST hash is exactly the right
//!    identity. Here every touched body hash necessarily differs, so hashing
//!    would report "new" E003 for every undocumented function a PR so much as
//!    reformatted — the 34k-findings failure mode this module exists to avoid.
//!    The symbol a finding is *about* is what survives a revision.
//! 3. **Per-PR size attribution.** W007 runs here with *no* stored nodes, which
//!    reduces it to a pure "over the budget" test; the "and it grew" half of its
//!    semantics comes from the base side of this diff instead of from whatever
//!    commit the graph was last mapped at. A file that was already over budget
//!    is over budget on both sides and cancels; a file this PR pushed past the
//!    budget appears exactly once.

use std::collections::{BTreeMap, HashMap, HashSet};

use keel_core::config::EnforceConfig;
use keel_core::store::GraphStore;
use keel_parsers::resolver::FileIndex;

use crate::types::Violation;
use crate::{violations, violations_economy};

use super::diff::DiffScan;

/// The codes the two-sided pass computes symmetrically, and therefore the only
/// ones a baseline diff may claim are "new".
///
/// Deliberately excludes E001/E004/E005 (need resolved cross-file references),
/// W001/W002 (need the repo-wide module and name census the base side has no
/// equivalent of) and W009/E006 (self-baselining already — they fire only on
/// dependencies absent from the stored graph, which is the same job done at
/// edit time).
pub const DIFFABLE_CODES: [&str; 5] = ["E002", "E003", "W005", "W006", "W007"];

/// Everything the baseline diff concluded about one PR.
pub struct BaselineDiff {
    /// Violations present on the head side and absent from the base side,
    /// ordered by file, then line, then code.
    pub new_violations: Vec<Violation>,
    /// Head-side findings that also existed on the base side — the noise this
    /// mechanism exists to keep off the PR.
    pub pre_existing: usize,
}

/// Run the diffable checks over one side of the diff.
///
/// `store` is the live graph, used identically for both sides: W005 asks it for
/// incoming edges and W006 for body matches. Whatever commit it was mapped at,
/// both sides see the same answers, so a difference between them comes from the
/// code and not from the graph.
fn side_violations(
    indices: &[FileIndex],
    store: &dyn GraphStore,
    cfg: &EnforceConfig,
) -> Vec<Violation> {
    // Both check groups are the ones `Engine::compile` runs, driven from their
    // shared entry points so a gate or an ordering can never differ between
    // the two surfaces.
    let mut economy = violations_economy::EconomyBatch::new(indices);
    let mut out = Vec::new();

    for file in indices {
        out.extend(violations::check_annotations(file, cfg));
        // Only W005 reads the stored nodes here, so skip the query when it is
        // off. W007 gets none on purpose — see the module docs.
        let existing = if cfg.dead_code {
            store.get_nodes_in_file(&file.file_path)
        } else {
            Vec::new()
        };
        out.extend(economy.check_file(file, store, &existing, &[], cfg));
    }

    debug_assert!(
        out.iter()
            .all(|v| DIFFABLE_CODES.contains(&v.code.as_str())),
        "side_violations emitted a code outside DIFFABLE_CODES"
    );
    out
}

/// `(file, line)` → the name of the definition that starts there.
///
/// Every per-symbol check reports at its definition's `line_start`, which makes
/// this the bridge from a violation back to the symbol it is about. W007 is the
/// exception — a file-level finding pinned to line 1, so it keys on whichever
/// definition starts the file or on nothing at all. Either way both sides agree,
/// which is all the identity has to do.
type SymbolLines<'a> = HashMap<(&'a str, u32), &'a str>;

fn symbol_lines(indices: &[FileIndex]) -> SymbolLines<'_> {
    indices
        .iter()
        .flat_map(|f| {
            f.definitions
                .iter()
                .map(move |d| ((f.file_path.as_str(), d.line_start), d.name.as_str()))
        })
        .collect()
}

/// The identity a finding keeps across a revision: its code, the file it lives
/// in (canonicalized to the base-side path across a rename), and the symbol it
/// is about.
fn finding_key(
    v: &Violation,
    symbols: &SymbolLines<'_>,
    renames: &BTreeMap<String, String>,
) -> (String, String, String) {
    let file = renames.get(&v.file).unwrap_or(&v.file).clone();
    let symbol = symbols
        .get(&(v.file.as_str(), v.line))
        .copied()
        .unwrap_or_default();
    (v.code.clone(), file, symbol.to_string())
}

/// Diff the head side's violations against the base side's.
pub fn diff(store: &dyn GraphStore, scan: &DiffScan, cfg: &EnforceConfig) -> BaselineDiff {
    let base = side_violations(&scan.base_indices, store, cfg);
    let head = side_violations(&scan.head_indices, store, cfg);

    let base_symbols = symbol_lines(&scan.base_indices);
    let head_symbols = symbol_lines(&scan.head_indices);
    let no_renames = BTreeMap::new();
    let base_keys: HashSet<(String, String, String)> = base
        .iter()
        .map(|v| finding_key(v, &base_symbols, &no_renames))
        .collect();

    let mut pre_existing = 0usize;
    let mut new_violations: Vec<Violation> = Vec::new();
    let mut claimed: HashSet<(String, String, String)> = HashSet::new();
    for v in head {
        let key = finding_key(&v, &head_symbols, &scan.renames);
        if base_keys.contains(&key) {
            pre_existing += 1;
            continue;
        }
        // One line per identity: a head side that produced the same key twice
        // (two symbol-less findings in one file) reports once.
        if !claimed.insert(key) {
            continue;
        }
        new_violations.push(v);
    }

    new_violations.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.code.cmp(&b.code))
    });

    BaselineDiff {
        new_violations,
        pre_existing,
    }
}

/// The new violations whose code the repo opted into gating on.
///
/// Gating is off unless *both* halves are present: the `--gate` switch on the
/// command line and a non-empty `review.gate` list in `keel.json`. New findings
/// are a report by default (the `validate-plan` precedent) — turning them into
/// a red build is a decision a repo makes per code, once, in config.
pub fn gate_hits<'a>(new_violations: &'a [Violation], gate: &[String]) -> Vec<&'a Violation> {
    new_violations
        .iter()
        .filter(|v| gate.iter().any(|c| c == &v.code))
        .collect()
}

#[cfg(test)]
#[path = "baseline_tests.rs"]
mod tests;
