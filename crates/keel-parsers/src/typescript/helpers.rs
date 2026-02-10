use std::collections::HashMap;

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
