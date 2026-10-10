//! Bring the stored graph to HEAD before a working-tree compile checks anything.
//!
//! `keel_meta.last_map_commit` names the commit whose tree the graph's nodes
//! describe, plus whatever working-tree edits were compiled since. When HEAD
//! moved off that commit (a branch checkout, a squash merge, a pull), the
//! graph is re-synced from HEAD's committed blobs — never the working tree, or
//! the edit about to be checked would be folded into its own baseline — and
//! the marker moves to HEAD only once every write has landed.
//!
//! Where the move cannot be bridged (the stored commit is gone, too many
//! files differ, a write failed) the compile refuses with exit 2 and one line
//! starting with [`NO_USABLE_GRAPH`]: enforcing against nodes from code this
//! checkout does not contain would report phantom removals and callers.

use std::path::Path;

use keel_core::config::KeelConfig;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_meta::LAST_MAP_COMMIT;
use keel_core::store::GraphStore;
use keel_enforce::gitdiff::{self, ChangeStatus, ChangedPath};
use keel_parsers::treesitter::detect_language;

use super::compile_parse::CompileParser;
use super::compile_sync::{sync_compiled_files, SyncSource};

/// The stable prefix of every "cannot check" refusal line.
pub(crate) const NO_USABLE_GRAPH: &str = "keel compile: no usable graph — ";
/// Most files an explicit-file compile re-syncs: it is the per-edit hook path,
/// which runs under a 5 s budget a killed re-sync would repeat on every edit.
pub(crate) const RESYNC_INLINE_MAX: usize = 30;
/// Most files a bare or `--changed` compile re-syncs; beyond it a full map is
/// comparable in time and better in fidelity.
pub(crate) const RESYNC_MAX_FILES: usize = 500;

fn short(commit: &str) -> String {
    commit.chars().take(12).collect()
}

fn gone_message(stored: &str) -> String {
    format!(
        "{NO_USABLE_GRAPH}the stored graph was built at commit {}, which this clone no longer \
         has. Run `keel map` to rebuild it; nothing was checked.",
        short(stored)
    )
}

fn too_many_message(stored: &str, count: usize, fix: &str) -> String {
    format!(
        "{NO_USABLE_GRAPH}the stored graph was built at commit {} and {count} source files \
         differ from HEAD. Run {fix}; nothing was checked.",
        short(stored)
    )
}

fn failed_message(stored: &str, reason: &str) -> String {
    format!(
        "{NO_USABLE_GRAPH}the stored graph was built at commit {} and could not be brought up \
         to date ({reason}). Run `keel map` to rebuild it; nothing was checked.",
        short(stored)
    )
}

/// Brings the stored graph to HEAD for a working-tree compile. `Err` is the
/// one-line refusal.
pub(crate) fn bring_to_head(
    root: &Path,
    store: &mut SqliteGraphStore,
    explicit_targets: bool,
    config: &KeelConfig,
    verbose: bool,
) -> Result<(), String> {
    let (Some(head), Some(stored)) = (
        gitdiff::head_commit(root),
        store.meta_value(LAST_MAP_COMMIT),
    ) else {
        return Ok(());
    };
    if stored == head {
        return Ok(());
    }
    let base = gitdiff::resolve_commit(root, &stored).map_err(|_| gone_message(&stored))?;
    let fail = |reason: String| failed_message(&stored, &reason);

    let known = |path: &str| detect_language(Path::new(path)).is_some();
    let mut paths = gitdiff::changed_paths_between(root, &base, &head).map_err(fail)?;
    paths.retain(|c| known(&c.path) || c.base_path().is_some_and(known));
    let count = paths.len();
    if count > RESYNC_MAX_FILES {
        return Err(too_many_message(&stored, count, "`keel map` to rebuild it"));
    }
    if explicit_targets && count > RESYNC_INLINE_MAX {
        return Err(too_many_message(
            &stored,
            count,
            "`keel compile` to bring it up to date",
        ));
    }

    // Removals first: a path that left the tree and came back under another
    // name is rebuilt below rather than pruned after it.
    for change in &paths {
        let gone = match &change.status {
            ChangeStatus::Deleted | ChangeStatus::RenamedToUnreadable => &change.path,
            ChangeStatus::Renamed { from } => from,
            ChangeStatus::Added | ChangeStatus::Modified => continue,
        };
        keel_enforce::engine_prune::prune_file(store, gone).map_err(|e| fail(e.to_string()))?;
    }

    // Everything present at HEAD is rebuilt from HEAD's blob.
    let literal_keys = super::map_boundary::persisted_literal_keys(root, store);
    let mut parser = CompileParser::new(root, literal_keys);
    let mut files = Vec::new();
    for change in paths.iter().filter(|c| present_at_head(c)) {
        match gitdiff::source_at(root, &head, &change.path).map_err(fail)? {
            Some(text) => files.extend(parser.parse(&root.join(&change.path), &text)),
            None => {
                keel_enforce::engine_prune::prune_file(store, &change.path)
                    .map_err(|e| fail(e.to_string()))?;
            }
        }
    }
    // Pass 1 adds the nodes of new definitions; an edge to one of them from a
    // file synced earlier in the same pass cannot resolve until the node is
    // stored, so pass 2 re-resolves the same files. A re-sync that adds no node
    // costs one pass.
    let resolvers = parser.resolver_set();
    if !files.is_empty() {
        let sync = |store: &mut SqliteGraphStore| {
            sync_compiled_files(
                store,
                root,
                &files,
                &resolvers,
                config.monorepo.enabled,
                SyncSource::Committed,
                verbose,
            )
        };
        let nodes_before = store.max_node_id();
        let mut done = sync(store);
        if done && store.max_node_id() > nodes_before {
            done = sync(store);
        }
        if !done {
            return Err(fail("graph write failed".into()));
        }
    }
    store
        .set_meta_value(LAST_MAP_COMMIT, &head)
        .map_err(|e| fail(e.to_string()))?;
    if verbose {
        eprintln!(
            "keel compile: re-synced {count} file(s) from {} to {}",
            short(&stored),
            short(&head)
        );
    }
    Ok(())
}

/// Whether the change leaves a file at `path` in HEAD's tree, and one keel parses.
fn present_at_head(change: &ChangedPath) -> bool {
    !matches!(
        change.status,
        ChangeStatus::Deleted | ChangeStatus::RenamedToUnreadable
    ) && detect_language(Path::new(&change.path)).is_some()
}

#[cfg(test)]
#[path = "graph_resync_tests.rs"]
mod tests;
