//! Parity oracle (issue #90): `KeelIgnore` must agree with `FileWalker` on
//! every source file of a tree with nested `.gitignore` / `.keelignore` files.

use super::*;
use std::fs;

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn keelignore_matcher_agrees_with_walker_on_nested_ignore_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // The walker only honours `.gitignore` inside a git repository.
    fs::create_dir(root.join(".git")).unwrap();

    write(root, "app.rs", "fn a() {}");
    // Nested .keelignore excluding a subdirectory.
    write(root, "pkg/.keelignore", "vendor/\n");
    write(root, "pkg/lib.rs", "fn l() {}");
    write(root, "pkg/vendor/v.rs", "fn v() {}");
    write(root, "pkg/vendor/deep/d.rs", "fn d() {}");
    // Nested .gitignore excluding a file.
    write(root, "gen/.gitignore", "out.rs\n");
    write(root, "gen/out.rs", "fn o() {}");
    write(root, "gen/keep.rs", "fn k() {}");
    // A deeper negation re-including what a shallower file excluded.
    write(root, ".gitignore", "*.gen.rs\n");
    write(root, "top.gen.rs", "fn t() {}");
    write(root, "neg/.gitignore", "!*.gen.rs\n");
    write(root, "neg/back.gen.rs", "fn b() {}");
    // .keelignore and .gitignore in the same directory disagree.
    write(root, "both/.gitignore", "x.rs\ny.rs\n");
    write(root, "both/.keelignore", "!x.rs\ny.rs\n");
    write(root, "both/x.rs", "fn x() {}");
    write(root, "both/y.rs", "fn y() {}");
    write(root, "both/z.rs", "fn z() {}");
    // An excluded ancestor directory with a deeper re-include attempt.
    write(root, "dead/.keelignore", "sub/\n");
    write(root, "dead/sub/.keelignore", "!*.rs\n");
    write(root, "dead/sub/s.rs", "fn s() {}");

    let walked: Vec<PathBuf> = FileWalker::new(root)
        .walk()
        .into_iter()
        .map(|e| e.path.strip_prefix(root).unwrap().to_path_buf())
        .collect();
    let ignore = KeelIgnore::new(root);
    let batch = KeelIgnore::new(root);

    let mut sources = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                sources.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    assert_eq!(sources.len(), 12, "fixture drifted: {sources:?}");
    let listed: Vec<PathBuf> = sources.clone();
    let batch_ignored = batch.ignored_paths(&listed);
    for rel in &sources {
        assert_eq!(
            batch_ignored.contains(rel),
            !walked.contains(rel),
            "ignored_paths and walker disagree on {}",
            rel.display()
        );
        assert_eq!(
            ignore.is_ignored(rel),
            !walked.contains(rel),
            "matcher and walker disagree on {}",
            rel.display()
        );
    }
    // Sanity: the fixture really exercises both outcomes.
    assert!(walked.contains(&PathBuf::from("neg/back.gen.rs")));
    assert!(walked.contains(&PathBuf::from("both/x.rs")));
    assert!(!walked.contains(&PathBuf::from("dead/sub/s.rs")));
    assert!(!walked.contains(&PathBuf::from("pkg/vendor/v.rs")));
}

