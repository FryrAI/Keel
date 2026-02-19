//! Monorepo detection and package enumeration.
//!
//! Detects Cargo workspaces, npm workspaces, Go workspaces, Nx, Turbo, and Lerna
//! monorepos by inspecting config files at the project root.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The kind of monorepo detected at the project root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MonorepoKind {
    CargoWorkspace,
    NpmWorkspaces,
    GoWorkspace,
    NxMonorepo,
    TurboMonorepo,
    LernaMonorepo,
    None,
}

impl Default for MonorepoKind {
    fn default() -> Self {
        MonorepoKind::None
    }
}

/// Metadata about a single package within a monorepo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub path: PathBuf,
    pub kind: MonorepoKind,
    pub language: String,
}

/// The overall layout of a monorepo: its kind and constituent packages.
#[derive(Debug, Clone, Default)]
pub struct MonorepoLayout {
    pub kind: MonorepoKind,
    pub packages: Vec<PackageInfo>,
}

/// Detect whether `root` is a monorepo and enumerate its packages.
///
/// Tries each detection strategy in priority order and returns the first match.
/// Returns `MonorepoLayout { kind: None, packages: [] }` if nothing matches.
pub fn detect_monorepo(root: &Path) -> MonorepoLayout {
    // Try each strategy in priority order
    if let Some(layout) = detect_cargo_workspace(root) {
        return layout;
    }
    if let Some(layout) = detect_npm_workspaces(root) {
        return layout;
    }
    if let Some(layout) = detect_go_workspace(root) {
        return layout;
    }
    if let Some(layout) = detect_nx(root) {
        return layout;
    }
    if let Some(layout) = detect_turbo(root) {
        return layout;
    }
    if let Some(layout) = detect_lerna(root) {
        return layout;
    }
    MonorepoLayout::default()
}

