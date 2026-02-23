use super::*;

#[test]
fn newer_patch() {
    assert!(is_newer("0.3.1", "0.3.2"));
}

#[test]
fn newer_minor() {
    assert!(is_newer("0.3.1", "0.4.0"));
}

#[test]
fn newer_major() {
    assert!(is_newer("0.3.1", "1.0.0"));
}

#[test]
fn same_version() {
    assert!(!is_newer("0.3.1", "0.3.1"));
}

#[test]
fn older_version() {
    assert!(!is_newer("0.3.1", "0.3.0"));
}

#[test]
fn older_major() {
    assert!(!is_newer("1.0.0", "0.9.9"));
}

#[test]
fn check_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CHECK_FILE);

    let state = UpdateCheck {
        last_checked: 1_700_000_000,
        latest_version: Some("0.4.0".to_string()),
    };

    std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();

    let loaded: UpdateCheck =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(loaded.last_checked, 1_700_000_000);
    assert_eq!(loaded.latest_version.as_deref(), Some("0.4.0"));
}

#[test]
fn ci_env_skips_check() {
    // When CI is set, maybe_check_async should return immediately.
    // We just verify it doesn't panic.
    std::env::set_var("CI", "true");
    maybe_check_async(false);
    std::env::remove_var("CI");
}

#[test]
fn no_telemetry_skips_check() {
    // Should return immediately without doing anything.
    maybe_check_async(true);
}
