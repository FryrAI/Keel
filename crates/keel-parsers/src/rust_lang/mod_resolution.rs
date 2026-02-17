//! Resolve `mod foo;` declarations to filesystem paths.
//!
//! Handles:
//! - `mod foo;` → `foo.rs` (preferred, Rust 2018+) or `foo/mod.rs`
//! - `#[path = "custom.rs"] mod foo;` → `custom.rs` relative to current dir
//! - Nested mod chains through directory hierarchy

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A resolved mod declaration: module name → filesystem path.
#[derive(Debug, Clone)]
pub struct ModDeclaration {
    pub name: String,
    pub resolved_path: PathBuf,
}

/// Extract `mod foo;` declarations from Rust source and resolve to file paths.
///
/// Only extracts declarations (ending with `;`), not inline mod blocks
/// (`mod foo { ... }`). Checks for `#[path = "..."]` attribute on the
/// preceding line.
pub fn extract_mod_declarations(content: &str, file_dir: &Path) -> Vec<ModDeclaration> {
    let mut mods = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }

        // Match `mod foo;` but NOT `mod foo {` or `mod foo { ... }`
        // Also handle `pub mod foo;`
        let mod_prefix = if trimmed.starts_with("mod ") {
            Some("mod ")
        } else if trimmed.starts_with("pub mod ") {
            Some("pub mod ")
        } else if trimmed.starts_with("pub(crate) mod ") {
            Some("pub(crate) mod ")
        } else if trimmed.starts_with("pub(super) mod ") {
            Some("pub(super) mod ")
        } else {
            None
        };

        let Some(prefix) = mod_prefix else {
            continue;
        };

        // Must end with `;` (declaration, not inline block)
        if !trimmed.ends_with(';') || trimmed.contains('{') {
            continue;
        }

        let name = trimmed
            .strip_prefix(prefix)
            .unwrap()
            .strip_suffix(';')
            .unwrap()
            .trim();

        // Skip if name is empty or contains invalid characters
        if name.is_empty() || name.contains(' ') {
            continue;
        }

        // Check preceding line for #[path = "..."]
        let custom_path = if i > 0 {
            extract_path_attribute(lines[i - 1].trim())
                .map(|p| file_dir.join(p))
        } else {
            None
        };

        let resolved = if let Some(p) = custom_path {
            p
        } else {
            resolve_mod_to_path(file_dir, name)
        };

        mods.push(ModDeclaration {
            name: name.to_string(),
            resolved_path: resolved,
        });
    }

    mods
}

/// Extract path string from `#[path = "..."]` attribute.
fn extract_path_attribute(line: &str) -> Option<&str> {
    if !line.starts_with("#[path") {
        return None;
    }
    // Extract the quoted string: #[path = "custom.rs"]
    let start = line.find('"')? + 1;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Resolve a module name to a filesystem path.
/// Prefers `name.rs` over `name/mod.rs` (Rust 2018+ convention).
fn resolve_mod_to_path(dir: &Path, name: &str) -> PathBuf {
    let as_file = dir.join(format!("{name}.rs"));
    let as_dir = dir.join(name).join("mod.rs");

    if as_file.exists() {
        as_file
    } else if as_dir.exists() {
        as_dir
    } else {
        // Default to file form even if it doesn't exist yet
        as_file
    }
}

/// Build a lookup map from module name to resolved file path.
pub fn build_mod_path_map(
    content: &str,
    file_dir: &Path,
) -> HashMap<String, PathBuf> {
    extract_mod_declarations(content, file_dir)
        .into_iter()
        .map(|m| (m.name, m.resolved_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_simple_mod() {
        let content = "mod parser;\nfn main() {}";
        let mods = extract_mod_declarations(content, Path::new("/src"));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "parser");
    }

    #[test]
    fn test_extract_pub_mod() {
        let content = "pub mod utils;\nmod internal;";
        let mods = extract_mod_declarations(content, Path::new("/src"));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "utils");
        assert_eq!(mods[1].name, "internal");
    }

    #[test]
    fn test_skip_inline_mod_block() {
        let content = "mod inline { fn foo() {} }\nmod external;";
        let mods = extract_mod_declarations(content, Path::new("/src"));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "external");
    }

    #[test]
    fn test_path_attribute_extraction() {
        assert_eq!(
            extract_path_attribute(r#"#[path = "custom.rs"]"#),
            Some("custom.rs")
        );
        assert_eq!(
            extract_path_attribute(r#"#[path = "sub/my_mod.rs"]"#),
            Some("sub/my_mod.rs")
        );
        assert_eq!(extract_path_attribute("fn foo() {}"), None);
    }

    #[test]
    fn test_resolve_mod_prefers_file() {
        let dir = std::env::temp_dir().join("keel_mod_res_prefer");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("parser")).unwrap();

        // Create both forms
        fs::write(dir.join("parser.rs"), "").unwrap();
        fs::write(dir.join("parser").join("mod.rs"), "").unwrap();

        let resolved = resolve_mod_to_path(&dir, "parser");
        assert_eq!(resolved, dir.join("parser.rs"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_mod_falls_back_to_dir() {
        let dir = std::env::temp_dir().join("keel_mod_res_fallback");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("graph")).unwrap();

        // Only dir form exists
        fs::write(dir.join("graph").join("mod.rs"), "").unwrap();

        let resolved = resolve_mod_to_path(&dir, "graph");
        assert_eq!(resolved, dir.join("graph").join("mod.rs"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_skip_comments() {
        let content = "// mod commented;\nmod real;";
        let mods = extract_mod_declarations(content, Path::new("/src"));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "real");
    }
}
