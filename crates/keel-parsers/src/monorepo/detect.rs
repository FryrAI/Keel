//! Detection strategies for each monorepo kind.

use std::fs;
use std::path::Path;

use super::helpers::{
    expand_glob_pattern, extract_toml_array, parse_pnpm_packages_yaml, scan_for_nested_manifests,
    scan_for_project_json,
};
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

/// Detect npm/yarn/pnpm workspaces.
///
/// Prefers the `workspaces` field in `package.json` (npm/yarn convention).
/// If that is absent or empty, falls back to `pnpm-workspace.yaml`'s
/// `packages:` list — the canonical pnpm workspace mechanism, which pnpm
/// does not require mirroring into `package.json` at all.
pub(crate) fn detect_npm_workspaces(root: &Path) -> Option<MonorepoLayout> {
    let workspace_globs = package_json_workspace_globs(root)
        .filter(|globs| !globs.is_empty())
        .or_else(|| pnpm_workspace_globs(root))?;

    // pnpm supports `!glob` exclusion entries; expand inclusions first, then
    // subtract everything an exclusion glob matches.
    let mut packages = Vec::new();
    for glob in workspace_globs.iter().filter(|g| !g.starts_with('!')) {
        expand_glob_pattern(root, glob, &mut packages, "typescript");
    }
    let mut excluded = Vec::new();
    for glob in workspace_globs.iter().filter(|g| g.starts_with('!')) {
        expand_glob_pattern(root, &glob[1..], &mut excluded, "typescript");
    }
    packages.retain(|p| !excluded.iter().any(|e| e.path == p.path));

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::NpmWorkspaces,
        packages,
    })
}

/// Read the `workspaces` field from a root `package.json`, if present.
///
/// Handles both the npm array form (`"workspaces": ["a", "b"]`) and the
/// legacy yarn object form (`"workspaces": { "packages": [...] }`).
fn package_json_workspace_globs(root: &Path) -> Option<Vec<String>> {
    let pkg_json = root.join("package.json");
    let content = fs::read_to_string(&pkg_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    match parsed.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>(),
        ),
        Some(serde_json::Value::Object(obj)) => Some(
            obj.get("packages")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

/// Read the `packages:` glob list from a root `pnpm-workspace.yaml`, if present.
fn pnpm_workspace_globs(root: &Path) -> Option<Vec<String>> {
    let yaml_path = root.join("pnpm-workspace.yaml");
    let content = fs::read_to_string(&yaml_path).ok()?;
    let globs = parse_pnpm_packages_yaml(&content);
    (!globs.is_empty()).then_some(globs)
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

/// Detect a root-less monorepo by scanning downward for nested project manifests.
///
/// Runs only after every root-level strategy above has failed to match. Some
/// real-world repos (notably ones that grew organically, e.g. `server/` +
/// `frontend/` + `worker/` added independently) never gain a root
/// `Cargo.toml [workspace]`, `package.json workspaces`, or
/// `pnpm-workspace.yaml` — but are still polyglot multi-package repos, and
/// treating them as a single flat package produces a boundary-less graph.
///
/// Scans up to two directory levels below `root` for `Cargo.toml`,
/// `package.json`, or `pyproject.toml`, skipping hidden directories and
/// common build/dependency output (`target`, `node_modules`, `dist`,
/// `build`, `__pycache__`). A directory containing a manifest is recorded as
/// a package and not recursed into further.
///
/// Requires at least two discovered packages to report
/// [`MonorepoKind::NestedProjects`] — a single nested manifest is an
/// ordinary single-language project with an unconventional root, not a
/// monorepo.
pub(crate) fn detect_nested_projects(root: &Path) -> Option<MonorepoLayout> {
    let mut packages = Vec::new();
    scan_for_nested_manifests(root, &mut packages, 2);

    if packages.len() < 2 {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::NestedProjects,
        packages,
    })
}
