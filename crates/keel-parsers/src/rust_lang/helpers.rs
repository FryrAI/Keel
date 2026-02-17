//! Helper functions for Rust resolution: visibility, use-path resolution, imports.

use std::path::Path;

use crate::resolver::Import;

/// Check if a Rust definition at the given line is `pub`.
/// Handles `pub fn`, `pub(crate) fn`, `pub(super) fn`, `pub(in path) fn`.
pub fn rust_is_public(content: &str, line_start: u32) -> bool {
    if line_start == 0 {
        return false;
    }
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line_start as usize).saturating_sub(1);
    if idx < lines.len() {
        let line = lines[idx].trim_start();
        return line.starts_with("pub ") || line.starts_with("pub(");
    }
    false
}

/// Resolve a Rust `use` path (crate:: or super::) to a file path.
pub fn resolve_rust_use_path(dir: &Path, source: &str) -> Option<String> {
    if source.starts_with("super::") {
        let rest = source.strip_prefix("super::")?;
        let parent = dir.parent()?;
        let module_name = rest.split("::").next()?;
        let as_file = parent.join(format!("{module_name}.rs"));
        let as_mod = parent.join(module_name).join("mod.rs");
        if as_file.exists() {
            return Some(as_file.to_string_lossy().to_string());
        }
        if as_mod.exists() {
            return Some(as_mod.to_string_lossy().to_string());
        }
        return Some(as_file.to_string_lossy().to_string());
    }
    if source.starts_with("crate::") {
        let rest = source.strip_prefix("crate::")?;
        let segments: Vec<&str> = rest.split("::").collect();
        // Walk up from current dir to find src/ or project root
        let mut search_dir = dir;
        let mut project_root = None;
        loop {
            if search_dir.join("Cargo.toml").exists() {
                project_root = Some(search_dir.to_path_buf());
                break;
            }
            match search_dir.parent() {
                Some(p) if p != search_dir => search_dir = p,
                _ => break,
            }
        }
        if let Some(root) = project_root {
            let src_dir = root.join("src");
            // Try progressively shorter module paths
            for depth in (1..=segments.len().min(3)).rev() {
                let module_path = segments[..depth].join("/");
                let as_file = src_dir.join(format!("{module_path}.rs"));
                let as_mod = src_dir.join(&module_path).join("mod.rs");
                if as_file.exists() {
                    return Some(as_file.to_string_lossy().to_string());
                }
                if as_mod.exists() {
                    return Some(as_mod.to_string_lossy().to_string());
                }
            }
            // Return the best guess even if file doesn't exist yet
            let module_name = segments[0];
            let as_file = src_dir.join(format!("{module_name}.rs"));
            return Some(as_file.to_string_lossy().to_string());
        }
    }
    None
}

/// Find an import that brings `name` into scope.
pub fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports.iter().find(|imp| {
        imp.imported_names.iter().any(|n| n == name)
            || imp.source.ends_with(&format!("::{name}"))
    })
}

/// Extract `impl Trait for Type` blocks from Rust source via text heuristics.
///
/// Captures the trait name, concrete type name, and method names defined inside.
/// Uses simple line-by-line scanning -- no full AST required.
pub fn extract_trait_impls(content: &str, file_path: &str) -> Vec<super::TraitImpl> {
    let mut impls = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "impl TraitName for TypeName {" patterns
        if let Some(ti) = parse_impl_trait_for_line(trimmed) {
            let mut methods = Vec::new();
            // Scan forward for `fn name(` inside the impl block
            let mut brace_depth = 0i32;
            let mut started = false;
            for (j, line) in lines.iter().enumerate().skip(i) {
                for ch in line.chars() {
                    if ch == '{' {
                        brace_depth += 1;
                        started = true;
                    } else if ch == '}' {
                        brace_depth -= 1;
                    }
                }
                // Extract fn names from lines within the impl block
                let inner = line.trim();
                if let Some(fn_name) = extract_fn_name_from_line(inner) {
                    methods.push(fn_name);
                }
                if started && brace_depth <= 0 {
                    i = j;
                    break;
                }
            }
            impls.push(super::TraitImpl {
                trait_name: ti.0,
                type_name: ti.1,
                methods,
                file_path: file_path.to_string(),
            });
        }
        i += 1;
    }
    impls
}

/// Parse a line like `impl Foo for Bar {` and return (trait_name, type_name).
fn parse_impl_trait_for_line(line: &str) -> Option<(String, String)> {
    let s = line.strip_prefix("impl ")?.trim();
    let for_pos = s.find(" for ")?;
    let trait_part = s[..for_pos].trim();
    let rest = s[for_pos + 5..].trim();
    // Type ends at '{', '<', or 'where'
    let type_end = rest
        .find(['{', '<'])
        .or_else(|| rest.find(" where "))
        .unwrap_or(rest.len());
    let type_part = rest[..type_end].trim();
    if trait_part.is_empty() || type_part.is_empty() {
        return None;
    }
    // Strip generic params from trait name (e.g. `From<String>` -> `From`)
    let trait_name = trait_part
        .find('<')
        .map_or(trait_part, |i| &trait_part[..i]);
    Some((trait_name.to_string(), type_part.to_string()))
}

