//! Base symlink text must not grandfather new source or bypass the review gate.

use super::*;

fn replaced_symlink(severity: &str, code: &str) -> TempDir {
    let dir = fixture(true);
    config(dir.path(), severity, &[code]);
    let path = dir.path().join(".keel/keel.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    cfg["homes"][0]["home"] = json!([]);
    fs::write(path, cfg.to_string()).unwrap();
    write(dir.path(), "src/CURRENT_DATE", "pass\n");
    std::os::unix::fs::symlink("CURRENT_DATE", dir.path().join("src/check.py")).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "base symlink"]);
    fs::remove_file(dir.path().join("src/check.py")).unwrap();
    write(dir.path(), "src/check.py", "CURRENT_DATE\n");
    dir
}

#[test]
fn homes_compile_base_symlink_does_not_grandfather_regular_source() {
    for (severity, code, exit) in [("warning", "W011", 0), ("error", "E007", 1)] {
        let dir = replaced_symlink(severity, code);
        let commit = keel_enforce::gitdiff::resolve_commit(dir.path(), "HEAD").unwrap();
        assert_eq!(
            keel_enforce::gitdiff::blob_at_checked(dir.path(), &commit, "src/check.py").unwrap(),
            None
        );
        for args in [
            vec!["compile", "src/check.py", "--json"],
            vec!["compile", "--changed", "--json"],
        ] {
            let out = keel(dir.path(), &args);
            assert_eq!(out.status.code(), Some(exit));
            let hits = violations_with_code(&parse(&out), code);
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0]["file"], "src/check.py");
            assert_eq!(hits[0]["line"], 1);
            assert_eq!(
                hits[0]["severity"],
                if exit == 1 { "ERROR" } else { "WARNING" }
            );
        }
    }
}

#[test]
fn homes_review_base_symlink_replacement_trips_configured_gate() {
    for (severity, code) in [("warning", "W011"), ("error", "E007")] {
        let dir = replaced_symlink(severity, code);
        for base in ["HEAD", "HEAD~1"] {
            if base == "HEAD~1" {
                git(dir.path(), &["add", "src/check.py"]);
                git(dir.path(), &["commit", "-q", "-m", "regular source"]);
            }
            let out = review(dir.path(), base, true);
            assert_eq!(out.status.code(), Some(1));
            let hits = review_hits(&out, code);
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0]["file"], "src/check.py");
            assert_eq!(hits[0]["line"], 1);
        }
    }
}
