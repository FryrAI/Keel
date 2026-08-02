//! `keel review --base <ref>` — the two-sided graph diff, as a PR cover letter.
//!
//! GitHub can show which lines moved. It cannot say which *contracts* moved and
//! which call sites the PR left behind, because that needs the graph. This
//! module answers exactly that, in one pass:
//!
//! 1. `git diff --name-status -M <base>` for the changed paths, with renames
//!    detected rather than reported as an add plus a remove;
//! 2. the **base** side parsed straight from `git show <base>:<path>` in memory
//!    (see `crate::parse_util`) — no checkout, no worktree, no schema change;
//! 3. the **head** side parsed from the working tree with the same Tier-1
//!    resolvers, so the two sides are symmetric and a difference is a real
//!    difference rather than a tier artifact;
//! 4. per symbol: signature differs = a contract change; same signature with a
//!    different `Definition::hash()` = body-only (or doc-only);
//! 5. for each contract change, the callers the stored graph knows about that
//!    live **outside** the diff — literally the call sites this PR did not
//!    update;
//! 6. the violations the diff *introduced*, base-side findings subtracted (see
//!    `crate::review::baseline`).
//!
//! Review-time only. Nothing here runs in the compile hot path.
//!
//! Related: `crate::gitdiff` (`changed_paths`, `blob_at`), `crate::checkpoint`
//! (`callers_of`, `CallerRef`), `crate::file_class` (the unanalyzed classes).

pub mod baseline;
pub mod diff;
pub mod render;
pub mod risk;

use std::path::Path;

use serde::{Deserialize, Serialize};

use keel_core::config::EnforceConfig;
use keel_core::store::GraphStore;
/// Re-exported so consumers outside `keel-core`'s dependency graph (the output
/// formatters) can name `ContractChange::symbol_kind`.
pub use keel_core::types::NodeKind;

use crate::checkpoint::CallerRef;
use crate::gitdiff;
use crate::parse_util::BLOB_RESOLUTION_TIER;
use crate::types::Violation;

/// What happened to one symbol between the base ref and the working tree.
///
/// Exactly one kind per symbol, most-consequential-wins: a symbol in a renamed
/// file whose signature also changed reports `SignatureChanged`, not `Moved` —
/// the reviewer needs the contract fact first, and the file rename is already
/// visible in the diff header. `Moved` is therefore reserved for a symbol that
/// travelled unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeKind {
    /// The signature differs between the two sides — the contract moved.
    SignatureChanged,
    /// The symbol does not exist on the base side.
    Added,
    /// The symbol existed on the base side and is gone.
    Removed,
    /// Same signature, same body, same docstring — the file was renamed.
    Moved {
        /// Base-side path the symbol travelled from.
        from: String,
    },
    /// Same signature, different body.
    BodyOnly,
    /// Same signature and body, different docstring.
    DocOnly,
}

impl ChangeKind {
    /// Whether this kind changes what callers see.
    ///
    /// `BodyOnly` and `DocOnly` do not: they are counted ("12 functions
    /// changed, only 3 changed their contract") but never listed.
    pub fn is_contract_change(&self) -> bool {
        !matches!(self, ChangeKind::BodyOnly | ChangeKind::DocOnly)
    }

    /// Stable lowercase label used by every renderer.
    pub fn label(&self) -> &'static str {
        match self {
            ChangeKind::SignatureChanged => "signature_changed",
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Moved { .. } => "moved",
            ChangeKind::BodyOnly => "body_only",
            ChangeKind::DocOnly => "doc_only",
        }
    }
}

/// One symbol's delta across the diff, plus the callers it left behind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChange {
    /// Symbol name.
    pub name: String,
    /// What sort of symbol it is — a function reads as `name()` in the
    /// headline, a class or module as a bare name.
    pub symbol_kind: NodeKind,
    /// Head-side path (base-side path for a deleted file).
    pub file: String,
    /// What happened.
    #[serde(flatten)]
    pub kind: ChangeKind,
    /// Signature on the base side, if the symbol existed there.
    pub sig_base: Option<String>,
    /// Signature on the head side, if the symbol still exists.
    pub sig_head: Option<String>,
    /// Content hash on the base side.
    pub hash_base: Option<String>,
    /// Content hash on the head side.
    pub hash_head: Option<String>,
    /// Whether the head side (or, for a removal, the base side) is exported.
    pub is_public: bool,
    /// Stored callers living in files this PR did **not** touch, capped for
    /// display at [`risk::MAX_DISPLAYED_CALLERS`].
    pub callers_outside_diff: Vec<CallerRef>,
    /// The uncapped count behind `callers_outside_diff`.
    pub callers_outside_diff_count: usize,
}

