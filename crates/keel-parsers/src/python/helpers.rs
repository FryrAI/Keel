//! Python analysis helper functions: type hints, __all__, relative imports.

use std::path::Path;

use crate::resolver::Import;

/// Check if a Python function signature has type annotations.
/// Python type hints: parameters have `: type` and return type after `->`.
pub fn py_has_type_hints(signature: &str) -> bool {
    let has_param_hints = {
        let params_part = signature.split("->").next().unwrap_or(signature);
        // Must have `:` in params (not just the function name portion)
        if let Some(paren_start) = params_part.find('(') {
            params_part[paren_start..].contains(':')
        } else {
            false
        }
    };
    let has_return_hint = signature.contains("->");
    has_param_hints && has_return_hint
}

/// Resolve a Python relative import to a file path.
/// e.g., `.foo` from `/project/pkg/bar.py` -> `/project/pkg/foo.py`
pub fn resolve_python_relative_import(dir: &Path, source: &str) -> Option<String> {
    let stripped = source.trim_start_matches('.');
    let dot_count = source.len() - stripped.len();
    if dot_count == 0 {
        return None;
    }

    let mut base = dir.to_path_buf();
    // Each extra dot beyond the first goes up one directory
    for _ in 1..dot_count {
        base = base.parent()?.to_path_buf();
    }

    if stripped.is_empty() {
        // `from . import X` — refers to __init__.py in current package
        let init = base.join("__init__.py");
        return Some(init.to_string_lossy().to_string());
    }

    // Replace dots with path separators
    let module_path = stripped.replace('.', "/");
    // Try as a module file first, then as a package
    let as_file = base.join(format!("{module_path}.py"));
    let as_pkg = base.join(&module_path).join("__init__.py");

    if as_file.exists() {
        Some(as_file.to_string_lossy().to_string())
    } else if as_pkg.exists() {
        Some(as_pkg.to_string_lossy().to_string())
    } else {
        // Return the file path even if it doesn't exist yet
        Some(as_file.to_string_lossy().to_string())
    }
}

/// Result of parsing `__all__` from a Python module.
pub enum DunderAll {
    /// `__all__` is a literal list of string names.
    Literal(Vec<String>),
    /// `__all__` is a dynamic expression (concatenation, function call, etc.).
    Dynamic,
}

/// Extract `__all__` from a Python source file using tree-sitter.
pub fn extract_python_all(
    parser: &mut crate::treesitter::TreeSitterParser,
    source: &str,
) -> Option<DunderAll> {
    let tree = parser.parse("python", source.as_bytes()).ok()?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    for i in 0..root.child_count() {
        let stmt = root.child(i)?;
        if stmt.kind() != "expression_statement" {
            continue;
        }
        let expr = stmt.child(0)?;
        if expr.kind() != "assignment" {
            continue;
        }
        let left = expr.child_by_field_name("left")?;
        if left.kind() != "identifier" || left.utf8_text(bytes).ok()? != "__all__" {
            continue;
        }
        let right = expr.child_by_field_name("right")?;
        if right.kind() == "list" {
            let mut names = Vec::new();
            for j in 0..right.named_child_count() {
                let item = right.named_child(j)?;
                if item.kind() == "string" {
                    let text = item.utf8_text(bytes).ok()?;
                    let name = text
                        .trim_matches(|c: char| c == '\'' || c == '"')
                        .to_string();
                    names.push(name);
                }
            }
            return Some(DunderAll::Literal(names));
        }
        // Non-list right-hand side — dynamic __all__
        return Some(DunderAll::Dynamic);
    }
    None
}

/// Find an import that brings `name` into scope.
pub fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports
        .iter()
        .find(|imp| imp.imported_names.iter().any(|n| n == name || n == "*"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_py_has_type_hints() {
        assert!(py_has_type_hints("greet(name: str) -> str"));
        assert!(!py_has_type_hints("greet(name)"));
        assert!(!py_has_type_hints("greet(name: str)")); // no return hint
    }

    #[test]
    fn test_resolve_relative_import() {
        let dir = Path::new("/project/pkg");
        let result = resolve_python_relative_import(dir, ".foo");
        assert!(result.is_some());
        assert!(result.unwrap().contains("foo.py"));
    }
}
