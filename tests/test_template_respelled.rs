//! Review-only template advisories: real CLI, map cache, and Git baselines.
#[path = "common/mod.rs"]
mod common;

use serde_json::Value;
use std::{fs, path::Path};

const HOME: &str =
    "fn berlin(x: &str) -> String { format!(\"({x} AT TIME ZONE 'Europe/Berlin')::date\") }\n";
const CALLER: &str = "const SQL: &str = \"SELECT (now() AT TIME ZONE 'Europe/Berlin')::date\";\n";

fn setup() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    common::git(dir.path(), &["init", "-q"]);
    common::git(
        dir.path(),
        &["commit", "--allow-empty", "-qm", "empty base"],
    );
    common::git(dir.path(), &["tag", "empty-base"]);
    fs::write(dir.path().join("home.rs"), HOME).unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "home"]);
    assert!(common::keel(dir.path(), &["init", "--yes"])
        .status
        .success());
    dir
}

fn map(dir: &Path) {
    assert!(common::keel(dir, &["map", "--json"]).status.success());
}

fn review(dir: &Path, base: &str) -> Value {
    let output = common::keel(dir, &["review", "--base", base, "--json", "--gate"]);
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn count(result: &Value) -> usize {
    result["template_advisories"].as_array().map_or(0, Vec::len)
}

#[test]
fn template_review_reads_unchanged_mapped_home_and_empty_base_exports_all() {
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    assert_eq!(count(&result), 1);
    let advisory = &result["template_advisories"][0];
    assert_eq!(advisory["code"], "W012");
    assert_eq!(advisory["homes"][0]["name"], "berlin");
    assert_eq!(advisory["file"], "caller.rs");
    assert!(result["new_violations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["code"] != "W012"));
    assert_eq!(count(&review(dir.path(), "empty-base")), 1);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}

#[test]
fn template_review_subtracts_renames_moves_and_counts_extra_copies() {
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    map(dir.path());
    common::git(dir.path(), &["mv", "caller.rs", "moved.rs"]);
    common::git(dir.path(), &["mv", "home.rs", "new-home.rs"]);
    map(dir.path());
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
    fs::write(
        dir.path().join("moved.rs"),
        format!("{CALLER}{}", CALLER.replace("SQL", "COPY")),
    )
    .unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
    fs::remove_file(dir.path().join("moved.rs")).unwrap();
    fs::write(dir.path().join("destination.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "destination.rs", "moved.rs"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}

#[test]
fn template_cache_is_map_derived_and_ignored_homes_never_enter_it() {
    let dir = setup();
    fs::write(dir.path().join(".keelignore"), "ignored.rs\n").unwrap();
    fs::write(
        dir.path().join("ignored.rs"),
        HOME.replace("berlin", "ignored"),
    )
    .unwrap();
    map(dir.path());
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
    fs::write(dir.path().join("home.rs"), "fn empty() {}\n").unwrap();
    assert_eq!(
        count(&review(dir.path(), "HEAD")),
        0,
        "removed callable is excluded before the next map"
    );
    {
        use keel_core::store::GraphStore;
        let db = dir.path().join(".keel/graph.db");
        let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
        assert_eq!(
            store.template_homes().len(),
            1,
            "review leaves cache intact"
        );
    }
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    assert_eq!(count(&result), 0);
    assert!(result.get("template_advisories").is_none());
}

#[cfg(unix)]
#[test]
fn template_review_skips_symlink_text_on_both_sides() {
    use std::os::unix::fs::symlink;
    let dir = setup();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    map(dir.path());
    fs::remove_file(dir.path().join("caller.rs")).unwrap();
    symlink("home.rs", dir.path().join("caller.rs")).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
    common::git(dir.path(), &["add", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "symlink"]);
    fs::remove_file(dir.path().join("caller.rs")).unwrap();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 1);
}

#[test]
fn template_review_without_segments_does_not_read_unavailable_blobs() {
    let dir = setup();
    fs::write(dir.path().join("home.rs"), "fn empty() {}\n").unwrap();
    map(dir.path());
    fs::write(dir.path().join("caller.rs"), b"fn query() {}\n\xff\n").unwrap();
    common::git(dir.path(), &["add", "home.rs", "caller.rs"]);
    common::git(dir.path(), &["commit", "-qm", "unavailable base"]);
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    assert_eq!(count(&review(dir.path(), "HEAD")), 0);
}

#[test]
fn template_advisory_reaches_json_human_llm_but_never_gates_or_compiles() {
    let dir = setup();
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["review"] = serde_json::json!({"gate": ["W012"]});
    fs::write(config_path, config.to_string()).unwrap();
    fs::write(dir.path().join("caller.rs"), CALLER).unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    let advisory = &result["template_advisories"][0];
    assert_eq!(advisory["category"], "template_respelled");
    assert_eq!(advisory["line"], 1);
    let message = "Literal re-spells the fixed text of berlin (home.rs:1): \"AT TIME ZONE 'Europe/Berlin')::date\" — call it instead.";
    assert_eq!(advisory["message"], message);
    assert!(result.get("template_study").is_none());
    assert!(result["new_violations"].as_array().unwrap().is_empty());
    for format in [Vec::new(), vec!["--llm"]] {
        let mut args = vec!["review", "--base", "HEAD", "--gate"];
        args.extend(format);
        let output = common::keel(dir.path(), &args);
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("W012") && text.contains(message), "{text}");
    }
    let output = common::keel(dir.path(), &["compile", "caller.rs", "--json"]);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "clean compile stays silent: {output:?}"
    );
    common::git(dir.path(), &["commit", "-qm", "caller"]);
    assert!(review(dir.path(), "HEAD")
        .get("template_advisories")
        .is_none());
    for format in [Vec::new(), vec!["--llm"]] {
        let mut args = vec!["review", "--base", "HEAD"];
        args.extend(format);
        let output = common::keel(dir.path(), &args);
        assert!(output.status.success());
        assert!(output.stdout.is_empty(), "{output:?}");
    }
}

#[test]
fn template_review_reports_each_literal_home_once_and_names_shared_owners() {
    let dir = setup();
    let home = "fn berlin(x: &str) -> String { format!(\"FIRST fixed::expression with punctuation {x} SECOND fixed::expression with punctuation\") }\n";
    fs::write(
        dir.path().join("home.rs"),
        format!("{home}{}", home.replace("berlin", "other")),
    )
    .unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "two homes"]);
    // Two distinct literal nodes on the same line must both survive deduplication.
    let literal =
        "\"FIRST fixed::expression with punctuation SECOND fixed::expression with punctuation\"";
    fs::write(
        dir.path().join("caller.rs"),
        format!("const A: &str = {literal}; const B: &str = {literal};\n"),
    )
    .unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    let result = review(dir.path(), "HEAD");
    assert_eq!(count(&result), 2);
    for advisory in result["template_advisories"].as_array().unwrap() {
        assert_eq!(advisory["homes"].as_array().unwrap().len(), 2);
        let message = advisory["message"].as_str().unwrap();
        assert!(message.contains("berlin (home.rs:1)") && message.contains("other (home.rs:2)"));
    }
}

#[test]
fn template_review_threshold_24_drops_a_16_to_23_character_segment() {
    let dir = setup();
    fs::write(
        dir.path().join("home.rs"),
        "fn short() -> &'static str { \"SELECT value::date\" }\n",
    )
    .unwrap();
    common::git(dir.path(), &["add", "home.rs"]);
    common::git(dir.path(), &["commit", "-qm", "short home"]);
    fs::write(
        dir.path().join("caller.rs"),
        "const SQL: &str = \"SELECT value::date\";\n",
    )
    .unwrap();
    common::git(dir.path(), &["add", "caller.rs"]);
    map(dir.path());
    assert!(review(dir.path(), "HEAD")
        .get("template_advisories")
        .is_none());
    for format in [Vec::new(), vec!["--llm"]] {
        let mut args = vec!["review", "--base", "HEAD"];
        args.extend(format);
        let output = common::keel(dir.path(), &args);
        assert!(output.status.success());
        assert!(!String::from_utf8(output.stdout).unwrap().contains("W012"));
    }
}
