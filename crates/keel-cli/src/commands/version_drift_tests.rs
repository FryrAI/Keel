use std::fs;

use tempfile::TempDir;

use super::*;

const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Write a minimal `.keel/keel.json` with the given pinned version.
fn write_keel_json(keel_dir: &Path, version: &str) {
    fs::create_dir_all(keel_dir).unwrap();
    fs::write(
        keel_dir.join("keel.json"),
        format!(r#"{{"version": "{version}", "languages": []}}"#),
    )
    .unwrap();
}

#[test]
fn matching_keel_json_and_no_agents_md_is_silent() {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    write_keel_json(&keel_dir, BINARY_VERSION);

    assert_eq!(
        version_drift_message(dir.path(), &KeelConfig::load(&keel_dir)),
        None
    );
}

#[test]
fn stale_keel_json_reports_exactly_one_line_naming_both_versions() {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    write_keel_json(&keel_dir, "0.1.0");

    let msg = version_drift_message(dir.path(), &KeelConfig::load(&keel_dir))
        .expect("expected drift message");
    assert_eq!(msg.lines().count(), 1, "message must be exactly one line");
    assert!(msg.starts_with("keel: .keel/keel.json records 0.1.0, binary is "));
    assert!(msg.contains(BINARY_VERSION));
    assert!(msg.ends_with("run keel init --update-docs"));

    // Pure read: the config file on disk must be untouched.
    let after = fs::read_to_string(keel_dir.join("keel.json")).unwrap();
    assert!(after.contains("0.1.0"));
}

#[test]
fn fresh_keel_json_but_stale_docs_stamp_still_reports_drift() {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    write_keel_json(&keel_dir, BINARY_VERSION);
    fs::write(
        dir.path().join("AGENTS.md"),
        "<!-- keel:start -->\n<!-- keel:version 0.0.1 -->\n## keel\n<!-- keel:end -->\n",
    )
    .unwrap();

    let msg = version_drift_message(dir.path(), &KeelConfig::load(&keel_dir))
        .expect("expected drift message");
    assert_eq!(msg.lines().count(), 1);
    assert!(msg.starts_with("keel: AGENTS.md records 0.0.1, binary is "));
}

#[test]
fn fresh_keel_json_and_fresh_docs_stamp_is_silent() {
    let dir = TempDir::new().unwrap();
    let keel_dir = dir.path().join(".keel");
    write_keel_json(&keel_dir, BINARY_VERSION);
    fs::write(
        dir.path().join("AGENTS.md"),
        format!("<!-- keel:start -->\n<!-- keel:version {BINARY_VERSION} -->\n## keel\n<!-- keel:end -->\n"),
    )
    .unwrap();

    assert_eq!(
        version_drift_message(dir.path(), &KeelConfig::load(&keel_dir)),
        None
    );
}

#[test]
fn docs_version_stamp_extracts_the_pinned_version() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("AGENTS.md"),
        "intro\n<!-- keel:version 1.2.3 -->\nmore\n",
    )
    .unwrap();
    assert_eq!(docs_version_stamp(dir.path()), Some("1.2.3".to_string()));
}

#[test]
fn docs_version_stamp_is_none_without_agents_md() {
    let dir = TempDir::new().unwrap();
    assert_eq!(docs_version_stamp(dir.path()), None);
}
