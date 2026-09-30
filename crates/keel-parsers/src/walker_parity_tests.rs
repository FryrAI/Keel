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
    for rel in &sources {
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

/// Builds `files` under a fresh git-like root (a `.git` directory, without which
/// the walker ignores `.gitignore`) and returns `(disagreements, walked)`: the
/// source files on which `KeelIgnore` and `FileWalker` differ, and the files
/// the walker visits.
fn compare(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".git")).unwrap();
    let mut sources = Vec::new();
    for (rel, content) in files {
        write(root, rel, content);
        if rel.ends_with(".rs") || rel.ends_with(".py") {
            sources.push(rel.to_string());
        }
    }
    let walked: Vec<String> = FileWalker::new(root)
        .walk()
        .into_iter()
        .map(|e| {
            e.path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let ignore = KeelIgnore::new(root);
    let bad = sources
        .into_iter()
        .filter(|s| ignore.is_ignored(Path::new(s)) == walked.contains(s))
        .collect();
    (bad, walked)
}

/// Asserts agreement on `files`, and that the walker visits exactly `visited`.
fn assert_parity(files: &[(&str, &str)], visited: &[&str]) {
    let (bad, mut walked) = compare(files);
    assert!(bad.is_empty(), "matcher disagrees with walker on {bad:?}");
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

/// 10,000 distinct paths 32 directories deep with no ignore files must cost a
/// small multiple of the base matcher (cached per-directory verdicts; debug
/// profile: base ~28 ms, 59ba129 ~18 s, this fold ~97 ms).
#[test]
fn deep_paths_without_ignore_files_are_cheap() {
    let dir = tempfile::tempdir().unwrap();
    let ignore = KeelIgnore::new(dir.path());
    let deep: String = (0..32).map(|i| format!("d{i}/")).collect();
    let paths: Vec<PathBuf> = (0..10_000)
        .map(|n| PathBuf::from(format!("{deep}f{n}.rs")))
        .collect();
    let start = std::time::Instant::now();
    let ignored = paths.iter().filter(|p| ignore.is_ignored(p)).count();
    let elapsed = start.elapsed();
    assert_eq!(ignored, 0);
    eprintln!("10k paths at depth 32: {elapsed:?}");
    assert!(elapsed.as_millis() < 500, "too slow: {elapsed:?}");
}
