//! Git-base context for CLI compile and CLI/MCP review expression homes.
//! Server compile intentionally never constructs this context.

use std::collections::{BTreeMap, HashMap};
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
        self.check_many(&[(file, text, base_path)])
    }

    /// Compare every checked file together, including deletions with empty head text.
    /// Symlink paths are skipped; their target text is checked at the target's path.
    pub fn check_many(&mut self, files: &[(&Path, &str, Option<&str>)]) -> Vec<Violation> {
        if !self.enabled {
            return vec![];
        }
        let heads = files
            .iter()
            .filter_map(|(file, text, base_path)| {
                if file.is_symlink() {
                    return None;
                }
                let path = self.relative_path(file)?;
                let head = self.scanner.scan(&path, text);
                Some((path, head, *base_path))
            })
            .collect::<Vec<_>>();
        if heads.iter().all(|(_, head, _)| head.is_empty()) {
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
        let mut scanned = Vec::new();
        for (path, head, base_path) in heads {
            let base_path = base_path.unwrap_or(&path);
            let base_text = match gitdiff::blob_at_checked(&self.root, commit, base_path) {
                Ok(Some(text)) => text,
                Ok(None) => String::new(),
                Err(e) => {
                    if self.verbose {
                        eprintln!("keel: homes skipped for {path}: {e}");
                    }
                    continue;
                }
            };
            let base = self.scanner.scan(base_path, &base_text);
            scanned.push((path, head, base));
        }
        self.scanner.introduced_many(&scanned)
    }

    /// Prepare graph-path findings and one fingerprint of each file's reported lines.
    pub fn compile_findings(
        &mut self,
        cwd: &Path,
        sources: &BTreeMap<PathBuf, String>,
    ) -> HashMap<String, (String, Vec<Violation>, String)> {
        let inputs = sources
            .iter()
            .map(|(p, text)| (p.as_path(), text.as_str(), None))
            .collect::<Vec<_>>();
        let findings = self.check_many(&inputs);
        sources
            .iter()
            .filter_map(|(path, text)| {
                let scope = self.relative_path(path)?;
                let hits = findings
                    .iter()
                    .filter(|v| v.file == scope)
                    .cloned()
                    .collect::<Vec<_>>();
                let lines = hits
                    .iter()
                    .map(|v| v.line)
                    .collect::<std::collections::HashSet<_>>();
                let mut texts = text
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| lines.contains(&(*i as u32 + 1)))
                    .map(|(_, line)| line.split_whitespace().collect::<Vec<_>>().join(" "))
                    .collect::<Vec<_>>();
                texts.sort_unstable();
                let fingerprint = keel_core::hash::hash_string(&texts.join("\n"));
                Some((
                    keel_core::paths::make_relative(cwd, path),
                    (scope, hits, fingerprint),
                ))
            })
            .collect()
    }

    /// Normalize a source path relative to this worktree, preserving the leaf's spelling.
    pub fn relative_path(&self, file: &Path) -> Option<String> {
        // Normalize parent components without following a source symlink into its home.
        let mut parent = file.parent()?;
        let mut missing = Vec::new();
        while !parent.exists() {
            missing.push(parent.file_name()?);
            parent = parent.parent()?;
        }
        let mut file_path = parent.canonicalize().ok()?;
        for component in missing.into_iter().rev() {
            file_path.push(component);
        }
        let file = file_path.join(file.file_name()?);
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
