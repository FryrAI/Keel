// Integration tests: configuration file roundtrip (E2E)
//
// Validates that keel's config.toml is read, written, and merged correctly
// across init, user edits, and subsequent commands.
//
// use std::process::Command;
// use tempfile::TempDir;
// use std::fs;

#[test]
#[ignore = "Not yet implemented"]
fn test_init_creates_default_config() {
    // GIVEN a fresh project directory
    // WHEN `keel init` is run
    // THEN .keel/config.toml is created with default settings (languages, thresholds)
}

#[test]
#[ignore = "Not yet implemented"]
fn test_config_persists_user_modifications() {
    // GIVEN an initialized project with default config
    // WHEN the user edits config.toml to change the confidence threshold to 0.8
    // THEN subsequent `keel compile` uses the 0.8 threshold, not the default
}

#[test]
#[ignore = "Not yet implemented"]
fn test_config_language_override() {
    // GIVEN an initialized project with default config
    // WHEN the user edits config.toml to disable Python support
    // THEN `keel map` skips .py files and produces no Python nodes
}

#[test]
#[ignore = "Not yet implemented"]
fn test_config_invalid_toml_produces_error() {
    // GIVEN an initialized project with a valid config
    // WHEN config.toml is replaced with invalid TOML syntax
    // THEN `keel compile` exits with code 2 (internal error) and a clear parse error message
}

#[test]
#[ignore = "Not yet implemented"]
fn test_config_merge_preserves_unknown_keys() {
    // GIVEN an initialized project with config.toml containing user-added custom keys
    // WHEN `keel init` is re-run (re-initialization)
    // THEN the custom keys are preserved and default keys are updated if needed
}
