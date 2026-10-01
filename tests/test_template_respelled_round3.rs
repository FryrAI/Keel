//! Round-three regressions for renamed owners and unknown callable populations.
#[path = "common/mod.rs"]
mod common;

use std::{fs, path::Path};

const HOME: &str =
    "// template\nfn berlin() -> &'static str { \"DOMAIN fixed::segment with punctuation\" }\n";
const COPY: &str = "const COPY: &str = \"DOMAIN fixed::segment with punctuation\";\n";

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "base"]);
    assert!(common::keel(dir.path(), &["init", "--yes"])
        .status
        .success());
    assert!(common::keel(dir.path(), &["map"]).status.success());
    dir
}

fn add_copy(root: &Path) {
    fs::write(root.join("caller.rs"), COPY).unwrap();
    common::git(root, &["add", "caller.rs"]);
}

fn assert_owner(root: &Path, present: bool) {
    let expected = usize::from(present);
    let output = common::keel(root, &["review", "--base", "HEAD", "--json", "--gate"]);
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let advisories = result["template_advisories"].as_array();
    assert_eq!(
        advisories.map_or(0, Vec::len),
        expected,
        "CLI review: {result}"
    );
    if present {
        assert_eq!(result["template_advisories"][0]["file"], "caller.rs");
        assert_eq!(
            result["template_advisories"][0]["homes"][0]["name"],
            "berlin"
        );
        assert_eq!(
            result["template_advisories"][0]["homes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert!(result["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|violation| violation["code"] != "W012"));

    let db = root.join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    let commit = keel_enforce::gitdiff::resolve_commit(root, "HEAD").unwrap();
    let paths = keel_enforce::gitdiff::changed_paths(root, &commit).unwrap();
    let exported =
        keel_enforce::template_respelled::review_export(&store, root, &commit, &paths).unwrap();
    assert_eq!(exported.homes.len(), expected, "precision export owners");
    assert_eq!(
        exported.occurrences.len(),
        expected,
        "precision export copies"
    );
    if present {
        assert_eq!(exported.occurrences[0].file, "caller.rs");
        assert_eq!(exported.occurrences[0].homes[0].name, "berlin");
    }
    let advisories =
        keel_enforce::template_respelled::review_advisories(&store, root, &commit, &paths, false);
    assert_eq!(advisories.len(), expected, "public advisory API");
    if present {
        assert_eq!(advisories[0].file, "caller.rs");
        assert_eq!(advisories[0].homes[0].name, "berlin");
    }
}

fn renamed_owner(edited: bool) {
    let dir = setup();
    common::git(dir.path(), &["mv", "home.rs", "dates.rs"]);
    if edited {
        fs::write(dir.path().join("dates.rs"), format!("{HOME}// changed\n")).unwrap();
    }
    add_copy(dir.path());
    let paths = keel_enforce::gitdiff::changed_paths(dir.path(), "HEAD").unwrap();
    assert!(
        paths.iter().any(|path| matches!(
            &path.status,
            keel_enforce::gitdiff::ChangeStatus::Renamed { from }
                if from == "home.rs" && path.path == "dates.rs"
        )),
        "edited={edited}: {paths:?}"
    );
    let diff = std::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "diff",
            "--name-status",
            "-M",
            "HEAD",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(diff.status.success());
    let status = if edited { "R088" } else { "R100" };
    assert!(
        String::from_utf8_lossy(&diff.stdout).contains(&format!("{status}\thome.rs\tdates.rs")),
        "{diff:?}"
    );
    assert_owner(dir.path(), true);
}

#[test]
fn template_round3_renamed_owner_survives_without_remapping() {
    renamed_owner(false);
}

#[test]
fn template_round3_renamed_edited_owner_survives_without_remapping() {
    renamed_owner(true);
}

#[test]
fn template_round3_readable_head_owner_survives_unreadable_base() {
    let dir = setup();
    let mut bytes = HOME.as_bytes().to_vec();
    bytes.extend_from_slice(b"// \xff\n");
    fs::write(dir.path().join("home.rs"), bytes).unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "non-UTF-8 base comment"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    add_copy(dir.path());
    assert_owner(dir.path(), true);
}

#[test]
fn template_round3_unreadable_head_keeps_unknown_cached_owner() {
    let dir = setup();
    let mut bytes = HOME.as_bytes().to_vec();
    bytes.extend_from_slice(b"// \xff\n");
    fs::write(dir.path().join("home.rs"), bytes).unwrap();
    add_copy(dir.path());
    assert_owner(dir.path(), true);
}

#[test]
fn template_round3_readable_head_prunes_removed_owner_despite_unreadable_base() {
    let dir = setup();
    let mut bytes = HOME.as_bytes().to_vec();
    bytes.extend_from_slice(b"// \xff\n");
    fs::write(dir.path().join("home.rs"), bytes).unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "non-UTF-8 base comment"]);
    fs::write(dir.path().join("home.rs"), "fn other() {}\n").unwrap();
    add_copy(dir.path());
    assert_owner(dir.path(), false);
}
