use super::*;

#[test]
fn save_project_id_empty_config() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();

    save_project_id(&keel_dir, "proj_123");

    let content = std::fs::read_to_string(keel_dir.join("keel.json")).unwrap();
    assert!(content.contains("proj_123"));
}

#[test]
fn save_project_id_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();

    // Write existing config without project_id
    std::fs::write(
        keel_dir.join("keel.json"),
        r#"{"version":"0.1.0","languages":["rust"]}"#,
    )
    .unwrap();

    save_project_id(&keel_dir, "proj_456");

    let content = std::fs::read_to_string(keel_dir.join("keel.json")).unwrap();
    assert!(content.contains("proj_456"));
    assert!(content.contains("version"));
}

#[test]
fn save_project_id_already_has_one() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();

    std::fs::write(
        keel_dir.join("keel.json"),
        r#"{"project_id":"existing_id"}"#,
    )
    .unwrap();

    // Should not overwrite
    save_project_id(&keel_dir, "new_id");

    let content = std::fs::read_to_string(keel_dir.join("keel.json")).unwrap();
    assert!(content.contains("existing_id"));
    assert!(!content.contains("new_id"));
}

#[test]
fn read_project_id_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_project_id(dir.path()), None);
}

#[test]
fn read_project_id_valid() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("keel.json"),
        r#"{"project_id":"proj_789"}"#,
    )
    .unwrap();
    assert_eq!(read_project_id(dir.path()), Some("proj_789".into()));
}

#[test]
fn read_project_id_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("keel.json"),
        r#"{"version":"0.1.0"}"#,
    )
    .unwrap();
    assert_eq!(read_project_id(dir.path()), None);
}