/// Extract a function name from a line like `fn foo(` or `pub fn bar(`.
fn extract_fn_name_from_line(line: &str) -> Option<String> {
    let s = line
        .strip_prefix("pub ")
        .or_else(|| line.strip_prefix("pub(crate) "))
        .unwrap_or(line);
    let s = s.strip_prefix("fn ")?;
    let name: String = s
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Extract inherent `impl Type { ... }` blocks from Rust source.
///
/// Returns a map: type_name -> vec of method_names. Only captures inherent
/// impls (NOT `impl Trait for Type`).
pub fn extract_impl_methods(
    content: &str,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "impl TypeName {" but NOT "impl Trait for TypeName {"
        if let Some(type_name) = parse_inherent_impl_line(trimmed) {
            let mut methods = Vec::new();
            let mut brace_depth = 0i32;
            let mut started = false;
            for (j, line) in lines.iter().enumerate().skip(i) {
                for ch in line.chars() {
                    if ch == '{' {
                        brace_depth += 1;
                        started = true;
                    } else if ch == '}' {
                        brace_depth -= 1;
                    }
                }
                let inner = line.trim();
                if let Some(fn_name) = extract_fn_name_from_line(inner) {
                    methods.push(fn_name);
                }
                if started && brace_depth <= 0 {
                    i = j;
                    break;
                }
            }
            map.entry(type_name).or_default().extend(methods);
        }
        i += 1;
    }
    map
}

/// Parse a line like `impl TypeName {` or `impl<T> Wrapper<T> {`
/// Returns the base type name for inherent impl blocks only.
fn parse_inherent_impl_line(line: &str) -> Option<String> {
    let s = line.strip_prefix("impl")?.trim();
    // Skip trait impls
    if s.contains(" for ") {
        return None;
    }
    // Handle generic params: `<T>` or `<T: Debug>`
    let after_generics = if s.starts_with('<') {
        skip_generics(s)?
    } else {
        s
    };
    // Type name is everything up to `{`, `where`, or end
    let type_end = after_generics
        .find('{')
        .or_else(|| after_generics.find(" where "))
        .unwrap_or(after_generics.len());
    let type_part = after_generics[..type_end].trim();
    if type_part.is_empty() {
        return None;
    }
    // Strip generic args from type name (e.g. `Wrapper<T>` -> `Wrapper`)
    let base_name = type_part
        .find('<')
        .map_or(type_part, |i| &type_part[..i]);
    Some(base_name.to_string())
}

/// Skip over `<...>` generics, returning the rest of the string.
fn skip_generics(s: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            depth -= 1;
            if depth == 0 {
                return Some(s[i + 1..].trim());
            }
        }
    }
    None
}

/// Extract derive macro names from `#[derive(Name1, Name2)]`.
/// Returns Vec<(macro_name, line_number)> with 1-based line numbers.
pub fn extract_derive_attrs(content: &str) -> Vec<(String, u32)> {
    let mut result = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#[derive(") {
            continue;
        }
        // Extract content between #[derive( and )]
        let start = trimmed.find('(').unwrap() + 1;
        let end = match trimmed.find(")]") {
            Some(e) => e,
            None => continue,
        };
        let names_str = &trimmed[start..end];
        for name in names_str.split(',') {
            let name = name.trim();
            if !name.is_empty() {
                result.push((name.to_string(), (i + 1) as u32));
            }
        }
    }
    result
}

/// Built-in attributes that should NOT be captured as macro references.
const BUILTIN_ATTRS: &[&str] = &[
    "cfg", "test", "allow", "deny", "warn", "doc", "must_use",
    "inline", "repr", "ignore", "derive", "cfg_attr", "cfg_test",
    "feature", "link", "no_mangle", "export_name", "cold", "track_caller",
];

/// Extract attribute macros from `#[path::name]` or `#[path::name(...)]`.
/// Skips built-in attrs (see BUILTIN_ATTRS).
/// Returns Vec<(macro_path, line_number)> with 1-based line numbers.
pub fn extract_attribute_macros(content: &str) -> Vec<(String, u32)> {
    let mut result = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            continue;
        }
        // Extract the attr name/path
        let after_hash = &trimmed[2..];
        // Find the end of the attr path (at '(', ']', ' ', or '=')
        let end = after_hash
            .find(['(', ']', ' ', '='])
            .unwrap_or(after_hash.len());
        let attr_path = after_hash[..end].trim();

        if attr_path.is_empty() {
            continue;
        }

        // Check if it's a built-in: compare the first segment
        let first_segment = attr_path.split("::").next().unwrap_or(attr_path);
        if BUILTIN_ATTRS.contains(&first_segment) {
            continue;
        }

        // Only capture path-based attrs (containing ::) to avoid noise
        if attr_path.contains("::") {
            result.push((attr_path.to_string(), (i + 1) as u32));
        }
    }
    result
}

/// Check if a line looks like a generic impl (contains `impl<`).
pub fn is_generic_impl(content: &str, type_name: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if (trimmed.starts_with("impl<") || trimmed.starts_with("impl <"))
            && trimmed.contains(type_name)
            && !trimmed.contains(" for ")
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_is_public() {
        assert!(rust_is_public("pub fn greet() {}", 1));
        assert!(!rust_is_public("fn internal() {}", 1));
        assert!(rust_is_public("  pub fn greet() {}", 1));
    }

    #[test]
    fn test_resolve_relative_import() {
        let dir = Path::new("/project/src");
        let result = resolve_rust_use_path(dir, "super::utils::helper");
        // Without real filesystem, returns the best-guess file path
        assert!(result.is_some());
        assert!(result.unwrap().contains("utils.rs"));
    }
}
