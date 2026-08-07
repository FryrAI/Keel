//! Unified file watcher for `keel serve --watch` and `keel watch`.
//!
//! One module, one behavior. A `notify` watcher with a shared ignore list and
//! the canonical language table as the extension gate, debounced so bursty
//! editors collapse into a single batch. `Create`/`Modify`/`Rename` become
//! recompiles; `Remove` (and rename-away, where the old path no longer exists)
//! prunes that file's nodes and edges from the shared graph so deleted files
//! stop accreting between full `keel map` runs.
//!
//! Atomic-save editors write a temp file then rename it over the target — that
//! surfaces as `Modify(Name)`/`Create` on a path that *does* exist, so it is
//! caught as a change; the vanished temp/old path (which does *not* exist) is
//! treated as a removal. Existence at flush time, not the raw event kind,
//! decides change-vs-remove.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::http::SharedEngine;
use crate::parse_shared::FileParser;

/// Directories to ignore when watching for file changes.
///
/// `.svelte-kit` is load-bearing: SvelteKit regenerates `.ts` files there on
/// every build, and watching them causes a recompile loop.
const IGNORED_DIRS: &[&str] = &[
    ".keel",
    ".svelte-kit",
    ".git",
    "node_modules",
    "__pycache__",
    "target",
    "dist",
    "build",
    ".next",
];

/// Debounce window: collect events for this long, then flush one batch.
const DEBOUNCE_MS: u64 = 200;

/// What happened to a watched path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// File was created or modified — recompile it.
    Changed,
    /// File was deleted (or renamed away) — prune it from the graph.
    Removed,
}

/// A debounced set of file changes, split by how each must be handled.
#[derive(Debug, Default, Clone)]
pub struct WatchBatch {
    /// Files to recompile.
    pub changed: Vec<PathBuf>,
    /// Files to prune from the graph.
    pub removed: Vec<PathBuf>,
}

