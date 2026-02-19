use std::collections::HashMap;
use std::path::Path;

use oxc_resolver::Resolver;

use crate::resolver::Import;

/// Check if a TS/JS function signature has type annotations.
pub(crate) fn ts_has_type_hints(signature: &str) -> bool {
    let params_part = signature.split("->").next().unwrap_or(signature);
    params_part.contains(':')
}

/// Determine if a definition is exported/public in TS/JS.
pub(crate) fn ts_is_public(content: &str, line_start: u32) -> bool {
    if line_start == 0 {
        return true;
    }
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line_start as usize).saturating_sub(1);
    if idx < lines.len() {
        let line = lines[idx];
        return line.contains("export ");
    }
    true
}

/// Find an import that brings `name` into scope.
pub(crate) fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports.iter().find(|imp| {
        imp.imported_names.iter().any(|n| n == name)
            || (imp.imported_names.is_empty() && imp.source.ends_with(name))
    })
}

/// Extract the declared name from a declaration fragment like `function foo(` or `class Bar {`.
pub(crate) fn extract_decl_name(s: &str) -> Option<String> {
    let s = s
        .strip_prefix("async ")
        .unwrap_or(s)
        .strip_prefix("function ")
        .or_else(|| s.strip_prefix("class "))
        .or_else(|| s.strip_prefix("const "))
        .or_else(|| s.strip_prefix("let "))
        .or_else(|| s.strip_prefix("var "))
        .or_else(|| s.strip_prefix("type "))
        .or_else(|| s.strip_prefix("interface "))
        .or_else(|| s.strip_prefix("enum "))?;
    let name: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Apply tsconfig path alias resolution.
/// E.g., `@components/Button` -> `/abs/path/src/components/Button`
pub(crate) fn resolve_path_alias(
    source: &str,
    aliases: &HashMap<String, String>,
) -> Option<String> {
    for (alias, target) in aliases {
        if source == alias {
            return Some(target.clone());
        }
        let prefix = format!("{alias}/");
        if source.starts_with(&prefix) {
            let rest = &source[prefix.len()..];
            return Some(format!("{target}/{rest}"));
        }
    }
    None
}

/// Extract `/// <reference path="..." />` directives from TypeScript source.
///
/// Triple-slash references are treated as implicit imports. The referenced
/// path is resolved via oxc_resolver (relative to the file's directory).
/// Confidence: 0.75 (reliable but older TS mechanism).
pub(crate) fn extract_triple_slash_references(
    content: &str,
    dir: &Path,
    resolver: &Resolver,
) -> Vec<Import> {
    let mut imports = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        // Only process lines at the top of the file (before any real code)
        // Triple-slash refs must appear before any other statements.
        if !trimmed.starts_with("///") && !trimmed.is_empty() && !trimmed.starts_with("//") {
            break;
        }
        if let Some(path_val) = extract_reference_path(trimmed) {
            let resolved_source = if let Ok(resolved) = resolver.resolve(dir, &path_val) {
                resolved.full_path().to_string_lossy().to_string()
            } else {
                // Fall back to joining with dir for relative paths
                dir.join(&path_val).to_string_lossy().to_string()
            };
            imports.push(Import {
                source: resolved_source,
                imported_names: vec![], // namespace-level import
                file_path: String::new(), // filled by caller
                line: (i + 1) as u32,
                is_relative: path_val.starts_with('.'),
            });
        }
    }
    imports
}

/// Parse `/// <reference path="..." />` and return the path value.
fn extract_reference_path(line: &str) -> Option<String> {
    let s = line.strip_prefix("///")?;
    let s = s.trim();
    if !s.starts_with("<reference") {
        return None;
    }
    // Find path="..." or path='...'
    let path_attr = s.find("path=")?;
    let rest = &s[path_attr + 5..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    let val = &inner[..end];
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

/// Check if a file path refers to a JavaScript (not TypeScript) file.
pub fn is_js_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("js" | "jsx" | "mjs" | "cjs") => true,
        _ => false,
    }
}

/// Check if a JS function has JSDoc type hints (`@param` or `@returns`/`@return`)
/// in a `/** ... */` comment block within the 15 lines preceding the function.
pub fn js_has_jsdoc_type_hints(source: &str, fn_line: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    // fn_line is 1-based line number
    if fn_line == 0 || fn_line > lines.len() {
        return false;
    }
    let end = fn_line - 1; // convert to 0-based, exclusive (lines before fn)
    let start = end.saturating_sub(15);

    let mut in_jsdoc = false;
    let mut found_param_or_returns = false;

    for line in &lines[start..end] {
        let trimmed = line.trim();
        if trimmed.starts_with("/**") {
            in_jsdoc = true;
            found_param_or_returns = false;
        }
        if in_jsdoc {
            if trimmed.contains("@param") || trimmed.contains("@returns") || trimmed.contains("@return ") {
                found_param_or_returns = true;
            }
        }
        if trimmed.contains("*/") {
            if in_jsdoc && found_param_or_returns {
                return true;
            }
            in_jsdoc = false;
        }
    }

    false
}

/// Extract a string literal from a `from '...'` or `from "..."` fragment.
pub(crate) fn extract_string_literal(s: &str) -> Option<String> {
    let start_single = s.find('\'');
    let start_double = s.find('"');
    match (start_single, start_double) {
        (Some(s1), Some(s2)) => {
            if s1 < s2 {
                let rest = &s[s1 + 1..];
                rest.find('\'').map(|end| rest[..end].to_string())
            } else {
                let rest = &s[s2 + 1..];
                rest.find('"').map(|end| rest[..end].to_string())
            }
        }
        (Some(s1), None) => {
            let rest = &s[s1 + 1..];
            rest.find('\'').map(|end| rest[..end].to_string())
        }
        (None, Some(s2)) => {
            let rest = &s[s2 + 1..];
            rest.find('"').map(|end| rest[..end].to_string())
        }
        (None, None) => None,
    }
}
