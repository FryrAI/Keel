use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::gitignore::Gitignore;
use ignore::Match;
use ignore::WalkBuilder;

use crate::monorepo::MonorepoLayout;
use crate::treesitter::detect_language;

pub struct WalkEntry {
    pub path: PathBuf,
    pub language: String,
    pub package: Option<String>,
}

pub struct FileWalker {
    root: PathBuf,
}

impl FileWalker {
    /// Creates a new file walker rooted at the given directory.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Walks the root directory and returns all recognized source files, respecting gitignore and `.keelignore`.
    pub fn walk(&self) -> Vec<WalkEntry> {
        let mut entries = Vec::new();

        let walker = WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .add_custom_ignore_filename(".keelignore")
            .build();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.into_path();
            if let Some(lang) = detect_language(&path) {
                entries.push(WalkEntry {
                    path,
                    language: lang.to_string(),
                    package: None,
                });
            }
        }

        entries
    }

    /// Walks files and annotates each with its monorepo package using longest-prefix match.
    pub fn walk_with_packages(&self, layout: &MonorepoLayout) -> Vec<WalkEntry> {
        let mut entries = self.walk();
        for entry in &mut entries {
            entry.package = find_package_for_path(&entry.path, layout);
        }
        entries
    }
}

/// The repository's ignore rules, applied to paths that did NOT come from a
/// directory walk — a `git diff --name-only` list, for instance.
///
/// [`FileWalker::walk`] gets `.keelignore` and `.gitignore` handling for free
/// from the `ignore` crate's walker, so the graph never contains an ignored
/// file. Commands whose file list comes from git need the same rules applied
/// after the fact, or they check files the map deliberately skipped (issue #70:
/// a vendored tree listed in `.keelignore` raised violations against
/// third-party source in the pre-commit hook; issue #90: the same for a
/// `.keelignore` nested in a package).
///
/// Every `.gitignore` and `.keelignore` from the root down to a path's parent
/// directory is consulted, each rooted at its own directory, with the walker's
/// precedence: a deeper directory's rules override a shallower one's, and
/// within one directory `.keelignore` outranks `.gitignore`. A file under an
/// excluded directory stays excluded whatever a deeper file says. Per-directory
/// matchers are built lazily on first use and cached, so the tree is never
/// walked. A missing ignore file simply contributes no patterns.
/// `.git/info/exclude`, the global gitignore and hidden-file rules are not
/// applied.
pub struct KeelIgnore {
    root: PathBuf,
    dirs: Mutex<HashMap<PathBuf, Arc<DirRules>>>,
}

/// The two ignore files of one directory, each rooted at that directory.
struct DirRules {
    keelignore: Gitignore,
    gitignore: Gitignore,
}

impl DirRules {
    fn load(dir: &Path) -> Self {
        // Unreadable or malformed ignore files contribute nothing rather than
        // failing: not ignoring a file is a false positive, refusing to run at
        // all is worse.
        Self {
            keelignore: Gitignore::new(dir.join(".keelignore")).0,
            gitignore: Gitignore::new(dir.join(".gitignore")).0,
        }
    }

    /// `.keelignore` first: it outranks `.gitignore` in the same directory.
    fn matched(&self, rel: &Path, is_dir: bool) -> Match<()> {
        self.keelignore
            .matched(rel, is_dir)
            .map(|_| ())
            .or(self.gitignore.matched(rel, is_dir).map(|_| ()))
    }
}

