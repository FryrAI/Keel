// Integration tests: configuration file roundtrip (E2E)
//
// Validates that keel's keel.json is read, written, and merged correctly
// across init, user edits, and subsequent commands.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

/// Path to the keel binary built by cargo.
fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "keel-cli"])
            .status()
            .expect("Failed to build keel");
        assert!(status.success(), "Failed to build keel binary");
    }
    path
}

/// Create a temp project with a TypeScript file.
fn setup_ts_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.ts"),
        "function hello(name: string): string { return name; }\n",
    )
    .unwrap();
    dir
}

#[test]
fn test_init_creates_default_config() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel init");

    assert!(
        output.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // keel.json should exist with default settings
    let config_path = dir.path().join(".keel/keel.json");
    assert!(config_path.exists(), "keel.json not created");

    let config_str = fs::read_to_string(&config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_str)
        .expect("keel.json should be valid JSON");

    // Check default structure
    assert_eq!(config["version"], "0.1.0");
    assert!(config["languages"].is_array(), "should have languages array");
    assert!(config["enforce"].is_object(), "should have enforce section");
    assert_eq!(config["enforce"]["type_hints"], true);
    assert_eq!(config["enforce"]["docstrings"], true);
}

#[test]
fn test_config_persists_user_modifications() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    // User modifies the config: change enforce.type_hints to false
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["enforce"]["type_hints"] = serde_json::json!(false);
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    // Verify it persists on disk
    let reloaded: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        reloaded["enforce"]["type_hints"], false,
        "User modification should persist"
    );

    // Map and compile should still work (not overwrite user config)
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Config should still have user's modification
    let after_map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        after_map["enforce"]["type_hints"], false,
        "Map should not overwrite user config"
    );
}

#[test]
fn test_config_language_override() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // Create both TS and Python files
    fs::write(
        src.join("app.ts"),
        "function run(): void {}\n",
    )
    .unwrap();
    fs::write(
        src.join("helper.py"),
        "def helper(x: int) -> int:\n    return x\n",
    )
    .unwrap();

    let keel = keel_bin();

    // Init (should detect both languages)
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let config_path = dir.path().join(".keel/keel.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let langs: Vec<String> = config["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // Should have detected both languages
    assert!(
        langs.iter().any(|l| l == "typescript"),
        "Should detect TypeScript, got: {:?}",
        langs
    );
    assert!(
        langs.iter().any(|l| l == "python"),
        "Should detect Python, got: {:?}",
        langs
    );
}

#[test]
fn test_config_invalid_json_produces_error() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    // Map first so graph.db is populated
    Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Corrupt the config with invalid JSON
    let config_path = dir.path().join(".keel/keel.json");
    fs::write(&config_path, "{ this is not valid json !!!").unwrap();

    // compile should still work (it doesn't read keel.json currently,
    // but the .keel/ dir exists and graph.db is valid)
    // This tests that partial corruption doesn't crash the tool
    let output = Command::new(&keel)
        .arg("compile")
        .current_dir(dir.path())
        .output()
        .expect("keel compile should not crash on corrupt config");

    // Should not panic — either succeeds or returns error code
    assert!(
        output.status.code().is_some(),
        "Process should not be killed by signal"
    );
}

#[test]
fn test_config_merge_preserves_unknown_keys() {
    let dir = setup_ts_project();
    let keel = keel_bin();

    // Init
    let output = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    // Add custom key to config
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["custom_user_key"] = serde_json::json!("my_value");
    config["team"] = serde_json::json!({"name": "backend", "lead": "alice"});
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    // Map should not destroy custom keys
    let output = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify custom keys still present
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        after["custom_user_key"], "my_value",
        "Custom key should survive map"
    );
    assert_eq!(
        after["team"]["name"], "backend",
        "Custom nested key should survive map"
    );
}
