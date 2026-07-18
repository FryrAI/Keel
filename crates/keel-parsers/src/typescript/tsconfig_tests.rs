//! Unit tests for tsconfig discovery, JSONC handling, and alias merging.

use super::*;

/// Creates a unique scratch directory, cleaned up on drop.
fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn strips_line_and_block_comments() {
    let input = r#"{
  // a line comment
  "a": 1, /* inline block */
  /* multi
     line */
  "b": 2
}"#;
    let cleaned = strip_jsonc_comments(input);
    let json: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
    assert_eq!(json["a"], 1);
    assert_eq!(json["b"], 2);
}

#[test]
fn does_not_strip_comment_markers_inside_strings() {
    let input = r#"{"url": "https://x.dev/a", "glob": "src/**/*", "esc": "a\"// b"}"#;
    let cleaned = strip_jsonc_comments(input);
    let json: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
    assert_eq!(json["url"], "https://x.dev/a");
    assert_eq!(json["glob"], "src/**/*");
    assert_eq!(json["esc"], "a\"// b");
}

#[test]
fn strips_trailing_commas() {
    let input = "{\n  \"a\": [1, 2,],\n  \"b\": {\"c\": 1,},\n}";
    let cleaned = strip_trailing_commas(&strip_jsonc_comments(input));
    let json: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
    assert_eq!(json["a"][1], 2);
    assert_eq!(json["b"]["c"], 1);
}

#[test]
fn merges_extends_chain_with_child_winning() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("tsconfig.base.json"),
        r#"{
          "compilerOptions": {
            "baseUrl": ".",
            "paths": {
              "@shared/*": ["packages/shared/src/*"],
              "@app/*": ["packages/base-app/*"]
            }
          }
        }"#,
    );
    write(
        &root.join("tsconfig.json"),
        r#"{
          // child overrides @app, inherits @shared
          "extends": "./tsconfig.base.json",
          "compilerOptions": {
            "baseUrl": ".",
            "paths": { "@app/*": ["src/app/*"] }
          }
        }"#,
    );

    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("@shared").map(String::as_str),
        Some(root.join("packages/shared/src").to_string_lossy().as_ref()),
        "parent alias must be inherited"
    );
    assert_eq!(
        aliases.get("@app").map(String::as_str),
        Some(root.join("src/app").to_string_lossy().as_ref()),
        "child alias must win over parent"
    );
}

#[test]
fn extends_resolves_relative_to_the_declaring_config() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    // Parent lives in config/, so its baseUrl "." means <root>/config.
    write(
        &root.join("config/tsconfig.base.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@p/*": ["lib/*"]}}}"#,
    );
    write(
        &root.join("tsconfig.json"),
        r#"{"extends": "./config/tsconfig.base.json"}"#,
    );

    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("@p").map(String::as_str),
        Some(root.join("config/lib").to_string_lossy().as_ref())
    );
}

#[test]
fn extends_chain_terminates_on_cycle() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("tsconfig.json"),
        r#"{"extends": "./b.json", "compilerOptions": {"paths": {"@a/*": ["a/*"]}}}"#,
    );
    write(&root.join("b.json"), r#"{"extends": "./tsconfig.json"}"#);

    // Must return rather than recurse forever.
    let aliases = load_aliases(&root);
    assert!(aliases.contains_key("@a"));
}

#[test]
fn sveltekit_generated_tsconfig_supplies_lib_alias() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(&root.join("svelte.config.js"), "export default {};");
    write(
        &root.join(".svelte-kit/tsconfig.json"),
        r#"{"compilerOptions": {"paths": {"$lib": ["../src/lib"], "$lib/*": ["../src/lib/*"]}}}"#,
    );
    write(
        &root.join("tsconfig.json"),
        r#"{"extends": "./.svelte-kit/tsconfig.json"}"#,
    );

    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("$lib").map(String::as_str),
        Some(root.join("src/lib").to_string_lossy().as_ref()),
        "$lib must normalize through the ../ in the generated config"
    );
}

#[test]
fn sveltekit_lib_falls_back_when_svelte_kit_dir_is_missing() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(&root.join("svelte.config.js"), "export default {};");
    // Fresh worktree: .svelte-kit/ has not been generated yet.
    write(
        &root.join("tsconfig.json"),
        r#"{"extends": "./.svelte-kit/tsconfig.json"}"#,
    );

    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("$lib").map(String::as_str),
        Some(root.join("src/lib").to_string_lossy().as_ref()),
        "$lib must fall back to src/lib for SvelteKit projects"
    );
}

#[test]
fn no_sveltekit_fallback_for_plain_ts_projects() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(&root.join("tsconfig.json"), r#"{"compilerOptions": {}}"#);
    assert!(!load_aliases(&root).contains_key("$lib"));
}

#[test]
fn jsconfig_is_used_when_no_tsconfig_exists() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("jsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["src/*"]}}}"#,
    );
    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("@").map(String::as_str),
        Some(root.join("src").to_string_lossy().as_ref())
    );
}

#[test]
fn project_references_contribute_aliases_without_overriding() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("tsconfig.json"),
        r#"{
          "compilerOptions": {"baseUrl": ".", "paths": {"@x/*": ["root-x/*"]}},
          "references": [{"path": "./pkg"}]
        }"#,
    );
    write(
        &root.join("pkg/tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@x/*": ["pkg-x/*"], "@y/*": ["pkg-y/*"]}}}"#,
    );

    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("@x").map(String::as_str),
        Some(root.join("root-x").to_string_lossy().as_ref()),
        "root project wins over a referenced project"
    );
    assert_eq!(
        aliases.get("@y").map(String::as_str),
        Some(root.join("pkg/pkg-y").to_string_lossy().as_ref())
    );
}

#[test]
fn missing_tsconfig_yields_no_aliases() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    assert!(load_aliases(&root).is_empty());
}

#[test]
fn malformed_tsconfig_is_ignored_not_fatal() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(&root.join("tsconfig.json"), "{ this is not json");
    assert!(load_aliases(&root).is_empty());
}

#[test]
fn recognizes_sveltekit_framework_modules() {
    assert!(is_sveltekit_framework_module("$app/navigation"));
    assert!(is_sveltekit_framework_module("$app/stores"));
    assert!(is_sveltekit_framework_module("$env/static/public"));
    assert!(is_sveltekit_framework_module("$service-worker"));
    assert!(!is_sveltekit_framework_module("$lib/foo"));
    assert!(!is_sveltekit_framework_module("$appointments/x"));
    assert!(!is_sveltekit_framework_module("./local"));
}

#[test]
fn normalize_removes_dot_segments() {
    assert_eq!(
        normalize(Path::new("/a/b/../c/./d")),
        PathBuf::from("/a/c/d")
    );
}

#[test]
fn normalize_handles_parent_dir_runs_and_root() {
    assert_eq!(
        normalize(Path::new("../../shared")),
        PathBuf::from("../../shared")
    );
    assert_eq!(
        normalize(Path::new("/repo/../../external")),
        PathBuf::from("/external")
    );
    assert_eq!(normalize(Path::new("a/b/../c")), PathBuf::from("a/c"));
    assert_eq!(normalize(Path::new("./a/./b")), PathBuf::from("a/b"));
}
