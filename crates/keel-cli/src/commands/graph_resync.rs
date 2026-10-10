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

use std::collections::BTreeSet;
use std::path::Path;

use keel_core::config::KeelConfig;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_meta::{LAST_MAP_COMMIT, RESYNC_PENDING};
use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeChange};
use keel_enforce::gitdiff;
use keel_parsers::treesitter::detect_language;

use super::compile_parse::CompileParser;
use super::compile_sync::{compiled_file_changes, write_compiled_changes, SyncSource};

/// The stable prefix of every "cannot check" refusal line.
pub(crate) const NO_USABLE_GRAPH: &str = "keel compile: no usable graph — ";
/// Under load: about 1.2 s fixed + 0.09 s/file, within the hook's 5 s budget.
pub(crate) const RESYNC_INLINE_MAX: usize = 15;
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
    let reason = reason.replace(['\r', '\n'], " ");
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
    let pending = store.meta_value(RESYNC_PENDING).and_then(|saved| {
        serde_json::from_str::<(String, BTreeSet<String>, BTreeSet<String>)>(&saved).ok()
    });
    let (mut changed, mut dependents) = match pending {
        Some((baseline, paths, dependents)) if baseline == stored => (paths, dependents),
        _ => (BTreeSet::new(), BTreeSet::new()),
    };
    if stored == head && changed.is_empty() && dependents.is_empty() {
        return Ok(());
    }
    let base = gitdiff::resolve_commit(root, &stored).map_err(|_| gone_message(&stored))?;
    let fail = |reason: String| failed_message(&stored, &reason);

    let known = |path: &str| detect_language(Path::new(path)).is_some();
    let mut paths = gitdiff::changed_paths_between(root, &base, &head).map_err(fail)?;
    paths.retain(|c| known(&c.path) || c.base_path().is_some_and(known));
    let check_count = |count| {
        if count > RESYNC_MAX_FILES {
            Err(too_many_message(&stored, count, "`keel map` to rebuild it"))
        } else if explicit_targets && count > RESYNC_INLINE_MAX {
            Err(too_many_message(
                &stored,
                count,
                "`keel compile` to bring it up to date",
            ))
        } else {
            Ok(())
        }
    };
    changed.extend(
        paths
            .iter()
            .flat_map(|c| std::iter::once(c.path.clone()).chain(c.base_path().map(str::to_owned))),
    );
    dependents.retain(|p| !changed.contains(p));
    check_count(changed.len() + dependents.len())?;
    let save_pending = |store: &SqliteGraphStore, dependents: &BTreeSet<String>| {
        store
            .set_meta_value(
                RESYNC_PENDING,
                &serde_json::to_string(&(&stored, &changed, dependents))
                    .expect("strings serialize"),
            )
            .map_err(|e| fail(e.to_string()))
    };

    let mut gone_paths = BTreeSet::new();
    // Everything present at HEAD is rebuilt from HEAD's blob.
    let literal_keys = super::map_boundary::persisted_literal_keys(root, store);
    let mut parser = CompileParser::new(root, literal_keys);
    let mut files = Vec::new();
    for path in &changed {
        match gitdiff::source_at(root, &head, path).map_err(fail)? {
            Some(text) => files.extend(parser.parse(&root.join(path), &text)),
            None => {
                gone_paths.insert(path.clone());
            }
        }
    }
    for path in &gone_paths {
        collect_dependents(
            store,
            store.get_nodes_in_file(path).iter().map(|n| n.id),
            &changed,
            &mut dependents,
        );
    }
    check_count(changed.len() + dependents.len())?;
    // Record every touched path before the first graph write, even without callers.
    save_pending(store, &dependents)?;
    for path in gone_paths {
        store
            .remove_body_rows_in_file(&path)
            .map_err(|e| fail(e.to_string()))?;
        keel_enforce::engine_prune::prune_file(store, &path).map_err(|e| fail(e.to_string()))?;
    }
    // Pass 1 adds the nodes of new definitions; an edge to one of them from a
    // file synced earlier in the same pass cannot resolve until the node is
    // stored, so pass 2 re-resolves the same files. A re-sync that adds no node
    // costs one pass.
    if !files.is_empty() {
        let resolvers = parser.resolver_set();
        for _ in 0..2 {
            let changes = compiled_file_changes(
                store,
                root,
                &files,
                &resolvers,
                config.monorepo.enabled,
                SyncSource::Committed,
            );
            collect_dependents(
                store,
                changes.node_changes.iter().filter_map(|c| match c {
                    NodeChange::Remove(id) => Some(*id),
                    _ => None,
                }),
                &changed,
                &mut dependents,
            );
            check_count(changed.len() + dependents.len())?;
            save_pending(store, &dependents)?;
            for change in &changes.node_changes {
                if let NodeChange::Remove(id) = change {
                    if let Some(node) = store.get_node_by_id(*id) {
                        store
                            .remove_body_row(&node.hash, &node.file_path)
                            .map_err(|e| fail(e.to_string()))?;
                    }
                }
            }
            let nodes_before = store.max_node_id();
            write_compiled_changes(store, changes, false).map_err(fail)?;
            if store.max_node_id() <= nodes_before {
                break;
            }
        }
    }
    let mut dependent_files = Vec::new();
    for path in &dependents {
        if let Some(text) = gitdiff::source_at(root, &head, path).map_err(fail)? {
            dependent_files.extend(parser.parse(&root.join(path), &text));
        }
    }
    if !dependent_files.is_empty() {
        let changes = compiled_file_changes(
            store,
            root,
            &dependent_files,
            &parser.resolver_set(),
            config.monorepo.enabled,
            SyncSource::ResolveReferences,
        );
        write_compiled_changes(store, changes, false).map_err(fail)?;
    }
    store
        .complete_resync(&head)
        .map_err(|e| fail(e.to_string()))?;
    if verbose {
        let count = changed.len() + dependents.len();
        eprintln!(
            "keel compile: re-synced {count} file(s) from {} to {}",
            short(&stored),
            short(&head)
        );
    }
    Ok(())
}

fn collect_dependents(
    store: &SqliteGraphStore,
    removed: impl Iterator<Item = u64>,
    changed: &BTreeSet<String>,
    dependents: &mut BTreeSet<String>,
) {
    for id in removed {
        for edge in store.get_edges(id, EdgeDirection::Incoming) {
            if matches!(edge.kind, EdgeKind::Calls | EdgeKind::Uses) {
                if let Some(caller) = store.get_node_by_id(edge.source_id) {
                    if !changed.contains(&caller.file_path) {
                        dependents.insert(caller.file_path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "graph_resync_tests.rs"]
mod tests;
