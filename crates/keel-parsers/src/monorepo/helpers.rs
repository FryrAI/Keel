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

/// Directory names to skip during downward scans: build output and
/// dependency caches that are never themselves a project root.
const SKIP_DIR_NAMES: [&str; 5] = ["target", "node_modules", "dist", "build", "__pycache__"];

/// Manifest file name paired with the language it implies.
const NESTED_MANIFESTS: [(&str, &str); 3] = [
    ("Cargo.toml", "rust"),
    ("package.json", "typescript"),
    ("pyproject.toml", "python"),
];

/// Recursively scan for nested project manifests (`Cargo.toml`,
/// `package.json`, `pyproject.toml`) up to `max_depth` directory levels
/// below `dir`. Hidden directories (dotfiles) and [`SKIP_DIR_NAMES`] are
/// skipped. A directory containing a manifest is recorded and not recursed
/// into further, mirroring [`scan_for_project_json`]'s stop-at-boundary
/// behavior.
pub(crate) fn scan_for_nested_manifests(
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
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') || SKIP_DIR_NAMES.contains(&name) {
            continue;
        }

        let manifest = NESTED_MANIFESTS
            .iter()
            .find(|(file_name, _)| path.join(file_name).exists());

        if let Some((_, language)) = manifest {
            packages.push(PackageInfo {
                name: name.to_string(),
                path: path.clone(),
                kind: MonorepoKind::NestedProjects,
                language: language.to_string(),
            });
            continue;
        }

        scan_for_nested_manifests(&path, packages, max_depth - 1);
    }
}

/// Parse the `packages:` block sequence from a `pnpm-workspace.yaml` file.
///
/// This is a hand-rolled parser for the constrained YAML subset pnpm
/// workspace files actually use, not a general YAML parser. It supports:
/// - A top-level `packages:` key (exact match after trimming).
/// - A following block sequence of scalar entries: `- 'glob'`, `- "glob"`,
///   or a bare `- glob`, one per line, indented or at column 0 (both are
///   valid YAML and both occur in the wild).
/// - Blank lines and full-line `#` comments anywhere, plus inline
///   ` # comment` suffixes on sequence items, which are ignored.
///
/// Parsing stops at end of input or at the first non-item line after the
/// key (the start of another key such as `onlyBuiltDependencies:`).
/// Flow sequences (`packages: [a, b]`), nested maps, anchors, and multi-line
/// scalars are NOT supported — none of these appear in real pnpm workspace
/// files.
pub(crate) fn parse_pnpm_packages_yaml(content: &str) -> Vec<String> {
    let mut globs = Vec::new();
    let mut in_packages = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if !in_packages {
            if trimmed == "packages:" {
                in_packages = true;
            }
            continue;
        }

        // Sequence items may sit at column 0 (valid YAML); any non-item line
        // after `packages:` is the next top-level key, so stop there.
        if let Some(item) = trimmed.strip_prefix('-') {
            if let Some(value) = parse_pnpm_sequence_value(item) {
                globs.push(value);
            }
        } else {
            break;
        }
    }

    globs
}

/// Extract the glob from one `- <glob>` sequence item: unwrap a
/// single/double-quoted value, or take an unquoted value up to the first
/// whitespace (globs contain none, and YAML inline comments are whitespace
/// followed by `#`). Returns `None` for empty or comment-only values.
fn parse_pnpm_sequence_value(item: &str) -> Option<String> {
    let item = item.trim();
    let value = match item.chars().next() {
        Some(q @ ('"' | '\'')) => item[1..].split(q).next().unwrap_or(""),
        _ => item.split_whitespace().next().unwrap_or(""),
    };
    (!value.is_empty() && !value.starts_with('#')).then(|| value.to_string())
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
