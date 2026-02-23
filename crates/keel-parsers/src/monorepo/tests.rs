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
