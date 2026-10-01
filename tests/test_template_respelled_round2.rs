//! Round-two regressions for copy attribution and removed cached owners.
#[path = "common/mod.rs"]
mod common;

use std::{fs, path::Path};

const HOME: &str = "fn home() -> &'static str { \"DOMAIN fixed::segment with punctuation\" }\n";
const COPY: &str = "const COPY: &str = \"DOMAIN fixed::segment with punctuation\";\n";

fn setup(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    for (file, source) in files {
        fs::write(dir.path().join(file), source).unwrap();
    }
    common::git(dir.path(), &["add", "."]);
    common::git(dir.path(), &["commit", "-qm", "base"]);
    assert!(common::keel(dir.path(), &["init", "--yes"])
        .status
        .success());
    assert!(common::keel(dir.path(), &["map"]).status.success());
    dir
}

fn assert_findings(root: &Path, expected: &[(&str, &str)]) {
    let output = common::keel(root, &["review", "--base", "HEAD", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<_> = result["template_advisories"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|advisory| {
            advisory["homes"].as_array().unwrap().iter().map(|home| {
                (
                    advisory["file"].as_str().unwrap(),
                    home["name"].as_str().unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(actual, expected, "structural review: {result}");

    let db = root.join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    let commit = keel_enforce::gitdiff::resolve_commit(root, "HEAD").unwrap();
    let paths = keel_enforce::gitdiff::changed_paths(root, &commit).unwrap();
    let exported =
        keel_enforce::template_respelled::review_export(&store, root, &commit, &paths).unwrap();
    let actual: Vec<_> = exported
        .occurrences
        .iter()
        .flat_map(|occurrence| {
            occurrence
                .homes
                .iter()
                .map(|home| (occurrence.file.as_str(), home.name.as_str()))
        })
        .collect();
    assert_eq!(actual, expected, "precision export");
    let advisories =
        keel_enforce::template_respelled::review_advisories(&store, root, &commit, &paths, false);
    let actual: Vec<_> = advisories
        .iter()
        .flat_map(|advisory| {
            advisory
                .homes
                .iter()
                .map(|home| (advisory.file.as_str(), home.name.as_str()))
        })
        .collect();
    assert_eq!(actual, expected, "public advisory API");
}

#[test]
fn template_round2_existing_copy_stays_in_edited_file() {
    for new_file in ["a.rs", "z.rs"] {
        let dir = setup(&[("m.rs", COPY)]);
        fs::write(
            dir.path().join("m.rs"),
            format!("{COPY}// unrelated edit\n"),
        )
        .unwrap();
        fs::write(dir.path().join(new_file), COPY).unwrap();
        common::git(dir.path(), &["add", new_file]);
        assert_findings(dir.path(), &[(new_file, "home")]);
    }
}

#[test]
fn template_round2_pure_rename_does_not_take_new_copy_blame() {
    let dir = setup(&[("old.rs", &format!("// existing copy\n{COPY}"))]);
    fs::rename(dir.path().join("old.rs"), dir.path().join("moved.rs")).unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "-A", "."]);
    let paths = keel_enforce::gitdiff::changed_paths(dir.path(), "HEAD").unwrap();
    assert!(paths.iter().any(|path| matches!(
        &path.status,
        keel_enforce::gitdiff::ChangeStatus::Renamed { from }
            if from == "old.rs" && path.path == "moved.rs"
    )));
    assert_findings(dir.path(), &[("caller.rs", "home")]);
}

#[cfg(unix)]
#[test]
fn template_round2_unreadable_owner_rename_drops_advisory() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let dir = setup(&[]);
    fs::rename(
        dir.path().join("home.rs"),
        dir.path().join(OsStr::from_bytes(b"home\xff.rs")),
    )
    .unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "-A", "."]);
    let paths = keel_enforce::gitdiff::changed_paths(dir.path(), "HEAD").unwrap();
    assert!(paths.iter().any(|path| {
        path.path == "home.rs"
            && path.status == keel_enforce::gitdiff::ChangeStatus::RenamedToUnreadable
    }));
    assert_findings(dir.path(), &[]);
}

#[test]
fn template_round2_removed_callable_is_not_an_owner() {
    let dir = setup(&[]);
    fs::write(dir.path().join("home.rs"), "fn kept() {}\n").unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    assert_findings(dir.path(), &[]);
}

#[test]
fn template_round2_renamed_ineligible_callable_has_no_self_advisory() {
    let dir = setup(&[]);
    fs::write(
        dir.path().join("home.rs"),
        "fn home_trimmed() -> &'static str { let _ = 1; \"DOMAIN fixed::segment with punctuation\" }\n",
    )
    .unwrap();
    assert_findings(dir.path(), &[]);
}

#[test]
fn template_round2_removed_coowner_leaves_live_owner() {
    let dir = setup(&[("other.rs", &HOME.replace("home()", "other()"))]);
    fs::write(dir.path().join("home.rs"), "fn kept() {}\n").unwrap();
    fs::write(dir.path().join("caller.rs"), COPY).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    assert_findings(dir.path(), &[("caller.rs", "other")]);
}
