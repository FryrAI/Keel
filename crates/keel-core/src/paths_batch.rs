//! Caller-owned memo of canonical parents for one map batch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{canonicalize_portable, graph_spelling, normalize_lexically};

/// Normalize graph paths while reusing canonical parents within one batch.
/// The caller must discard this memo before changing directory or symlink topology.
pub struct ProjectPathBatch {
    root: PathBuf,
    parents: HashMap<PathBuf, PathBuf>,
}

impl ProjectPathBatch {
    /// Resolve the root once and start an empty, caller-owned parent memo.
    pub fn new(root: &Path) -> Self {
        Self {
            root: canonicalize_portable(root).unwrap_or_else(|_| normalize_lexically(root)),
            parents: HashMap::new(),
        }
    }

    /// Render a batch path with the same identity and outside-root fallback as `make_relative`.
    pub fn make_relative(&mut self, path: &Path) -> String {
        relative(&self.root, path, Some(&mut self.parents))
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    }
}

/// Normalize one path with an optional caller-owned memo of canonical parents.
pub(super) fn relative(
    root: &Path,
    path: &Path,
    mut parents: Option<&mut HashMap<PathBuf, PathBuf>>,
) -> Option<String> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    // A directory alias has no source-file leaf whose identity needs preserving.
    if joined.is_dir() {
        let directory = canonicalize_portable(&joined).ok()?;
        return Some(graph_spelling(directory.strip_prefix(root).ok()?));
    }
    let parent = joined.parent()?;
    let mut normalized = parents.as_ref().and_then(|memo| memo.get(parent)).cloned();
    if normalized.is_none() {
        let mut existing = parent;
        let mut missing = Vec::new();
        while !existing.exists() {
            missing.push(existing.components().next_back()?.as_os_str());
            existing = existing.parent()?;
        }
        let mut canonical = canonicalize_portable(existing).ok()?;
        for component in missing.into_iter().rev() {
            canonical.push(component);
        }
        if let Some(memo) = parents.as_mut() {
            memo.insert(parent.to_path_buf(), canonical.clone());
        }
        normalized = Some(canonical);
    }
    let mut normalized = normalized?;
    normalized.push(joined.components().next_back()?.as_os_str());
    let normalized = normalize_lexically(&normalized);
    Some(graph_spelling(normalized.strip_prefix(root).ok()?))
}
