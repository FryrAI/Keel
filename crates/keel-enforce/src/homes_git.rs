//! Git-base context for CLI compile and CLI/MCP review expression homes.
//! Server compile intentionally never constructs this context.

use std::path::{Path, PathBuf};

use keel_core::config::KeelConfig;

use crate::gitdiff;
use crate::types::Violation;
use crate::violations_homes::HomeScanner;

/// An invocation's prepared rules and lazily resolved immutable Git base.
pub struct GitHomes {
    scanner: HomeScanner,
    root: PathBuf,
    base_ref: String,
    commit: Option<Result<String, String>>,
    enabled: bool,
    verbose: bool,
}

impl GitHomes {
    /// Prepare a context rooted at the current worktree, independently of graph paths.
    pub fn new(dir: &Path, base: &str, config: &KeelConfig, verbose: bool) -> Self {
        Self {
            scanner: HomeScanner::new(&config.homes, config.enforce.homes),
            root: keel_core::paths::worktree_root(dir).unwrap_or_else(|| dir.to_path_buf()),
            base_ref: base.into(),
            commit: None,
            enabled: !config.homes.is_empty(),
            verbose,
        }
    }

    /// Check already-read source against the base path (which may differ after a rename).
    /// A missing base path is empty; an unavailable or non-UTF-8 blob skips only this file.
    pub fn check(&mut self, file: &Path, text: &str, base_path: Option<&str>) -> Vec<Violation> {
        if !self.enabled {
            return vec![];
        }
        let Some(path) = self.relative_path(file) else {
            return vec![];
        };
        let head = self.scanner.scan(&path, text);
        if head.is_empty() {
            return vec![];
        }
        if self.commit.is_none() {
            let commit = gitdiff::resolve_commit(&self.root, &self.base_ref);
            if let Err(e) = &commit {
                if self.verbose {
                    eprintln!("keel: homes skipped: {e}");
                }
            }
            self.commit = Some(commit);
        }
        let Ok(commit) = self.commit.as_ref().expect("resolved base") else {
            return vec![];
        };
        let base_path = base_path.unwrap_or(&path);
        let base_text = match gitdiff::blob_at_checked(&self.root, commit, base_path) {
            Ok(Some(text)) => text,
            Ok(None) => String::new(),
            Err(e) => {
                if self.verbose {
                    eprintln!("keel: homes skipped for {path}: {e}");
                }
                return vec![];
            }
        };
        let base = self.scanner.scan(base_path, &base_text);
        self.scanner.introduced(&path, &head, &base)
    }

    /// Normalize a source path relative to this worktree, preserving the leaf's spelling.
    pub fn relative_path(&self, file: &Path) -> Option<String> {
        // Normalize parent components without following a source symlink into its home.
        let file = file.parent()?.canonicalize().ok()?.join(file.file_name()?);
        let root = self.root.canonicalize().ok()?;
        let rel = file.strip_prefix(root).ok()?;
        Some(
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
}