/// Detect Cargo workspace from `[workspace]` section in Cargo.toml.
fn detect_cargo_workspace(root: &Path) -> Option<MonorepoLayout> {
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml).ok()?;

    // Look for [workspace] section with members
    if !content.contains("[workspace]") {
        return None;
    }

    let mut packages = Vec::new();
    // Parse members from the workspace section
    // Simple line-based parsing: find members = [...] patterns
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("members") {
            // Could be single-line: members = ["a", "b"]
            // or start of multi-line array
            let after_eq = trimmed.splitn(2, '=').nth(1)?.trim();
            if after_eq.starts_with('[') {
                // Parse inline or start collecting multi-line
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
fn detect_npm_workspaces(root: &Path) -> Option<MonorepoLayout> {
    let pkg_json = root.join("package.json");
    let content = fs::read_to_string(&pkg_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let workspace_globs = match parsed.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()
        }
        Some(serde_json::Value::Object(obj)) => {
            // Yarn-style: { packages: [...] }
            obj.get("packages")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
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
fn detect_go_workspace(root: &Path) -> Option<MonorepoLayout> {
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
fn detect_nx(root: &Path) -> Option<MonorepoLayout> {
    let nx_json = root.join("nx.json");
    if !nx_json.exists() {
        return None;
    }

    // Scan for project.json files in immediate subdirectories
    let mut packages = Vec::new();
    scan_for_project_json(root, root, &mut packages, 3);

    if packages.is_empty() {
        return None;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::NxMonorepo,
        packages,
    })
}

/// Detect Turbo monorepo from turbo.json (relies on package.json workspaces).
fn detect_turbo(root: &Path) -> Option<MonorepoLayout> {
    let turbo_json = root.join("turbo.json");
    if !turbo_json.exists() {
        return None;
    }

    // Turbo relies on npm workspaces for package discovery
    let pkg_json = root.join("package.json");
    let content = fs::read_to_string(&pkg_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let workspace_globs = match parsed.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()
        }
        _ => return None,
    };

    let mut packages = Vec::new();
    for glob in &workspace_globs {
        expand_glob_pattern(root, glob, &mut packages, "typescript");
    }

    if packages.is_empty() {
        return None;
    }

    // Override the kind to Turbo
    for pkg in &mut packages {
        pkg.kind = MonorepoKind::TurboMonorepo;
    }

    Some(MonorepoLayout {
        kind: MonorepoKind::TurboMonorepo,
        packages,
    })
}

/// Detect Lerna monorepo from lerna.json.
fn detect_lerna(root: &Path) -> Option<MonorepoLayout> {
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

// --- Helpers ---

/// Extract a TOML array value for a given key. Handles both inline and multi-line arrays.
fn extract_toml_array(content: &str, key: &str) -> Option<Vec<String>> {
    let mut values = Vec::new();
    let mut in_array = false;
    let mut found_key = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if !found_key {
            if trimmed.starts_with(key) && trimmed.contains('=') {
                found_key = true;
                let after_eq = trimmed.splitn(2, '=').nth(1)?.trim();
                if after_eq.starts_with('[') && after_eq.ends_with(']') {
                    // Single-line array
                    parse_inline_array(after_eq, &mut values);
                    return Some(values);
                } else if after_eq.starts_with('[') {
                    in_array = true;
                    // Parse any values on this line after [
                    let partial = after_eq.trim_start_matches('[');
                    parse_inline_array(&format!("[{}]", partial.trim_end_matches(']')), &mut values);
                }
            }
            continue;
        }

        if in_array {
            if trimmed.starts_with(']') {
                return Some(values);
            }
            // Parse quoted strings from array lines
            let cleaned = trimmed.trim_end_matches(',').trim();
            let unquoted = cleaned.trim_matches('"');
            if !unquoted.is_empty() && !unquoted.starts_with('#') {
                values.push(unquoted.to_string());
            }
        }
    }

    if found_key && !values.is_empty() {
        Some(values)
    } else {
        None
    }
}

/// Parse a single-line TOML/JSON array like `["a", "b/*"]`.
fn parse_inline_array(s: &str, out: &mut Vec<String>) {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    for part in inner.split(',') {
        let cleaned = part.trim().trim_matches('"').trim_matches('\'');
        if !cleaned.is_empty() {
            out.push(cleaned.to_string());
        }
    }
}

/// Expand a simple glob pattern (supports trailing `/*` and `/**`) by listing directories.
fn expand_glob_pattern(
    root: &Path,
    pattern: &str,
    packages: &mut Vec<PackageInfo>,
    default_language: &str,
) {
    // Handle patterns like "crates/*", "packages/*", "apps/**"
    let clean = pattern.trim_end_matches('/');
    if let Some(prefix) = clean.strip_suffix("/*").or_else(|| clean.strip_suffix("/**")) {
        let search_dir = root.join(prefix);
        if let Ok(entries) = fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    // Skip hidden directories
                    if name.starts_with('.') {
                        continue;
                    }
                    packages.push(PackageInfo {
                        name,
                        path,
                        kind: MonorepoKind::None, // Will be overridden by caller
                        language: default_language.to_string(),
                    });
                }
            }
        }
    } else {
        // Literal directory path (e.g., "web" or "server")
        let pkg_path = root.join(clean);
        if pkg_path.is_dir() {
            let name = clean
                .rsplit('/')
                .next()
                .unwrap_or(clean)
                .to_string();
            packages.push(PackageInfo {
                name,
                path: pkg_path,
                kind: MonorepoKind::None,
                language: default_language.to_string(),
            });
        }
    }
}

/// Recursively scan for Nx `project.json` files up to `max_depth`.
fn scan_for_project_json(
    root: &Path,
    dir: &Path,
    packages: &mut Vec<PackageInfo>,
    max_depth: u32,
) {
    if max_depth == 0 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            if path.join("project.json").exists() {
                packages.push(PackageInfo {
                    name,
                    path: path.clone(),
                    kind: MonorepoKind::NxMonorepo,
                    language: "typescript".to_string(),
                });
            }
            // Don't recurse into discovered packages, but keep looking in other dirs
            if !path.join("project.json").exists() {
                scan_for_project_json(root, &path, packages, max_depth - 1);
            }
        }
    }
}

impl std::fmt::Display for MonorepoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonorepoKind::CargoWorkspace => write!(f, "Cargo workspace"),
            MonorepoKind::NpmWorkspaces => write!(f, "npm workspaces"),
            MonorepoKind::GoWorkspace => write!(f, "Go workspace"),
            MonorepoKind::NxMonorepo => write!(f, "Nx monorepo"),
            MonorepoKind::TurboMonorepo => write!(f, "Turbo monorepo"),
            MonorepoKind::LernaMonorepo => write!(f, "Lerna monorepo"),
            MonorepoKind::None => write!(f, "none"),
        }
    }
}

#[cfg(test)]
mod tests {
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
        // Just a bare directory with no config files
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
        let vals = extract_toml_array(content, "members").unwrap();
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
        let vals = extract_toml_array(content, "members").unwrap();
        assert_eq!(vals, vec!["crates/*", "tools/cli"]);
    }
}
