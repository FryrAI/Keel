//! Multi-language integration tests: discover across languages.
//!
//! Verifies that `keel discover` returns valid JSON output for functions
//! discovered from each of the four supported languages.

use std::process::Command;

use super::test_multi_lang_setup::{find_hash_by_name, init_and_map, keel_bin, setup_mixed_project};

#[test]
fn test_discover_works_across_languages() {
    let dir = setup_mixed_project();
    init_and_map(&dir);
    let keel = keel_bin();

    // Try discovering each language's function by hash
    let functions = ["add", "greet", "multiply", "divide"];
    let langs = ["TypeScript", "Python", "Go", "Rust"];

    for (func, lang) in functions.iter().zip(langs.iter()) {
        let hash = find_hash_by_name(&dir, func);
        assert!(
            hash.is_some(),
            "{lang} function '{func}' should be in graph"
        );
        let hash = hash.unwrap();

        let output = Command::new(&keel)
            .args(["discover", &hash, "--json"])
            .current_dir(dir.path())
            .output()
            .unwrap_or_else(|_| panic!("keel discover failed for {lang} {func}"));

        assert!(
            output.status.success(),
            "discover failed for {lang} {func}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "discover should produce output for {lang} {func}"
        );

        let json: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|_| panic!("discover output for {lang} {func} should be valid JSON"));
        assert_eq!(
            json["command"], "discover",
            "discover output should have command field"
        );
        assert!(
            json["target"].is_object(),
            "discover output should have target for {lang} {func}"
        );
        assert_eq!(
            json["target"]["name"], *func,
            "target name should be {func}"
        );
    }
}