/// A changed file keel has no grammar for, named explicitly.
///
/// `keel compile` exits 0 with empty stdout on a `.sql` migration or a `.baml`
/// contract, which is indistinguishable from "checked and clean". A review that
/// silently omitted them would repeat that mistake at PR scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnanalyzedFile {
    /// Repo-relative path.
    pub path: String,
    /// Why it was not analyzed: `boundary`, `data`, `fixture`, `generated`, or
    /// `unparsed`.
    pub class: String,
}

/// The full `keel review` payload, rendered by the output formatters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub version: String,
    pub command: String,
    /// The base ref the working tree was compared against.
    pub base: String,
    /// Resolution tier of the two-sided parse — always Tier 1 (tree-sitter).
    pub resolution: String,
    /// Paths in the diff, analyzable or not.
    pub files_changed: usize,
    /// Paths keel parsed on at least one side.
    pub files_analyzed: usize,
    /// Symbols with any delta at all (contract, body, or doc).
    pub functions_touched: usize,
    /// Symbols whose contract moved — the headline number.
    pub contract_change_count: usize,
    /// Symbols whose body changed but whose contract held.
    pub body_only_count: usize,
    /// Symbols where only the docstring changed.
    pub doc_only_count: usize,
    /// Every touched symbol, ranked callers-outside-the-diff first.
    pub changes: Vec<ContractChange>,
    /// Changed files keel does not parse.
    pub unanalyzed: Vec<UnanalyzedFile>,
    /// Violations this diff introduced — head-side findings with every
    /// base-side finding subtracted. Restricted to
    /// [`baseline::DIFFABLE_CODES`]; E001/E004/E005 stay on the `keel compile`
    /// surface, where both sides are resolved at the same tier.
    pub new_violations: Vec<Violation>,
    /// Head-side findings that also existed on the base side, and are
    /// therefore this PR's inheritance rather than its doing.
    pub pre_existing_violations: usize,
}

/// Build the review of the working tree against `base`, evaluated in `dir`.
///
/// `store` is the live graph. The two-sided definition diff never consults it,
/// so a stale or freshly-mapped store changes the caller counts and the
/// baseline checks' shared inputs but never the contract facts. `enforce`
/// supplies the same check toggles and line budget `keel compile` uses, so the
/// baseline diff cannot report a code the repo turned off.
///
/// Errors only when git cannot resolve `base` — every other failure (an
/// unreadable blob, a file that vanished from the working tree) degrades to
/// "that side has no definitions", which is the correct reading.
pub fn review(
    store: &dyn GraphStore,
    dir: &Path,
    base: &str,
    enforce: &EnforceConfig,
) -> Result<ReviewResult, String> {
    let paths = gitdiff::changed_paths(dir, base)?;
    let scan = diff::scan_paths(dir, base, &paths);

    let baseline = baseline::diff(store, &scan, enforce);

    let mut changes = scan.changes;
    risk::attach_callers(store, &mut changes, &scan.diff_files);
    risk::rank(&mut changes);

    let contract_change_count = changes
        .iter()
        .filter(|c| c.kind.is_contract_change())
        .count();
    let body_only_count = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::BodyOnly)
        .count();
    let doc_only_count = changes
        .iter()
        .filter(|c| c.kind == ChangeKind::DocOnly)
        .count();

    Ok(ReviewResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "review".to_string(),
        base: base.to_string(),
        resolution: BLOB_RESOLUTION_TIER.to_string(),
        files_changed: paths.len(),
        files_analyzed: scan.files_analyzed,
        functions_touched: changes.len(),
        contract_change_count,
        body_only_count,
        doc_only_count,
        changes,
        unanalyzed: scan.unanalyzed,
        new_violations: baseline.new_violations,
        pre_existing_violations: baseline.pre_existing,
    })
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