/// Builds `files` (paths relative to a temp dir) and returns
/// `(model_disagreements, walk_disagreements, walked)` for the repo rooted at
/// `temp/repo` (`repo` empty = the temp dir itself): the source files on which
/// the hand model [`KeelIgnore::is_ignored`], respectively the batch filter
/// [`KeelIgnore::ignored_paths`], differs from `FileWalker`, and the files the
/// walker visits. `git` creates a `.git` directory (without a repository
/// marker the walker ignores `.gitignore`).
fn compare_at(
    files: &[(&str, &str)],
    repo: &str,
    git: bool,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(repo);
    fs::create_dir_all(&root).unwrap();
    if git {
        fs::create_dir(root.join(".git")).unwrap();
    }
    let mut sources = Vec::new();
    for (rel, content) in files {
        write(dir.path(), rel, content);
        let inside = Path::new(rel).strip_prefix(repo).unwrap_or(Path::new(rel));
        if (repo.is_empty() || rel.starts_with(repo)) && is_source(rel) {
            sources.push(inside.to_string_lossy().into_owned());
        }
    }
    let walked: Vec<String> = FileWalker::new(&root)
        .walk()
        .into_iter()
        .map(|e| {
            e.path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let ignore = KeelIgnore::new(&root);
    let model = sources
        .iter()
        .filter(|s| ignore.is_ignored(Path::new(s)) == walked.contains(s))
        .cloned()
        .collect();
    let listed: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let ignored = KeelIgnore::new(&root).ignored_paths(&listed);
    let walk = sources
        .into_iter()
        .filter(|s| ignored.contains(Path::new(s)) == walked.contains(s))
        .collect();
    (model, walk, walked)
}

fn is_source(rel: &str) -> bool {
    rel.ends_with(".rs") || rel.ends_with(".py")
}

fn compare(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>, Vec<String>) {
    compare_at(files, "", true)
}

/// Asserts the hand model AND the batch filter agree with the walker on
/// `files`, and that the walker visits exactly `visited`.
fn assert_parity(files: &[(&str, &str)], visited: &[&str]) {
    let (model, walk, mut walked) = compare(files);
    assert!(
        model.is_empty(),
        "hand model disagrees with walker on {model:?}"
    );
    assert!(
        walk.is_empty(),
        "ignored_paths disagrees with walker on {walk:?}"
    );
    walked.sort();
    let mut want: Vec<&str> = visited.to_vec();
    want.sort();
    assert_eq!(walked, want, "fixture no longer exercises the case");
}

/// Like [`assert_walk_parity`], and the hand model must agree with the walker
/// too (for behaviours production resolves by walking but base-side paths
/// need modelled).
fn assert_both_parity(files: &[(&str, &str)], visited: &[&str]) {
    let (model, _, _) = compare_at(files, "", true);
    assert!(
        model.is_empty(),
        "hand model disagrees with walker on {model:?}"
    );
    assert_walk_parity(files, "", true, visited);
}

/// Asserts only the batch filter (existing paths) agrees with the walker, for
/// behaviours the hand model deliberately omits, and that the walker visits
/// exactly `visited`.
fn assert_walk_parity(files: &[(&str, &str)], repo: &str, git: bool, visited: &[&str]) {
    let (_, walk, mut walked) = compare_at(files, repo, git);
    assert!(
        walk.is_empty(),
        "ignored_paths disagrees with walker on {walk:?}"
    );
    walked.sort();
    let mut want: Vec<&str> = visited.to_vec();
    want.sort();
    assert_eq!(walked, want, "fixture no longer exercises the case");
}

/// Base agrees (root rules exclude); 59ba129 wrongly includes (the nested
/// `.gitignore` whitelist wins).
#[test]
fn parity_keelignore_outranks_deeper_gitignore_negation() {
    assert_parity(
        &[
            (".keelignore", "vendor/\n"),
            (".gitignore", "vendor/\n"),
            ("pkg/.gitignore", "!vendor/\n"),
            ("pkg/vendor/v.rs", "fn v() {}"),
            ("pkg/ok.rs", "fn o() {}"),
        ],
        &["pkg/ok.rs"],
    );
}

/// Base agrees; 59ba129 wrongly includes (nested whitelist wins).
#[test]
fn parity_root_keelignore_beats_nested_gitignore_whitelist() {
    assert_parity(
        &[
            (".keelignore", "*.rs\n"),
            ("pkg/.gitignore", "!x.rs\n"),
            ("pkg/x.rs", "fn x() {}"),
        ],
        &[],
    );
}

/// Base agrees; 59ba129 wrongly excludes (the nested exclusion wins).
#[test]
fn parity_root_keelignore_whitelist_beats_nested_gitignore_exclusion() {
    assert_parity(
        &[
            (".keelignore", "!*.rs\n"),
            ("pkg/.gitignore", "x.rs\n"),
            ("pkg/x.rs", "fn x() {}"),
        ],
        &["pkg/x.rs"],
    );
}

/// Base agrees; 59ba129 wrongly exposes the excluded subtree.
#[test]
fn parity_root_keelignore_dir_beats_nested_gitignore_whitelist() {
    assert_parity(
        &[
            (".keelignore", "pkg/sub/\n"),
            ("pkg/.gitignore", "!sub/\n"),
            ("pkg/sub/s.rs", "fn s() {}"),
            ("pkg/t.rs", "fn t() {}"),
        ],
        &["pkg/t.rs"],
    );
}

/// `.ignore` files: neither base nor 59ba129 reads them (both fail).
#[test]
fn parity_dot_ignore_files() {
    assert_parity(
        &[
            ("ig/.ignore", "i.py\n"),
            ("ig/i.py", "def i(): pass"),
            ("ig/j.py", "def j(): pass"),
        ],
        &["ig/j.py"],
    );
}

/// Rank by kind: `.keelignore` > `.ignore` > `.gitignore`, across depths. Base
/// and 59ba129 both fail (neither reads `.ignore`).
#[test]
fn parity_kind_ranking_across_depths() {
    assert_parity(
        &[
            // root .ignore excludes; nested .gitignore whitelist loses.
            (".ignore", "a.rs\n"),
            ("p1/.gitignore", "!a.rs\n"),
            ("p1/a.rs", "fn a() {}"),
            // root .gitignore excludes; nested .ignore whitelist wins.
            (".gitignore", "b.rs\n"),
            ("p2/.ignore", "!b.rs\n"),
            ("p2/b.rs", "fn b() {}"),
            // root .keelignore excludes; nested .ignore whitelist loses.
            (".keelignore", "c.rs\n"),
            ("p3/.ignore", "!c.rs\n"),
            ("p3/c.rs", "fn c() {}"),
        ],
        &["p2/b.rs"],
    );
}

/// A nested repository ends `.gitignore` inheritance: base and 59ba129 exclude
/// `pkg/x.rs`, the walker includes it. `.keelignore` keeps crossing the boundary.
#[test]
fn parity_nested_repository_boundary() {
    assert_parity(
        &[
            (".gitignore", "*.rs\n"),
            ("pkg/.git/HEAD", "ref: refs/heads/main\n"),
            ("pkg/x.rs", "fn x() {}"),
            ("top.rs", "fn t() {}"),
        ],
        &["pkg/x.rs"],
    );
    assert_parity(
        &[
            (".keelignore", "*.rs\n"),
            ("pkg/.git/HEAD", "ref: refs/heads/main\n"),
            ("pkg/x.rs", "fn x() {}"),
        ],
        &[],
    );
}

/// The hand model builds each directory's state once, however many paths share
/// it: 10,000 distinct absent paths 32 directories deep cost 33 directory
/// states (deterministic; timing belongs in a benchmark, not a test).
#[test]
fn deep_paths_without_ignore_files_build_one_state_per_directory() {
    let dir = tempfile::tempdir().unwrap();
    let ignore = KeelIgnore::new(dir.path());
    let deep: String = (0..32).map(|i| format!("d{i}/")).collect();
    let paths: Vec<PathBuf> = (0..10_000)
        .map(|n| PathBuf::from(format!("{deep}f{n}.rs")))
        .collect();
    assert!(paths.iter().all(|p| !ignore.is_ignored(p)));
    assert_eq!(ignore.cached_dirs(), 33, "root plus the 32 ancestors");
}

/// The pruned walk enters only the listed paths' ancestor directories, however
/// many unrelated directories the tree holds.
#[test]
fn pruned_walk_enters_only_ancestor_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for d in 0..50 {
        write(root, &format!("other{d}/x/f.rs"), "fn f() {}");
    }
    let mut listed = std::collections::HashSet::new();
    for n in 0..200 {
        let rel = format!("a/b/c{}/f{n}.rs", n % 3);
        write(root, &rel, "fn f() {}");
        listed.insert(PathBuf::from(rel));
    }
    let (walked, dirs) = KeelIgnore::new(root).walk_listed(listed.clone());
    assert_eq!(walked, listed);
    // root, a, a/b and the three a/b/cN directories.
    assert_eq!(dirs, 6);
}

/// Hidden files and directories are skipped by the walker unless a rule
/// whitelists them. Base and df040e5 include the hidden ones.
#[test]
fn parity_hidden_entries() {
    assert_both_parity(
        &[
            (".github/scripts/h.py", "def h(): pass"),
            (".hid.py", "def h(): pass"),
            ("ok.py", "def o(): pass"),
        ],
        &["ok.py"],
    );
    assert_both_parity(
        &[
            (".keelignore", "!.github/\n!.hid.py\n"),
            (".github/scripts/h.py", "def h(): pass"),
            (".hid.py", "def h(): pass"),
            (".other/o.py", "def o(): pass"),
        ],
        &[".github/scripts/h.py", ".hid.py"],
    );
}

/// `.git/info/exclude` excludes a file the walker then never maps, even when
/// git still lists it as tracked. Base and df040e5 check it.
#[test]
fn parity_git_info_exclude() {
    assert_both_parity(
        &[
            (".git/info/exclude", "tracked.rs\n"),
            ("tracked.rs", "fn t() {}"),
            ("ok.rs", "fn o() {}"),
        ],
        &["ok.rs"],
    );
}

/// The walker reads `.keelignore` in directories above its root (both
/// directions). Base and df040e5 do not.
#[test]
fn parity_ignore_files_above_the_root() {
    assert_walk_parity(
        &[
            (".keelignore", "!*.rs\n"),
            ("repo/.gitignore", "*.rs\n"),
            ("repo/x.rs", "fn x() {}"),
        ],
        "repo",
        true,
        &["x.rs"],
    );
    assert_walk_parity(
        &[
            (".keelignore", "x.rs\n"),
            ("repo/x.rs", "fn x() {}"),
            ("repo/y.rs", "fn y() {}"),
        ],
        "repo",
        true,
        &["y.rs"],
    );
}

/// Without any repository marker the walker ignores `.gitignore` (require_git).
/// Base and df040e5 honour it.
#[test]
fn parity_gitignore_needs_a_repository() {
    assert_walk_parity(
        &[(".gitignore", "x.rs\n"), ("x.rs", "fn x() {}")],
        "",
        false,
        &["x.rs"],
    );
}

/// A tracked symlink to a source file is never mapped (not a regular file).
/// Base and df040e5 admit it.
#[cfg(unix)]
#[test]
fn parity_symlink_is_not_a_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "real.py", "def r(): pass");
    std::os::unix::fs::symlink(root.join("real.py"), root.join("alias.py")).unwrap();
    let walked: Vec<PathBuf> = FileWalker::new(root)
        .walk()
        .into_iter()
        .map(|e| e.path.strip_prefix(root).unwrap().to_path_buf())
        .collect();
    assert_eq!(walked, vec![PathBuf::from("real.py")]);
    let listed = vec![PathBuf::from("alias.py"), PathBuf::from("real.py")];
    let ignored = KeelIgnore::new(root).ignored_paths(&listed);
    assert!(ignored.contains(Path::new("alias.py")));
    assert!(!ignored.contains(Path::new("real.py")));
}

/// A HEAD-side path through a directory that became a symlink is judged as the
/// file it resolves to inside the root: map indexes the target, never the alias
/// spelling. A target under an excluded directory is ignored.
#[cfg(unix)]
#[test]
fn path_through_a_symlinked_directory_is_judged_by_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "real/lib.rs", "fn l() {}");
    write(root, "hid/lib.rs", "fn l() {}");
    write(root, ".keelignore", "hid/\n");
    std::os::unix::fs::symlink("real", root.join("pkg")).unwrap();
    std::os::unix::fs::symlink("hid", root.join("alias")).unwrap();
    let listed = [PathBuf::from("pkg/lib.rs"), PathBuf::from("alias/lib.rs")];
    let ignored = KeelIgnore::new(root).ignored_paths(&listed);
    assert!(!ignored.contains(Path::new("pkg/lib.rs")));
    assert!(ignored.contains(Path::new("alias/lib.rs")));
}