impl KeelIgnore {
    /// Creates a matcher for the ignore files under `root`; nothing is read
    /// until a path is checked.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            dirs: Mutex::new(HashMap::new()),
        }
    }

    /// The cached rules of `dir` (relative to the root; empty is the root).
    fn rules(&self, dir: &Path) -> Arc<DirRules> {
        let mut dirs = self.dirs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rules) = dirs.get(dir) {
            return Arc::clone(rules);
        }
        let rules = Arc::new(DirRules::load(&self.root.join(dir)));
        dirs.insert(dir.to_path_buf(), Arc::clone(&rules));
        rules
    }

    /// Whether the entry `rel` (root-relative) is excluded by the nearest
    /// directory, from its parent up to the root, that has an opinion on it.
    fn entry_ignored(&self, rel: &Path, is_dir: bool) -> bool {
        let mut dir = rel.parent().unwrap_or(Path::new(""));
        loop {
            let within = rel.strip_prefix(dir).unwrap_or(rel);
            match self.rules(dir).matched(within, is_dir) {
                Match::Ignore(()) => return true,
                Match::Whitelist(()) => return false,
                Match::None => {}
            }
            match dir.parent() {
                Some(up) if !dir.as_os_str().is_empty() => dir = up,
                _ => return false,
            }
        }
    }

    /// Whether `path` — relative to the root, or absolute beneath it — is
    /// excluded, either directly or through an ignored parent directory.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let relative = match path.strip_prefix(&self.root) {
            Ok(rel) => rel,
            // An absolute path outside the root is not governed by these rules;
            // anything else is already root-relative.
            Err(_) if path.is_absolute() => return false,
            Err(_) => path,
        };
        // Top-down, stopping at the first excluded ancestor: git never
        // re-includes a file under an excluded directory, and the walker
        // enforces it by never descending into one.
        let mut prefix = PathBuf::new();
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            prefix.push(component);
            let is_dir = components.peek().is_some();
            if self.entry_ignored(&prefix, is_dir) {
                return true;
            }
        }
        false
    }
}

