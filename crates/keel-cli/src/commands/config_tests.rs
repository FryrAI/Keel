use super::*;

#[test]
fn test_parse_value_bool() {
    assert_eq!(parse_value("true"), serde_json::Value::Bool(true));
    assert_eq!(parse_value("false"), serde_json::Value::Bool(false));
}

#[test]
fn test_parse_value_number() {
    assert_eq!(parse_value("42"), serde_json::json!(42));
    assert_eq!(parse_value("0"), serde_json::json!(0));
}

#[test]
fn test_parse_value_string() {
    assert_eq!(parse_value("hello"), serde_json::json!("hello"));
    assert_eq!(parse_value("free"), serde_json::json!("free"));
}

#[test]
fn test_parse_value_null() {
    assert_eq!(parse_value("null"), serde_json::Value::Null);
}

#[test]
fn test_resolve_dot_path() {
    let json = serde_json::json!({
        "tier": "free",
        "telemetry": {
            "enabled": true,
            "remote": true,
            "detailed": false,
        }
    });
    assert_eq!(
        resolve_dot_path(&json, "tier"),
        Some(&serde_json::json!("free"))
    );
    assert_eq!(
        resolve_dot_path(&json, "telemetry.enabled"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        resolve_dot_path(&json, "telemetry.remote"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(resolve_dot_path(&json, "nonexistent"), None);
    assert_eq!(resolve_dot_path(&json, "telemetry.nonexistent"), None);
}

#[test]
fn test_set_dot_path() {
    let mut json = serde_json::json!({
        "tier": "free",
        "telemetry": {
            "enabled": true,
            "remote": true,
        }
    });

    assert!(set_dot_path(&mut json, "tier", serde_json::json!("team")));
    assert_eq!(json["tier"], serde_json::json!("team"));

    assert!(set_dot_path(
        &mut json,
        "telemetry.enabled",
        serde_json::json!(false)
    ));
    assert_eq!(json["telemetry"]["enabled"], serde_json::json!(false));

    assert!(!set_dot_path(
        &mut json,
        "nonexistent",
        serde_json::json!("x")
    ));
    assert!(!set_dot_path(
        &mut json,
        "telemetry.nonexistent",
        serde_json::json!("x")
    ));
}

#[test]
fn test_run_dump_config() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();

    let config = keel_core::config::KeelConfig::default();
    std::fs::write(
        keel_dir.join("keel.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();

    // dump_config should succeed
    let result = dump_config(&keel_dir.join("keel.json"));
    assert_eq!(result, 0);
}

#[test]
fn test_run_get_set_config() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();

    let config = keel_core::config::KeelConfig::default();
    let config_path = keel_dir.join("keel.json");
    std::fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    // Get tier
    let result = get_config(&config_path, "tier");
    assert_eq!(result, 0);

    // Set tier
    let result = set_config(&config_path, "tier", "team");
    assert_eq!(result, 0);

    // Verify written
    let updated: keel_core::config::KeelConfig =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(updated.tier, keel_core::config::Tier::Team);
}
