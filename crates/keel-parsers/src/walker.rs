use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use ignore::{DirEntry, WalkBuilder};

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

        for result in walk_builder(&self.root).build() {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !is_walked_file(&entry) {
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
            entry.package = package_for_path(&entry.path, layout);
        }
        entries
    }
}

/// The one walk configuration: `keel map` ([`FileWalker::walk`]) and the
/// git-list filter ([`KeelIgnore::ignored_paths`]) both build from it, so they
/// cannot drift.
fn walk_builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .add_custom_ignore_filename(".keelignore");
    builder
}

/// Whether a walk entry is something the walker indexes: a regular file (a
/// symlink is not followed and is never one).
fn is_walked_file(entry: &DirEntry) -> bool {
    entry.file_type().is_some_and(|ft| ft.is_file())
}

/// The repository's ignore rules, applied to paths that did NOT come from a
/// directory walk — a `git diff --name-only` list, for instance.
///
/// [`FileWalker::walk`] gets `.keelignore`, `.ignore` and `.gitignore` handling
/// for free from the `ignore` crate's walker, so the graph never contains an
/// ignored file. Commands whose file list comes from git need the same rules
/// applied after the fact, or they check files the map deliberately skipped
/// (issue #70: a vendored tree listed in `.keelignore` raised violations
/// against third-party source in the pre-commit hook; issue #90: the same for a
/// `.keelignore` nested in a package).
///
/// The rules are the walker's. Ignore files from the root down to a path's
/// parent directory all apply, each rooted at its own directory, and they rank
/// by KIND before depth: the nearest `.keelignore` opinion at any depth beats
/// every `.ignore`, which beats every `.gitignore`; within one kind the deepest
/// directory wins. `.gitignore` inheritance stops at the nearest directory that
/// holds a `.git` entry (a nested repository). A file under an excluded
/// directory stays excluded whatever a deeper file says. Per-directory state is
/// built lazily and cached, so the tree is never walked. A missing ignore file
/// contributes no patterns. A fourth kind, `<dir>/.git/info/exclude` (only for
/// a `.git` directory, or a linked worktree's shared one), ranks after `.gitignore` with its climb. The
/// walker's hidden rule is modelled too: an entry whose name starts with `.` is
/// ignored when no rule matched it (a `!` whitelist un-hides), but only for
/// paths `detect_language` recognises, since map never indexes the rest
/// whatever their location. The global gitignore is not applied, and an
/// explicitly named compile target is never filtered.
///
/// [`KeelIgnore::ignored_paths`] is the filter for HEAD-side paths. A source
/// path it is given gets the walker's own verdict: a walk built from the same
/// configuration as `keel map`, pruned to the listed paths' ancestor
/// directories, must yield it as a regular file, else it is ignored unless it
/// is gone (`NotFound`), when the hand model decides. A path through a symlinked
/// directory is judged as the file it resolves to inside the root, since that
/// is what map indexes.
/// Hidden entries, `.git/info/exclude`, ignore files above the root,
/// `require_git` and non-regular files follow by construction. Non-source
/// paths, and every BASE-side path (a deletion's path, a rename's source, which
/// must never be judged by what occupies the path now) go through the
/// hand-built kind-first model in [`KeelIgnore::is_ignored`], an approximation
/// of the walker.
///
/// Residuals: ignore files above the root and `require_git` are not modelled
/// for base-side paths; an external `GIT_DIR`/`GIT_WORK_TREE` can root git's
/// list elsewhere than keel's root; a path whose git spelling differs from its
/// on-disk spelling (case-insensitive or Unicode-normalising filesystems) is
/// not yielded by the walk and is therefore ignored (#98).
pub struct KeelIgnore {
    root: PathBuf,
    dirs: Mutex<HashMap<PathBuf, Arc<DirState>>>,
}

/// One directory's ignore files (each rooted at that directory), its place in
/// the chain to the root, and whether the directory itself is excluded.
struct DirState {
    /// Directory relative to the root; empty for the root.
    dir: PathBuf,
    keelignore: Gitignore,
    ignore: Gitignore,
    gitignore: Gitignore,
    /// `<dir>/.git/info/exclude`, rooted at this directory.
    exclude: Gitignore,
    /// Whether a `.git` entry sits here, ending `.gitignore` inheritance.
    has_git: bool,
    parent: Option<Arc<DirState>>,
    /// This directory is excluded by an ignore rule, directly or through an
    /// ancestor.
    excluded: bool,
    /// Like `excluded`, plus the walker's hidden rule (a `.`-named directory no
    /// rule spoke for): what map does to a recognised source file below it.
    excluded_hidden: bool,
}

