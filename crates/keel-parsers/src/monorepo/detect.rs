//! Detection strategies for each monorepo kind.

use std::fs;
use std::path::Path;

use super::helpers::{expand_glob_pattern, extract_toml_array, scan_for_project_json};
use super::{MonorepoKind, MonorepoLayout, PackageInfo};

/// Detect Cargo workspace from `[workspace]` section in Cargo.toml.
pub(crate) fn detect_cargo_workspace(root: &Path) -> Option<MonorepoLayout> {
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml).ok()?;

    if !content.contains("[workspace]") {
        return None;
    }

    let mut packages = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("members") {
            let after_eq = trimmed.split_once('=')?.1.trim();
            if after_eq.starts_with('[') {
                let members_str = extract_toml_array(&content, "members")?;
                for member_glob in members_str {
                    expand_glob_pattern(root, &member_glob, &mut packages, "rust");
                }
                break;
            }
        }
    }

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::CargoWorkspace,
        packages,
    })
}

/// Detect npm/yarn/pnpm workspaces from package.json.
pub(crate) fn detect_npm_workspaces(root: &Path) -> Option<MonorepoLayout> {
    let pkg_json = root.join("package.json");
    let content = fs::read_to_string(&pkg_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let workspace_globs = match parsed.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        Some(serde_json::Value::Object(obj)) => {
            // Yarn-style: { packages: [...] }
            obj.get("packages")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => return None,
    };

    if workspace_globs.is_empty() {
        return None;
    }

    let mut packages = Vec::new();
    for glob in &workspace_globs {
        expand_glob_pattern(root, glob, &mut packages, "typescript");
    }

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::NpmWorkspaces,
        packages,
    })
}

/// Detect Go workspace from go.work file.
pub(crate) fn detect_go_workspace(root: &Path) -> Option<MonorepoLayout> {
    let go_work = root.join("go.work");
    let content = fs::read_to_string(&go_work).ok()?;

    let mut packages = Vec::new();
    let mut in_use_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "use (" {
            in_use_block = true;
            continue;
        }
        if trimmed == ")" {
            in_use_block = false;
            continue;
        }
        if in_use_block {
            let dir = trimmed.trim_matches(|c: char| c == '"' || c.is_whitespace());
            if !dir.is_empty() && !dir.starts_with("//") {
                let pkg_path = root.join(dir);
                if pkg_path.is_dir() {
                    let name = dir.rsplit('/').next().unwrap_or(dir).to_string();
                    packages.push(PackageInfo {
                        name,
                        path: pkg_path,
                        kind: MonorepoKind::GoWorkspace,
                        language: "go".to_string(),
                    });
                }
            }
        }
        // Single-line use: `use ./mymod`
        if trimmed.starts_with("use ") && !trimmed.contains('(') {
            let dir = trimmed
                .strip_prefix("use ")
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            if !dir.is_empty() {
                let pkg_path = root.join(dir);
                if pkg_path.is_dir() {
                    let name = dir.rsplit('/').next().unwrap_or(dir).to_string();
                    packages.push(PackageInfo {
                        name,
                        path: pkg_path,
                        kind: MonorepoKind::GoWorkspace,
                        language: "go".to_string(),
                    });
                }
            }
        }
    }

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::GoWorkspace,
        packages,
    })
}

/// Detect Nx monorepo from nx.json + project.json files.
pub(crate) fn detect_nx(root: &Path) -> Option<MonorepoLayout> {
    let nx_json = root.join("nx.json");
    if !nx_json.exists() {
        return None;
    }

    let mut packages = Vec::new();
    scan_for_project_json(root, &mut packages, 3);

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::NxMonorepo,
        packages,
    })
}

/// Detect Turbo monorepo from turbo.json (relies on package.json workspaces).
pub(crate) fn detect_turbo(root: &Path) -> Option<MonorepoLayout> {
    let turbo_json = root.join("turbo.json");
    if !turbo_json.exists() {
        return None;
    }

    let pkg_json = root.join("package.json");
    let content = fs::read_to_string(&pkg_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let workspace_globs = match parsed.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        _ => return None,
    };

    let mut packages = Vec::new();
    for glob in &workspace_globs {
        expand_glob_pattern(root, glob, &mut packages, "typescript");
    }

    if packages.is_empty() {
        return None;
    }

    for pkg in &mut packages {
        pkg.kind = MonorepoKind::TurboMonorepo;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::TurboMonorepo,
        packages,
    })
}

/// Detect Lerna monorepo from lerna.json.
pub(crate) fn detect_lerna(root: &Path) -> Option<MonorepoLayout> {
    let lerna_json = root.join("lerna.json");
    let content = fs::read_to_string(&lerna_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let pkg_globs = parsed
        .get("packages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["packages/*".to_string()]);

    let mut packages = Vec::new();
    for glob in &pkg_globs {
        expand_glob_pattern(root, glob, &mut packages, "typescript");
    }

    if packages.is_empty() {
        return None;
    }

    for pkg in &mut packages {
        pkg.kind = MonorepoKind::LernaMonorepo;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::LernaMonorepo,
        packages,
    })
}