/// Find which package a file belongs to using longest-prefix match.
fn find_package_for_path(file_path: &Path, layout: &MonorepoLayout) -> Option<String> {
    let mut best_match: Option<&str> = None;
    let mut best_len = 0;

    for pkg in &layout.packages {
        if file_path.starts_with(&pkg.path) {
            let pkg_len = pkg.path.as_os_str().len();
            if pkg_len > best_len {
                best_len = pkg_len;
                best_match = Some(&pkg.name);
            }
        }
    }

    best_match.map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monorepo::{MonorepoKind, PackageInfo};
    use std::fs;

    #[test]
    fn test_walker_finds_source_files() {
        let dir = std::env::temp_dir().join("keel_walker_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("src/lib.py"), "def f(): pass").unwrap();
        fs::write(dir.join("README.md"), "# Hello").unwrap();

        let walker = FileWalker::new(&dir);
        let entries = walker.walk();

        assert_eq!(entries.len(), 2);
        let langs: Vec<_> = entries.iter().map(|e| e.language.as_str()).collect();
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_walker_respects_keelignore() {
        let dir = std::env::temp_dir().join("keel_walker_ignore_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("vendor")).unwrap();
        fs::write(dir.join("src/app.ts"), "export {}").unwrap();
        fs::write(dir.join("vendor/lib.ts"), "export {}").unwrap();
        fs::write(dir.join(".keelignore"), "vendor/\n").unwrap();

        let walker = FileWalker::new(&dir);
        let entries = walker.walk();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.to_str().unwrap().contains("app.ts"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// The matcher must agree with the walker: a `.keelignore` directory
    /// pattern excludes everything beneath it, at any depth, and nothing else.
    #[test]
    fn test_keelignore_matcher_excludes_ignored_subtrees() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".keelignore"), "vendor/\n").unwrap();

        let ignore = KeelIgnore::new(root);
        assert!(ignore.is_ignored(Path::new("vendor/lib.ts")));
        assert!(ignore.is_ignored(Path::new("vendor/deep/x.rs")));
        assert!(!ignore.is_ignored(Path::new("src/app.ts")));
        // Absolute paths beneath the root resolve the same way.
        assert!(ignore.is_ignored(&root.join("vendor/lib.ts")));
        assert!(!ignore.is_ignored(&root.join("src/app.ts")));
    }

    /// `.gitignore` counts too — the walker honors both files, so a git-derived
    /// file list must not check something the map skipped for gitignore alone.
    #[test]
    fn test_keelignore_matcher_honors_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "generated/\n").unwrap();

        let ignore = KeelIgnore::new(root);
        assert!(ignore.is_ignored(Path::new("generated/api.ts")));
        assert!(!ignore.is_ignored(Path::new("src/app.ts")));
    }

    /// `.keelignore` outranks `.gitignore`, exactly as the walker's custom
    /// ignore file outranks `.gitignore` — a negation in `.keelignore` must be
    /// able to re-include a gitignored file, which the reverse order got wrong.
    #[test]
    fn test_keelignore_matcher_lets_keelignore_override_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "generated.rs\n").unwrap();
        fs::write(root.join(".keelignore"), "!generated.rs\n").unwrap();
        fs::write(root.join("generated.rs"), "fn g() {}").unwrap();

        assert!(!KeelIgnore::new(root).is_ignored(Path::new("generated.rs")));
        let walked = FileWalker::new(root).walk();
        assert_eq!(
            walked.len(),
            1,
            "the matcher must agree with the walker, which visits the file"
        );
    }

    /// A negation cannot climb out of an excluded directory: git never
    /// re-includes a file under an ignored parent, and the walker prunes the
    /// directory without ever seeing the child.
    #[test]
    fn test_keelignore_matcher_ignores_children_of_ignored_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("vendor")).unwrap();
        fs::write(root.join(".keelignore"), "vendor/\n!vendor/keep.rs\n").unwrap();
        fs::write(root.join("vendor/keep.rs"), "fn k() {}").unwrap();
        fs::write(root.join("app.rs"), "fn a() {}").unwrap();

        assert!(KeelIgnore::new(root).is_ignored(Path::new("vendor/keep.rs")));
        let walked = FileWalker::new(root).walk();
        assert_eq!(walked.len(), 1, "the walker never descends into vendor/");
        assert!(walked[0].path.ends_with("app.rs"));
    }

    /// No ignore files at all: nothing is ignored (and the matcher is empty, so
    /// the check short-circuits).
    #[test]
    fn test_keelignore_matcher_without_ignore_files_ignores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ignore = KeelIgnore::new(dir.path());
        assert!(!ignore.is_ignored(Path::new("vendor/lib.ts")));
        assert!(!ignore.is_ignored(Path::new("src/app.ts")));
    }

    #[test]
    fn test_walk_with_packages_annotates_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create package dirs with source files
        fs::create_dir_all(root.join("packages/web/src")).unwrap();
        fs::create_dir_all(root.join("packages/api/src")).unwrap();
        fs::write(root.join("packages/web/src/app.ts"), "export {}").unwrap();
        fs::write(root.join("packages/api/src/main.ts"), "export {}").unwrap();
        fs::write(root.join("root.ts"), "export {}").unwrap();

        let layout = MonorepoLayout {
            kind: MonorepoKind::NpmWorkspaces,
            packages: vec![
                PackageInfo {
                    name: "web".to_string(),
                    path: root.join("packages/web"),
                    kind: MonorepoKind::NpmWorkspaces,
                    language: "typescript".to_string(),
                },
                PackageInfo {
                    name: "api".to_string(),
                    path: root.join("packages/api"),
                    kind: MonorepoKind::NpmWorkspaces,
                    language: "typescript".to_string(),
                },
            ],
        };

        let walker = FileWalker::new(root);
        let entries = walker.walk_with_packages(&layout);

        // Find the web and api entries
        let web_entry = entries
            .iter()
            .find(|e| e.path.to_str().unwrap().contains("packages/web"));
        let api_entry = entries
            .iter()
            .find(|e| e.path.to_str().unwrap().contains("packages/api"));
        let root_entry = entries
            .iter()
            .find(|e| e.path.file_name().and_then(|n| n.to_str()) == Some("root.ts"));

        assert_eq!(web_entry.unwrap().package.as_deref(), Some("web"));
        assert_eq!(api_entry.unwrap().package.as_deref(), Some("api"));
        assert_eq!(root_entry.unwrap().package, None);
    }
}

#[cfg(test)]
#[path = "walker_parity_tests.rs"]
mod parity_tests;