/// The first non-`None` opinion on `rel` among one kind of ignore file, from
/// `start` (the entry's parent) upward; `stop_at_git` ends the climb after the
/// first directory holding a `.git` entry.
fn kind_match(
    start: &Arc<DirState>,
    rel: &Path,
    is_dir: bool,
    pick: fn(&DirState) -> &Gitignore,
    stop_at_git: bool,
) -> Match<()> {
    let mut state = Some(start);
    while let Some(s) = state {
        let matcher = pick(s);
        if !matcher.is_empty() {
            let within = rel.strip_prefix(&s.dir).unwrap_or(rel);
            let m = matcher.matched(within, is_dir).map(|_| ());
            if !m.is_none() {
                return m;
            }
        }
        if stop_at_git && s.has_git {
            break;
        }
        state = s.parent.as_ref();
    }
    Match::None
}

/// The merged opinion of the ignore-file kinds on the entry `rel` whose parent
/// directory is `parent`, in the walker's rank order.
fn entry_match(parent: &Arc<DirState>, rel: &Path, is_dir: bool) -> Match<()> {
    kind_match(parent, rel, is_dir, |s| &s.keelignore, false)
        .or(kind_match(parent, rel, is_dir, |s| &s.ignore, false))
        .or(kind_match(parent, rel, is_dir, |s| &s.gitignore, true))
        .or(kind_match(parent, rel, is_dir, |s| &s.exclude, true))
}

/// The `info/exclude` file the walker reads for the directory `dir` holding
/// `git_dir` (its `.git` entry): `<.git>/info/exclude` for a directory, and for
/// a linked worktree's `.git` FILE the shared one under the `commondir` of the
/// `gitdir:` it names (ignore 0.4 `resolve_git_commondir`). Anything missing or
/// malformed contributes nothing.
fn exclude_file(dir: &Path, git_dir: &Path) -> Option<PathBuf> {
    if git_dir.is_dir() {
        return Some(git_dir.join("info/exclude"));
    }
    if !git_dir.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(git_dir).ok()?;
    let real = dir.join(text.lines().next()?.strip_prefix("gitdir: ")?);
    let common = std::fs::read_to_string(real.join("commondir")).ok()?;
    let common = common.lines().next()?;
    // The walker joins only a `.`-leading line onto the gitdir.
    let common = if common.starts_with('.') {
        real.join(common)
    } else {
        PathBuf::from(common)
    };
    Some(common.join("info/exclude"))
}

