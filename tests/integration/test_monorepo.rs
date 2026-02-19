// Integration tests for monorepo detection and cross-package resolution.

use keel_parsers::monorepo::{detect_monorepo, MonorepoKind};
use std::fs;

/// Create a minimal Cargo workspace and verify detection + package enumeration.
#[test]
fn test_cargo_workspace_detection_and_packages() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create Cargo.toml with workspace members
    fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
members = [
    "crates/*",
]
"#,
    )
    .unwrap();

    // Create member crate directories
    fs::create_dir_all(root.join("crates/core/src")).unwrap();
    fs::create_dir_all(root.join("crates/cli/src")).unwrap();

    // Add minimal Rust source files
    fs::write(root.join("crates/core/src/lib.rs"), "pub fn core_fn() {}").unwrap();
    fs::write(root.join("crates/cli/src/main.rs"), "fn main() {}").unwrap();

    let layout = detect_monorepo(root);
    assert_eq!(layout.kind, MonorepoKind::CargoWorkspace);
    assert_eq!(layout.packages.len(), 2);

    let names: Vec<&str> = layout.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"core"));
    assert!(names.contains(&"cli"));
}

/// Create a minimal npm workspaces project and verify detection.
#[test]
fn test_npm_workspaces_detection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::write(
        root.join("package.json"),
        r#"{ "name": "mono", "workspaces": ["packages/*"] }"#,
    )
    .unwrap();

    fs::create_dir_all(root.join("packages/web/src")).unwrap();
    fs::create_dir_all(root.join("packages/api/src")).unwrap();
    fs::write(root.join("packages/web/src/index.ts"), "export {}").unwrap();
    fs::write(root.join("packages/api/src/index.ts"), "export {}").unwrap();

    let layout = detect_monorepo(root);
    assert_eq!(layout.kind, MonorepoKind::NpmWorkspaces);
    assert_eq!(layout.packages.len(), 2);
}

/// Create a go.work workspace and verify detection.
#[test]
fn test_go_workspace_detection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("svc")).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(
        root.join("go.work"),
        "go 1.21\n\nuse (\n\t./svc\n\t./lib\n)\n",
    )
    .unwrap();

    let layout = detect_monorepo(root);
    assert_eq!(layout.kind, MonorepoKind::GoWorkspace);
    assert_eq!(layout.packages.len(), 2);
}

/// A bare directory with no monorepo config returns None.
#[test]
fn test_no_monorepo_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let layout = detect_monorepo(dir.path());
    assert_eq!(layout.kind, MonorepoKind::None);
    assert!(layout.packages.is_empty());
}

/// Verify that walk_with_packages annotates files correctly.
#[test]
fn test_walk_with_packages_annotation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create npm monorepo structure
    fs::write(
        root.join("package.json"),
        r#"{ "name": "mono", "workspaces": ["packages/*"] }"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("packages/web/src")).unwrap();
    fs::create_dir_all(root.join("packages/api/src")).unwrap();
    fs::write(root.join("packages/web/src/app.ts"), "export {}").unwrap();
    fs::write(root.join("packages/api/src/main.ts"), "export {}").unwrap();
    // A root-level file (not in any package)
    fs::write(root.join("config.ts"), "export {}").unwrap();

    let layout = detect_monorepo(root);
    assert_eq!(layout.kind, MonorepoKind::NpmWorkspaces);

    let walker = keel_parsers::walker::FileWalker::new(root);
    let entries = walker.walk_with_packages(&layout);

    // Verify we found at least the 3 files
    assert!(entries.len() >= 3);

    // Check package annotations
    for entry in &entries {
        let path_str = entry.path.to_str().unwrap();
        if path_str.contains("packages/web") {
            assert_eq!(
                entry.package.as_deref(),
                Some("web"),
                "web file should have web package"
            );
        } else if path_str.contains("packages/api") {
            assert_eq!(
                entry.package.as_deref(),
                Some("api"),
                "api file should have api package"
            );
        } else if entry.path.file_name().and_then(|n| n.to_str()) == Some("config.ts") {
            assert_eq!(entry.package, None, "root file should have no package");
        }
    }
}

/// Config with monorepo section round-trips correctly.
#[test]
fn test_monorepo_config_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let config = keel_core::config::KeelConfig {
        monorepo: keel_core::config::MonorepoConfig {
            enabled: true,
            kind: Some("CargoWorkspace".to_string()),
            packages: vec!["core".to_string(), "cli".to_string()],
        },
        ..keel_core::config::KeelConfig::default()
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    fs::write(dir.path().join("keel.json"), &json).unwrap();

    let loaded = keel_core::config::KeelConfig::load(dir.path());
    assert!(loaded.monorepo.enabled);
    assert_eq!(loaded.monorepo.kind.as_deref(), Some("CargoWorkspace"));
    assert_eq!(loaded.monorepo.packages, vec!["core", "cli"]);
}

/// Config without monorepo section (backward compat) returns defaults.
#[test]
fn test_monorepo_config_backward_compat() {
    let dir = tempfile::tempdir().unwrap();
    let json = r#"{ "version": "0.1.0", "languages": ["rust"] }"#;
    fs::write(dir.path().join("keel.json"), json).unwrap();

    let loaded = keel_core::config::KeelConfig::load(dir.path());
    assert!(!loaded.monorepo.enabled);
    assert!(loaded.monorepo.kind.is_none());
    assert!(loaded.monorepo.packages.is_empty());
}
