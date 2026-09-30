//! Git-base context for CLI compile and CLI/MCP review expression homes.
//! Server compile intentionally never constructs this context.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use keel_core::config::KeelConfig;

use crate::gitdiff;
use crate::types::Violation;
use crate::violations_homes::{HomeOccurrence, HomeScanner};

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
        let scanned = self.scan_many(files);
        self.scanner.introduced_many(&scanned)
    }

    fn scan_many(
        &mut self,
        files: &[(&Path, &str, Option<&str>)],
    ) -> Vec<(String, Vec<HomeOccurrence>, Vec<HomeOccurrence>)> {
        if !self.enabled {
            return vec![];
        }
        let mut seen = BTreeSet::new();
        let heads = files
            .iter()
            .filter_map(|(file, text, base_path)| {
                if file.is_symlink() {
                    return None;
                }
                let path = self.relative_path(file)?;
                if !seen.insert(path.clone()) {
                    return None;
                }
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
            if !self.scanner.eligible(base_path) {
                scanned.push((path, head, vec![]));
                continue;
            }
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
        scanned
    }

    /// Prepare graph-path findings and each file's own surplus identities before pooling.
    pub fn compile_findings(
        &mut self,
        cwd: &Path,
        sources: &BTreeMap<PathBuf, String>,
    ) -> HashMap<String, (String, Vec<Violation>, String)> {
        let inputs = sources
            .iter()
            .map(|(p, text)| (p.as_path(), text.as_str(), None))
            .collect::<Vec<_>>();
        let scanned = self.scan_many(&inputs);
        let findings = self.scanner.introduced_many(&scanned);
        let fingerprints = scanned
            .iter()
            .map(|(path, head, base)| {
                let own = self.scanner.introduced(path, head, base);
                let lines = own.iter().map(|v| v.line).collect::<BTreeSet<_>>();
                let identities = head
                    .iter()
                    .filter(|o| lines.contains(&o.line))
                    .map(|o| keel_core::hash::hash_string(&o.text))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join("\n");
                (path.as_str(), format!("homes-v2:\n{identities}"))
            })
            .collect::<HashMap<_, _>>();
        sources
            .keys()
            .filter_map(|path| {
                let scope = self.relative_path(path)?;
                let hits = findings
                    .iter()
                    .filter(|v| v.file == scope)
                    .cloned()
                    .collect::<Vec<_>>();
                let fingerprint = fingerprints
                    .get(scope.as_str())
                    .cloned()
                    .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::config::{HomeRule, HomeSeverity};

    #[test]
    fn homes_base_path_filter_excludes_home_out_of_scope_and_untracked_languages() {
        let scanner = HomeScanner::new(
            &[HomeRule {
                name: "civil-day".into(),
                patterns: vec!["CURRENT_DATE".into()],
                home: vec!["src/time.rs".into()],
                scope: vec!["src".into()],
            }],
            HomeSeverity::Warning,
        );
        assert!(scanner.eligible("src/query.rs"));
        assert!(scanner.eligible("src/query.sql"));
        // The filter is applied to the base path independently of the head path.
        for path in [
            "src/time.rs",
            "other/query.rs",
            "src/readme.md",
            "src/query.txt",
        ] {
            assert!(!scanner.eligible(path), "{path}");
        }
    }

    #[test]
    fn homes_ineligible_base_paths_do_not_read_blobs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let mut config = KeelConfig::default();
        config.homes.push(HomeRule {
            name: "civil-day".into(),
            patterns: vec!["CURRENT_DATE".into()],
            home: vec!["src/time.rs".into()],
            scope: vec!["src".into()],
        });
        let mut homes = GitHomes::new(dir.path(), "HEAD", &config, false);
        // An already-resolved base that cannot be read makes an accidental
        // blob fetch observable: eligible paths skip the file on read failure.
        homes.commit = Some(Ok("unavailable-base".into()));
        let file = dir.path().join("src/query.rs");
        for base in ["src/time.rs", "outside/query.rs", "src/readme.md"] {
            assert_eq!(
                homes.check(&file, "// CURRENT_DATE", Some(base)).len(),
                1,
                "{base}"
            );
        }
        assert!(homes
            .check(&file, "// CURRENT_DATE", Some("src/old.rs"))
            .is_empty());
    }
}