/// The walker's hidden rule: it skips a `.`-named entry only when no ignore
/// rule matched it.
fn hidden_unmatched(rel: &Path, m: &Match<()>) -> bool {
    m.is_none()
        && rel
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
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

    /// The cached state of `dir` (relative to the root; empty is the root),
    /// building its ancestors first.
    fn state(&self, dir: &Path) -> Arc<DirState> {
        if let Some(state) = self.cached(dir) {
            return state;
        }
        let parent = dir
            .parent()
            .filter(|_| !dir.as_os_str().is_empty())
            .map(|up| self.state(up));
        let (excluded, excluded_hidden) = match parent.as_ref() {
            None => (false, false),
            Some(p) => {
                let m = entry_match(p, dir, true);
                let explicit = p.excluded || m.is_ignore();
                (
                    explicit,
                    explicit || p.excluded_hidden || hidden_unmatched(dir, &m),
                )
            }
        };
        let abs = self.root.join(dir);
        let git_dir = abs.join(".git");
        let exclude = match exclude_file(&abs, &git_dir) {
            Some(file) => {
                let mut builder = GitignoreBuilder::new(&abs);
                let _ = builder.add(file);
                builder.build().unwrap_or_else(|_| Gitignore::empty())
            }
            None => Gitignore::empty(),
        };
        // Unreadable or malformed ignore files contribute nothing rather than
        // failing: not ignoring a file is a false positive, refusing to run at
        // all is worse.
        let state = Arc::new(DirState {
            dir: dir.to_path_buf(),
            keelignore: Gitignore::new(abs.join(".keelignore")).0,
            ignore: Gitignore::new(abs.join(".ignore")).0,
            gitignore: Gitignore::new(abs.join(".gitignore")).0,
            exclude,
            has_git: git_dir.exists() || abs.join(".jj").exists(),
            parent,
            excluded,
            excluded_hidden,
        });
        let mut dirs = self.dirs.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(dirs.entry(dir.to_path_buf()).or_insert(state))
    }

    fn cached(&self, dir: &Path) -> Option<Arc<DirState>> {
        let dirs = self.dirs.lock().unwrap_or_else(|e| e.into_inner());
        dirs.get(dir).map(Arc::clone)
    }

    /// `path` relative to the root, or `None` for an absolute path outside it,
    /// which these rules do not govern.
    fn relative<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        match path.strip_prefix(&self.root) {
            Ok(rel) => Some(rel),
            Err(_) if path.is_absolute() => None,
            // Already root-relative.
            Err(_) => Some(path),
        }
    }

    /// The subset of HEAD-side `paths` (root-relative, or absolute beneath the
    /// root) the walker would not index. Source paths get the walker's own
    /// verdict, decided after the walk; everything else goes through the hand
    /// model of [`KeelIgnore::is_ignored`]. Base-side paths must NOT be passed
    /// here: call `is_ignored` for them. Paths outside the root are never
    /// reported.
    pub fn ignored_paths(&self, paths: &[PathBuf]) -> HashSet<PathBuf> {
        let mut ignored = HashSet::new();
        let mut sources = Vec::new();
        for path in paths {
            let Some(rel) = self.relative(path) else {
                continue;
            };
            if detect_language(rel).is_some() {
                sources.push((path, rel.to_path_buf()));
            } else if self.is_ignored(path) {
                ignored.insert(path.clone());
            }
        }
        if !sources.is_empty() {
            let canonical_root = self.root.canonicalize().ok();
            let mut parents: HashMap<PathBuf, PathBuf> = HashMap::new();
            // The path map indexes: a path through a symlinked directory is
            // the same file as its target inside the root.
            let sources: Vec<(&PathBuf, PathBuf, PathBuf)> = sources
                .into_iter()
                .map(|(path, rel)| {
                    let effective =
                        self.through_aliases(&rel, canonical_root.as_deref(), &mut parents);
                    (path, rel, effective)
                })
                .collect();
            let rels: HashSet<PathBuf> = sources.iter().map(|(_, _, e)| e.clone()).collect();
            let (walked, _) = self.walk_listed(rels);
            for (path, rel, effective) in sources {
                if walked.contains(&effective) {
                    continue;
                }
                // Not yielded: ignored unless it vanished since the diff (only
                // NotFound means that; any other error is a traversal failure,
                // which the walk also reads as not indexed), when the hand
                // model decides.
                let gone = self
                    .root
                    .join(&rel)
                    .symlink_metadata()
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
                if !gone || self.is_ignored(path) {
                    ignored.insert(path.clone());
                }
            }
        }
        ignored
    }

    /// `rel` with its parent directory resolved through symlinks to where it
    /// really is inside the root (the leaf is never resolved: a symlinked file
    /// is not a regular file). Falls back to `rel` when the directory is gone
    /// or resolves outside the root. `parents` memoises per directory.
    fn through_aliases(
        &self,
        rel: &Path,
        canonical_root: Option<&Path>,
        parents: &mut HashMap<PathBuf, PathBuf>,
    ) -> PathBuf {
        let (Some(parent), Some(name), Some(croot)) =
            (rel.parent(), rel.file_name(), canonical_root)
        else {
            return rel.to_path_buf();
        };
        let resolved = parents.entry(parent.to_path_buf()).or_insert_with(|| {
            self.root
                .join(parent)
                .canonicalize()
                .ok()
                .and_then(|real| real.strip_prefix(croot).ok().map(Path::to_path_buf))
                .unwrap_or_else(|| parent.to_path_buf())
        });
        resolved.join(name)
    }

    /// Runs the shared walk pruned to the ancestors of `listed`, returning the
    /// listed regular files it yields and the number of directories it entered.
    fn walk_listed(&self, listed: HashSet<PathBuf>) -> (HashSet<PathBuf>, usize) {
        let ancestors: HashSet<PathBuf> = listed
            .iter()
            .flat_map(|rel| rel.ancestors().skip(1))
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        let root = self.root.clone();
        let wanted = listed.clone();
        let walk = walk_builder(&self.root)
            .filter_entry(move |entry| {
                entry.depth() == 0
                    || entry
                        .path()
                        .strip_prefix(&root)
                        .is_ok_and(|rel| ancestors.contains(rel) || wanted.contains(rel))
            })
            .build();
        let (mut yielded, mut dirs) = (HashSet::new(), 0);
        for entry in walk.flatten() {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                dirs += 1;
            } else if is_walked_file(&entry) {
                if let Ok(rel) = entry.path().strip_prefix(&self.root) {
                    if listed.contains(rel) {
                        yielded.insert(rel.to_path_buf());
                    }
                }
            }
        }
        (yielded, dirs)
    }

    /// Number of directories whose ignore state has been built (test hook).
    #[cfg(test)]
    fn cached_dirs(&self) -> usize {
        self.dirs.lock().unwrap().len()
    }

    /// Whether `path` — relative to the root, or absolute beneath it — is
    /// excluded, either directly or through an ignored parent directory, by
    /// the hand model of the walker's rules. The hidden rule applies only to
    /// paths `detect_language` recognises. This is the verdict for base-side
    /// paths and the fallback for vanished ones; it never reads the path.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let Some(relative) = self.relative(path) else {
            return false;
        };
        let hidden_rule = detect_language(relative).is_some();
        let parent = self.state(relative.parent().unwrap_or(Path::new("")));
        let m = entry_match(&parent, relative, false);
        if hidden_rule {
            parent.excluded_hidden || m.is_ignore() || hidden_unmatched(relative, &m)
        } else {
            parent.excluded || m.is_ignore()
        }
    }
}

/// Find which package a file belongs to using longest-prefix match.
///
/// `file_path` must be in the same form as the layout's package paths (absolute
/// when the layout came from `detect_monorepo(root)`); a root-relative path
/// never prefix-matches. This is the single package authority shared by
/// `keel map` (via `walk_with_packages`) and `keel compile`'s graph sync.
pub fn package_for_path(file_path: &Path, layout: &MonorepoLayout) -> Option<String> {
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
