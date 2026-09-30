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
