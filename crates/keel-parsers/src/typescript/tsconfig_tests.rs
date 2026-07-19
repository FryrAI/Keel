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

/// The first (primary) target registered for an alias, for assertions that care
/// only about the primary target of a `paths` entry.
fn first<'a>(aliases: &'a AliasMap, key: &str) -> Option<&'a str> {
    aliases.get(key).and_then(|v| v.first()).map(String::as_str)
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
        first(&aliases, "@shared"),
        Some(root.join("packages/shared/src").to_string_lossy().as_ref()),
        "parent alias must be inherited"
    );
    assert_eq!(
        first(&aliases, "@app"),
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
        first(&aliases, "@p"),
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
        first(&aliases, "$lib"),
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
        first(&aliases, "$lib"),
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
        first(&aliases, "@"),
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
        first(&aliases, "@x"),
        Some(root.join("root-x").to_string_lossy().as_ref()),
        "root project wins over a referenced project"
    );
    assert_eq!(
        first(&aliases, "@y"),
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

// ---------------------------------------------------------------------------
// Per-file discovery (issue #31, part 1): nearest tsconfig in a monorepo
// ---------------------------------------------------------------------------

#[test]
fn nearest_tsconfig_resolves_lib_alias_in_rootless_monorepo() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    // No root tsconfig. A per-package SvelteKit app lives under apps/web, the
    // exact shape from the issue (apps/web/tsconfig.json + svelte.config.js).
    write(
        &root.join("apps/web/svelte.config.js"),
        "export default {};",
    );
    write(
        &root.join("apps/web/tsconfig.json"),
        r#"{"compilerOptions": {}}"#,
    );

    // A file deep inside the package must see the package's $lib.
    let file_dir = root.join("apps/web/src/routes");
    std::fs::create_dir_all(&file_dir).unwrap();
    let (aliases, _visited) = load_aliases_for_file(&file_dir, Some(&root));
    assert_eq!(
        first(&aliases, "$lib"),
        Some(root.join("apps/web/src/lib").to_string_lossy().as_ref()),
        "$lib must resolve against the nearest package tsconfig, not the alias-less root"
    );
}

#[test]
fn nearest_tsconfig_prefers_the_package_over_the_repo_root() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    // Root defines @shared; the package redefines @shared and adds @local.
    write(
        &root.join("tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["root-shared/*"]}}}"#,
    );
    write(
        &root.join("apps/web/tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["pkg-shared/*"], "@local/*": ["src/*"]}}}"#,
    );

    let file_dir = root.join("apps/web/src");
    std::fs::create_dir_all(&file_dir).unwrap();
    let (aliases, _visited) = load_aliases_for_file(&file_dir, Some(&root));
    assert_eq!(
        first(&aliases, "@shared"),
        Some(root.join("apps/web/pkg-shared").to_string_lossy().as_ref()),
        "the nearest tsconfig wins over the repo root"
    );
    assert_eq!(
        first(&aliases, "@local"),
        Some(root.join("apps/web/src").to_string_lossy().as_ref())
    );
}

#[test]
fn rootless_monorepo_yields_no_aliases_outside_any_package() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("apps/web/tsconfig.json"),
        r#"{"compilerOptions": {}}"#,
    );
    // A directory that is not under any alias-declaring package.
    let outside = root.join("scripts");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(
        load_aliases_for_file(&outside, Some(&root)).0.is_empty(),
        "the walk must stop at the ceiling without inventing aliases"
    );
}

#[test]
fn walk_reports_every_visited_ancestor_for_cache_warming() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("apps/web/tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@local/*": ["src/*"]}}}"#,
    );

    let file_dir = root.join("apps/web/src/routes");
    std::fs::create_dir_all(&file_dir).unwrap();
    let (aliases, visited) = load_aliases_for_file(&file_dir, Some(&root));

    // Aliases resolve against the nearest declaring package...
    assert!(first(&aliases, "@local").is_some());
    // ...and the walk reports the whole chain down to that package, so the
    // caller can warm the cache for each intermediate directory in one pass.
    assert_eq!(
        visited,
        vec![
            root.join("apps/web/src/routes"),
            root.join("apps/web/src"),
            root.join("apps/web"),
        ],
        "visited must list start_dir down to the nearest alias-declaring dir",
    );
}

// ---------------------------------------------------------------------------
// paths fallback arrays (issue #31, part 2): register EVERY target
// ---------------------------------------------------------------------------

#[test]
fn paths_registers_all_targets_of_a_fallback_array() {
    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@app/*": ["src/app/*", "generated/app/*"]}}}"#,
    );
    let aliases = load_aliases(&root);
    assert_eq!(
        aliases.get("@app"),
        Some(&vec![
            root.join("src/app").to_string_lossy().to_string(),
            root.join("generated/app").to_string_lossy().to_string(),
        ]),
        "both targets must be registered, in declaration order"
    );
}

#[test]
fn paths_fallback_array_picks_the_target_that_exists_on_disk() {
    use crate::resolver::LanguageResolver;
    use crate::typescript::TsResolver;

    let tmp = scratch();
    let root = tmp.path().to_path_buf();
    write(
        &root.join("tsconfig.json"),
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@gen/*": ["src/app/*", "generated/app/*"]}}}"#,
    );
    // Only the SECOND target exists on disk (e.g. generated output).
    write(
        &root.join("generated/app/thing.ts"),
        "export const x = 1;\n",
    );

    let importer = root.join("src/routes/page.ts");
    write(&importer, "import { x } from '@gen/thing';\n");

    let resolver = TsResolver::with_project_root(&root);
    let content = std::fs::read_to_string(&importer).unwrap();
    let result = resolver.parse_file(&importer, &content);
    let import = result
        .imports
        .iter()
        .find(|i| i.source.contains("thing"))
        .expect("the @gen import must be extracted");
    assert!(
        import.source.contains("generated/app/thing"),
        "resolution must pick the second target that exists on disk, got {}",
        import.source
    );
    assert!(
        !import.source.contains("src/app/thing"),
        "the non-existent first target must not be fabricated, got {}",
        import.source
    );
}
