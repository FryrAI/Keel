use super::*;
use std::fs;

#[test]
fn test_detect_cargo_workspace() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["crates/core", "crates/cli"]
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("crates/core")).unwrap();
    fs::create_dir_all(dir.path().join("crates/cli")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::CargoWorkspace);
    assert_eq!(layout.packages.len(), 2);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"core"));
    assert!(names.contains(&"cli"));
}

#[test]
fn test_detect_npm_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "root", "workspaces": ["packages/*"] }"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("packages/web")).unwrap();
    fs::create_dir_all(dir.path().join("packages/api")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NpmWorkspaces);
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn test_detect_go_workspace() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("go.work"),
        "go 1.21\n\nuse (\n\t./svc\n\t./lib\n)\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("svc")).unwrap();
    fs::create_dir_all(dir.path().join("lib")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::GoWorkspace);
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn test_detect_no_monorepo() {
    let dir = tempfile::tempdir().unwrap();
    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::None);
    assert!(layout.packages.is_empty());
}

#[test]
fn test_detect_nx() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("nx.json"), "{}").unwrap();
    fs::create_dir_all(dir.path().join("apps/web")).unwrap();
    fs::write(dir.path().join("apps/web/project.json"), "{}").unwrap();
    fs::create_dir_all(dir.path().join("libs/shared")).unwrap();
    fs::write(dir.path().join("libs/shared/project.json"), "{}").unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NxMonorepo);
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn test_detect_lerna() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("lerna.json"),
        r#"{ "packages": ["packages/*"] }"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("packages/alpha")).unwrap();
    fs::create_dir_all(dir.path().join("packages/beta")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::LernaMonorepo);
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn test_extract_toml_array_inline() {
    let content = r#"
[workspace]
members = ["a", "b", "c"]
"#;
    let vals = helpers::extract_toml_array(content, "members").unwrap();
    assert_eq!(vals, vec!["a", "b", "c"]);
}

#[test]
fn test_extract_toml_array_multiline() {
    let content = r#"
[workspace]
members = [
    "crates/*",
    "tools/cli",
]
"#;
    let vals = helpers::extract_toml_array(content, "members").unwrap();
    assert_eq!(vals, vec!["crates/*", "tools/cli"]);
}

#[test]
fn test_detect_pnpm_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("pnpm-workspace.yaml"),
        "packages:\n  - 'apps/*'\n  - \"libs/*\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("apps/web")).unwrap();
    fs::create_dir_all(dir.path().join("libs/shared")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NpmWorkspaces);
    assert_eq!(layout.packages.len(), 2);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"web"));
    assert!(names.contains(&"shared"));
}

#[test]
fn test_pnpm_workspaces_prefers_package_json_when_present() {
    // package.json `workspaces` takes priority; pnpm-workspace.yaml is ignored
    // if the former already yields a non-empty list.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "root", "workspaces": ["packages/*"] }"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("pnpm-workspace.yaml"),
        "packages:\n  - 'apps/*'\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("packages/web")).unwrap();
    fs::create_dir_all(dir.path().join("apps/other")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NpmWorkspaces);
    assert_eq!(layout.packages.len(), 1);
    assert_eq!(layout.packages[0].name, "web");
}

#[test]
fn test_parse_pnpm_packages_yaml_quoted_unquoted_and_comments() {
    let content = r#"
# root config
packages:
  - 'apps/*'
  - "libs/*"
  # a comment inside the list
  - services/api

onlyBuiltDependencies:
  - some-dep
"#;
    let globs = helpers::parse_pnpm_packages_yaml(content);
    assert_eq!(globs, vec!["apps/*", "libs/*", "services/api"]);
}

#[test]
fn test_parse_pnpm_packages_yaml_column_zero_items() {
    // Sequence items at column 0 are valid YAML and a common style.
    let content = "packages:\n- 'apps/*'\n- libs/*\nonlyBuiltDependencies:\n- some-dep\n";
    let globs = helpers::parse_pnpm_packages_yaml(content);
    assert_eq!(globs, vec!["apps/*", "libs/*"]);
}

#[test]
fn test_parse_pnpm_packages_yaml_inline_comments() {
    let content = "packages:\n  - 'apps/*' # web apps\n  - libs/* # shared\n";
    let globs = helpers::parse_pnpm_packages_yaml(content);
    assert_eq!(globs, vec!["apps/*", "libs/*"]);
}

#[test]
fn test_parse_pnpm_packages_yaml_no_packages_key() {
    let content = "onlyBuiltDependencies:\n  - some-dep\n";
    let globs = helpers::parse_pnpm_packages_yaml(content);
    assert!(globs.is_empty());
}

#[test]
fn test_detect_nested_projects_root_less_repo() {
    // No root manifest at all — server/Cargo.toml, frontend/package.json,
    // worker/pyproject.toml one level down (mirrors the Zenzy_poc layout).
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("server")).unwrap();
    fs::write(
        dir.path().join("server/Cargo.toml"),
        "[package]\nname = \"server\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("frontend")).unwrap();
    fs::write(
        dir.path().join("frontend/package.json"),
        r#"{ "name": "frontend" }"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("worker")).unwrap();
    fs::write(
        dir.path().join("worker/pyproject.toml"),
        "[project]\nname = \"worker\"\n",
    )
    .unwrap();
    // Should be skipped even though it "looks" like a manifest dir.
    fs::create_dir_all(dir.path().join("node_modules/some-dep")).unwrap();
    fs::write(
        dir.path().join("node_modules/some-dep/package.json"),
        r#"{ "name": "some-dep" }"#,
    )
    .unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NestedProjects);
    assert_eq!(layout.packages.len(), 3);
    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"server"));
    assert!(names.contains(&"frontend"));
    assert!(names.contains(&"worker"));
    let langs: Vec<&str> = layout
        .packages
        .iter()
        .map(|p| p.language.as_str())
        .collect();
    assert!(langs.contains(&"rust"));
    assert!(langs.contains(&"typescript"));
    assert!(langs.contains(&"python"));
}

#[test]
fn test_detect_nested_projects_finds_two_levels_deep() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("apps/server")).unwrap();
    fs::write(
        dir.path().join("apps/server/Cargo.toml"),
        "[package]\nname = \"server\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("apps/frontend")).unwrap();
    fs::write(
        dir.path().join("apps/frontend/package.json"),
        r#"{ "name": "frontend" }"#,
    )
    .unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::NestedProjects);
    assert_eq!(layout.packages.len(), 2);
}

#[test]
fn test_detect_nested_projects_requires_at_least_two() {
    // A single nested manifest is just an unconventional single-project repo,
    // not a monorepo.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("server")).unwrap();
    fs::write(
        dir.path().join("server/Cargo.toml"),
        "[package]\nname = \"server\"\n",
    )
    .unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::None);
    assert!(layout.packages.is_empty());
}

#[test]
fn test_detect_nested_projects_does_not_shadow_root_cargo_workspace() {
    // Existing root Cargo workspace detection must still win outright.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"
[workspace]
members = ["crates/core", "crates/cli"]
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("crates/core")).unwrap();
    fs::create_dir_all(dir.path().join("crates/cli")).unwrap();

    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::CargoWorkspace);
    assert_eq!(layout.packages.len(), 2);
}