impl WatchBatch {
    /// True when there is nothing to compile and nothing to prune.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Result of applying a [`WatchBatch`] to the graph.
#[derive(Debug, Default, Clone, Copy)]
pub struct BatchOutcome {
    /// Nodes removed while pruning deleted files.
    pub pruned: usize,
    /// Files recompiled.
    pub compiled: usize,
    /// Errors reported by the recompile.
    pub errors: usize,
    /// Warnings reported by the recompile.
    pub warnings: usize,
}

/// Decide how one path changed, given its event kind and whether it currently
/// exists on disk.
///
/// Pure — the existence check is injected so this is unit-testable without
/// touching the filesystem. A `Remove` is always a removal; a `Create`/`Modify`
/// (which also covers renames, since `notify` reports `Modify(Name)`) is a
/// change if the path still exists and a removal otherwise (rename-away).
pub fn classify_kind(kind: &EventKind, exists: bool) -> Option<ChangeKind> {
    match kind {
        EventKind::Remove(_) => Some(ChangeKind::Removed),
        EventKind::Create(_) | EventKind::Modify(_) => Some(if exists {
            ChangeKind::Changed
        } else {
            ChangeKind::Removed
        }),
        _ => None,
    }
}

/// Classify every watched path in a raw notify event.
fn classify_event(root: &Path, event: &Event) -> Vec<(PathBuf, ChangeKind)> {
    event
        .paths
        .iter()
        .filter(|p| should_watch(root, p))
        .filter_map(|p| classify_kind(&event.kind, p.exists()).map(|ck| (p.clone(), ck)))
        .collect()
}

/// Check if a path should trigger a recompile or prune.
///
/// Canonical language table, so watch mode never disagrees with the parsers
/// about which files exist (e.g. `.svelte`, `.mts`). Only components *inside*
/// the repo may match the ignore list — a checkout living under e.g.
/// `~/build/` must not ignore every file it contains.
pub fn should_watch(root: &Path, path: &Path) -> bool {
    if keel_parsers::treesitter::detect_language(path).is_none() {
        return false;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    for component in relative.components() {
        if let std::path::Component::Normal(name) = component {
            if IGNORED_DIRS.contains(&name.to_str().unwrap_or("")) {
                return false;
            }
        }
    }
    true
}

/// Fold debounced per-path classifications into a [`WatchBatch`].
fn build_batch(pending: HashMap<PathBuf, ChangeKind>) -> WatchBatch {
    let mut batch = WatchBatch::default();
    for (path, kind) in pending {
        match kind {
            ChangeKind::Changed => batch.changed.push(path),
            ChangeKind::Removed => batch.removed.push(path),
        }
    }
    batch
}

/// Start watching `root` for file changes.
///
/// Returns the watcher handle (keep it alive — dropping it stops the watch) and
/// a channel that emits debounced [`WatchBatch`]es.
pub fn start_watching(
    root: &Path,
) -> Result<(RecommendedWatcher, mpsc::Receiver<WatchBatch>), notify::Error> {
    let (tx, rx) = mpsc::channel::<WatchBatch>(64);
    let (event_tx, mut event_rx) = mpsc::channel::<(PathBuf, ChangeKind)>(256);

    // Debounce: the latest classification per path wins (a modify-then-delete
    // inside one window ends up a removal), flushed once the window goes idle.
    tokio::spawn(async move {
        let debounce = Duration::from_millis(DEBOUNCE_MS);
        let mut pending: HashMap<PathBuf, ChangeKind> = HashMap::new();
        loop {
            match tokio::time::timeout(debounce, event_rx.recv()).await {
                Ok(Some((path, kind))) => {
                    pending.insert(path, kind);
                }
                Ok(None) => break, // channel closed
                Err(_) => {
                    if !pending.is_empty() {
                        let batch = build_batch(std::mem::take(&mut pending));
                        if tx.send(batch).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    let root_owned = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            for (path, kind) in classify_event(&root_owned, &event) {
                let _ = event_tx.blocking_send((path, kind));
            }
        }
    })?;

    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok((watcher, rx))
}

/// Path as stored in the graph: relative to the project root. Delegates to the
/// canonical [`keel_core::paths::make_relative`] so every crate strips repo-root
/// prefixes identically.
fn relative_path(root: &Path, path: &Path) -> String {
    keel_core::paths::make_relative(root, path)
}

/// Apply a batch to the shared graph: prune deleted files, recompile changed
/// ones. Shared by `keel serve --watch` and `keel watch`.
pub fn apply_batch(engine: &SharedEngine, root: &Path, batch: &WatchBatch) -> BatchOutcome {
    let mut outcome = BatchOutcome::default();

    // Prune deleted files so their nodes/edges stop accreting in the graph.
    if !batch.removed.is_empty() {
        if let Ok(mut engine) = engine.lock() {
            for path in &batch.removed {
                if let Ok(n) = engine.prune_file(&relative_path(root, path)) {
                    outcome.pruned += n;
                }
            }
        }
    }

    // Recompile changed files incrementally (parse relative to root, matching
    // how the graph stores paths).
    if !batch.changed.is_empty() {
        let mut parser = FileParser::new();
        let indices: Vec<_> = batch
            .changed
            .iter()
            .map(|p| relative_path(root, p))
            .filter_map(|rel| parser.parse(&rel))
            .collect();
        if !indices.is_empty() {
            outcome.compiled = indices.len();
            if let Ok(mut engine) = engine.lock() {
                let result = engine.compile(&indices);
                outcome.errors = result.errors.len();
                outcome.warnings = result.warnings.len();
            }
        }
    }

    outcome
}

/// Log one batch's outcome to stderr. Silent when nothing was applied.
fn log_outcome(batch: &WatchBatch, outcome: &BatchOutcome) {
    let mut parts = Vec::new();
    if outcome.compiled > 0 {
        parts.push(format!("{} recompiled", outcome.compiled));
    }
    if outcome.pruned > 0 {
        parts.push(format!(
            "{} node(s) pruned from {} deleted file(s)",
            outcome.pruned,
            batch.removed.len()
        ));
    }
    if parts.is_empty() {
        return;
    }
    let status = if outcome.errors > 0 || outcome.warnings > 0 {
        format!(
            " — {} error(s), {} warning(s)",
            outcome.errors, outcome.warnings
        )
    } else {
        String::new()
    };
    eprintln!("[keel watch] {}{}", parts.join(", "), status);
}

/// Run the watch loop until the watcher is dropped or the process exits.
///
/// The single entry point both `keel serve --watch` (spawned as a task) and
/// `keel watch` (blocked on) share.
pub async fn watch(
    engine: SharedEngine,
    root: PathBuf,
    verbose: bool,
) -> Result<(), notify::Error> {
    // `_watcher` must outlive the loop — dropping it ends the watch.
    let (_watcher, mut rx) = start_watching(&root)?;
    if verbose {
        eprintln!("[keel watch] watching {}", root.display());
    }
    while let Some(batch) = rx.recv().await {
        let outcome = apply_batch(&engine, &root, &batch);
        log_outcome(&batch, &outcome);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};

    #[test]
    fn should_watch_accepts_source_files() {
        let root = PathBuf::from("/project");
        assert!(should_watch(&root, &PathBuf::from("/project/src/foo.ts")));
        assert!(should_watch(&root, &PathBuf::from("/project/lib/bar.py")));
        assert!(should_watch(&root, &PathBuf::from("/project/main.rs")));
    }

    #[test]
    fn should_watch_rejects_non_source_and_ignored_dirs() {
        let root = PathBuf::from("/project");
        assert!(!should_watch(&root, &PathBuf::from("/project/src/foo.md")));
        assert!(!should_watch(&root, &PathBuf::from("/project/img.png")));
        assert!(!should_watch(
            &root,
            &PathBuf::from("/project/node_modules/foo.ts")
        ));
        assert!(!should_watch(
            &root,
            &PathBuf::from("/project/target/debug/main.rs")
        ));
    }

    #[test]
    fn classify_remove_is_removed() {
        let kind = EventKind::Remove(RemoveKind::File);
        assert_eq!(classify_kind(&kind, false), Some(ChangeKind::Removed));
        // Even if some later event recreated the path, a Remove kind prunes.
        assert_eq!(classify_kind(&kind, true), Some(ChangeKind::Removed));
    }

    #[test]
    fn classify_create_and_modify_depend_on_existence() {
        let create = EventKind::Create(CreateKind::File);
        let modify = EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content));
        assert_eq!(classify_kind(&create, true), Some(ChangeKind::Changed));
        assert_eq!(classify_kind(&modify, true), Some(ChangeKind::Changed));
        // Modify on a path that no longer exists = rename-away = removal.
        assert_eq!(classify_kind(&modify, false), Some(ChangeKind::Removed));
    }

    #[test]
    fn classify_rename_to_existing_target_is_change() {
        // Atomic save: temp is renamed over the real file, which now exists.
        let rename = EventKind::Modify(ModifyKind::Name(RenameMode::To));
        assert_eq!(classify_kind(&rename, true), Some(ChangeKind::Changed));
    }

    #[test]
    fn classify_ignores_access_events() {
        let access = EventKind::Access(notify::event::AccessKind::Read);
        assert_eq!(classify_kind(&access, true), None);
    }

    #[test]
    fn build_batch_splits_by_kind() {
        let mut pending = HashMap::new();
        pending.insert(PathBuf::from("/p/a.rs"), ChangeKind::Changed);
        pending.insert(PathBuf::from("/p/b.rs"), ChangeKind::Removed);
        let batch = build_batch(pending);
        assert_eq!(batch.changed, vec![PathBuf::from("/p/a.rs")]);
        assert_eq!(batch.removed, vec![PathBuf::from("/p/b.rs")]);
        assert!(!batch.is_empty());
    }

    #[test]
    fn apply_batch_prunes_removed_file_from_graph() {
        use std::sync::{Arc, Mutex};

        use keel_core::sqlite::SqliteGraphStore;
        use keel_core::types::{GraphNode, NodeKind};
        use keel_enforce::engine::EnforcementEngine;

        fn node(id: u64, name: &str, file: &str) -> GraphNode {
            GraphNode {
                complexity: 0,
                id,
                hash: format!("h{id}"),
                kind: NodeKind::Function,
                name: name.into(),
                signature: format!("fn {name}()"),
                file_path: file.into(),
                line_start: 1,
                line_end: 3,
                docstring: None,
                is_public: false,
                type_hints_present: true,
                has_docstring: false,
                is_associated: false,
                external_endpoints: vec![],
                previous_hashes: vec![],
                module_id: 0,
                package: None,
            }
        }

        let root = PathBuf::from("/project");
        // Seed the graph directly (compile validates; `keel map` creates nodes).
        let store = SqliteGraphStore::in_memory().unwrap();
        store.insert_node(&node(1, "foo", "src/gone.rs")).unwrap();
        store.insert_node(&node(2, "keep", "src/keep.rs")).unwrap();
        let engine: SharedEngine = Arc::new(Mutex::new(EnforcementEngine::new(Box::new(store))));

        // A deletion arrives as an absolute path (as notify emits); apply_batch
        // makes it relative and prunes it from the graph.
        let batch = WatchBatch {
            changed: vec![],
            removed: vec![root.join("src/gone.rs")],
        };
        let outcome = apply_batch(&engine, &root, &batch);
        assert_eq!(outcome.pruned, 1, "the deleted file's node is pruned");

        // Re-pruning the same file finds nothing left; the other file's node,
        // never named in a batch, is untouched.
        let outcome2 = apply_batch(&engine, &root, &batch);
        assert_eq!(outcome2.pruned, 0);
        let keep_batch = WatchBatch {
            changed: vec![],
            removed: vec![root.join("src/keep.rs")],
        };
        assert_eq!(apply_batch(&engine, &root, &keep_batch).pruned, 1);
    }

    #[test]
    fn classify_event_filters_and_splits() {
        let root = PathBuf::from("/project");
        // A watched source file that no longer exists -> removed.
        let event = Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec![
                PathBuf::from("/project/src/deleted.rs"),
                PathBuf::from("/project/node_modules/ignored.ts"),
                PathBuf::from("/project/notes.md"),
            ],
            attrs: Default::default(),
        };
        let classified = classify_event(&root, &event);
        assert_eq!(classified.len(), 1, "only the watched source file survives");
        assert_eq!(classified[0].0, PathBuf::from("/project/src/deleted.rs"));
        assert_eq!(classified[0].1, ChangeKind::Removed);
    }
}
