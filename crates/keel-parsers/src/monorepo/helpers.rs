//! Shared helper functions for monorepo detection.

use std::fs;
use std::path::Path;

use super::{MonorepoKind, PackageInfo};

/// Extract a TOML array value for a given key. Handles both inline and multi-line arrays.
pub(crate) fn extract_toml_array(content: &str, key: &str) -> Option<Vec<String>> {
    let mut values = Vec::new();
    let mut in_array = false;
    let mut found_key = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if !found_key {
            if trimmed.starts_with(key) && trimmed.contains('=') {
                found_key = true;
                let after_eq = trimmed.split_once('=')?.1.trim();
                if after_eq.starts_with('[') && after_eq.ends_with(']') {
                    // Single-line array
                    parse_inline_array(after_eq, &mut values);
                    return Some(values);
                } else if after_eq.starts_with('[') {
                    in_array = true;
                    // Parse any values on this line after [
                    let partial = after_eq.trim_start_matches('[');
                    parse_inline_array(
                        &format!("[{}]", partial.trim_end_matches(']')),
                        &mut values,
                    );
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
pub(crate) fn parse_inline_array(s: &str, out: &mut Vec<String>) {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    for part in inner.split(',') {
        let cleaned = part.trim().trim_matches('"').trim_matches('\'');
        if !cleaned.is_empty() {
            out.push(cleaned.to_string());
        }
    }
}

/// Expand a simple glob pattern (supports trailing `/*` and `/**`) by listing directories.
pub(crate) fn expand_glob_pattern(
    root: &Path,
    pattern: &str,
    packages: &mut Vec<PackageInfo>,
    default_language: &str,
) {
    let clean = pattern.trim_end_matches('/');
    if let Some(prefix) = clean
        .strip_suffix("/*")
        .or_else(|| clean.strip_suffix("/**"))
    {
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
            let name = clean.rsplit('/').next().unwrap_or(clean).to_string();
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
pub(crate) fn scan_for_project_json(dir: &Path, packages: &mut Vec<PackageInfo>, max_depth: u32) {
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
                scan_for_project_json(&path, packages, max_depth - 1);
            }
        }
    }
}
