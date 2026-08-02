//! The staleness guard: refuse to enforce against a graph built somewhere
//! else in history.
//!
//! `keel compile --changed` reads the stored graph for everything it cannot
//! see in the file it was handed — who calls this function, which functions
//! existed before the edit. When the graph was built from a commit this
//! checkout does not contain (a poisoned CI cache, a rebase, a branch switch,
//! a `git reset --hard`), those answers describe other code: renamed functions
//! read as removed (`E004`), live callers read as broken (`E001`). A stale
//! graph is strictly worse than no graph, because no graph fails obviously.
//!
//! So it fails obviously. `compile` exits 2 — keel's internal-error code, not
//! a violation — and names the fix.
//!
//! The guard is deliberately silent in every case where it cannot be certain:
//! a graph with no `last_map_commit` marker (mapped by a pre-0.5 keel, or
//! mapped outside a git repo), a checkout with no git, or a marker naming an
//! object this clone does not have. Those must keep working exactly as before.

use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_boundary::LAST_MAP_COMMIT;
use keel_core::store::GraphStore;
use keel_enforce::gitdiff::{self, Ancestry};

/// The diagnostic `keel compile` prints before exiting 2, or `None` when the
/// stored graph is usable (or its provenance cannot be established).
pub(crate) fn stale_graph_message(cwd: &Path, store: &SqliteGraphStore) -> Option<String> {
    let recorded = store.meta_value(LAST_MAP_COMMIT)?;
    match gitdiff::is_ancestor(cwd, &recorded, "HEAD") {
        Ancestry::NotAncestor => Some(stale_message(&recorded)),
        Ancestry::Ancestor | Ancestry::Unknown => None,
    }
}

/// Wording for a graph whose commit is not in `HEAD`'s history.
///
/// Says what is wrong, why the output would have been untrustworthy, and the
/// one command that fixes it — the same shape as a violation's `fix_hint`.
fn stale_message(commit: &str) -> String {
    let short: String = commit.chars().take(12).collect();
    format!(
        "keel compile: the stored graph was built at commit {short}, which is not an ancestor \
         of HEAD — it describes code this checkout does not contain, so its callers and \
         removals would be phantom. Run `keel map` to rebuild the graph."
    )
}

#[cfg(test)]
#[path = "graph_staleness_tests.rs"]
mod tests;
