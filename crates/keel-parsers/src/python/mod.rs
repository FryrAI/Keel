use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

/// Tier 1 + Tier 2 resolver for Python.
///
/// - Tier 1: tree-sitter-python for structural extraction.
/// - Tier 2: heuristic resolution (ty subprocess used when available).
pub struct PyResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
}

impl PyResolver {
    pub fn new() -> Self {
        PyResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("python", path, content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("keel: warning: failed to parse {}: {}", path.display(), e);
                ParseResult {
                    definitions: vec![],
                    references: vec![],
                    imports: vec![],
                    external_endpoints: vec![],
                }
            }
        };

        // Extract __all__ from tree-sitter AST
        let all_exports = extract_python_all(&mut parser, content);

        // Tier 2: enhance definitions with Python-specific analysis
        for def in &mut result.definitions {
            def.type_hints_present = py_has_type_hints(&def.signature);
            match &all_exports {
                Some(DunderAll::Literal(names)) => {
                    // Module node stays public; other defs use __all__
                    if def.kind == keel_core::types::NodeKind::Module {
                        def.is_public = true;
                    } else {
                        def.is_public = names.contains(&def.name);
                    }
                }
                Some(DunderAll::Dynamic) | None => {
                    // No __all__ or dynamic __all__ — use convention
                    def.is_public = !def.name.starts_with('_');
                }
            }
        }

        // Tier 2: resolve relative imports to file paths
        let dir = path.parent().unwrap_or(Path::new("."));
        for imp in &mut result.imports {
            if imp.is_relative {
                if let Some(resolved) = resolve_python_relative_import(dir, &imp.source) {
                    imp.source = resolved;
                }
            }
        }

        self.cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), result.clone());
        result
    }

    fn get_cached(&self, path: &Path) -> Option<ParseResult> {
        self.cache.lock().unwrap().get(path).cloned()
    }
}

impl Default for PyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageResolver for PyResolver {
    fn language(&self) -> &str {
        "python"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["py"]
    }

    fn parse_file(&self, path: &Path, content: &str) -> ParseResult {
        self.parse_and_cache(path, content)
    }

    fn resolve_definitions(&self, file: &Path) -> Vec<Definition> {
        self.get_cached(file)
            .map(|r| r.definitions)
            .unwrap_or_default()
    }

    fn resolve_references(&self, file: &Path) -> Vec<Reference> {
        self.get_cached(file)
            .map(|r| r.references)
            .unwrap_or_default()
    }

    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge> {
        let cache = self.cache.lock().unwrap();
        let caller_file = PathBuf::from(&call_site.file_path);
        let caller_result = cache.get(&caller_file)?;

        // Check if callee is imported
        let import = find_import_for_name(&caller_result.imports, &call_site.callee_name);

        if let Some(imp) = import {
            let confidence = if imp.imported_names.contains(&"*".to_string()) {
                0.50 // star import — low confidence
            } else {
                0.80 // direct import
            };
            return Some(ResolvedEdge {
                target_file: imp.source.clone(),
                target_name: call_site.callee_name.clone(),
                confidence,
                resolution_tier: "tier1".into(),
            });
        }

        // Check if callee is defined in the same file
        for def in &caller_result.definitions {
            if def.name == call_site.callee_name {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: call_site.callee_name.clone(),
                    confidence: 0.95,
                    resolution_tier: "tier1".into(),
                });
            }
        }

        None
    }
}

/// Check if a Python function signature has type annotations.
/// Python type hints: parameters have `: type` and return type after `->`.
fn py_has_type_hints(signature: &str) -> bool {
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
fn resolve_python_relative_import(dir: &Path, source: &str) -> Option<String> {
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
enum DunderAll {
    /// `__all__` is a literal list of string names.
    Literal(Vec<String>),
    /// `__all__` is a dynamic expression (concatenation, function call, etc.).
    Dynamic,
}

/// Extract `__all__` from a Python source file using tree-sitter.
fn extract_python_all(
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
            // Extract string literals from the list
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

fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports.iter().find(|imp| {
        imp.imported_names.iter().any(|n| n == name || n == "*")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_py_resolver_parse_function() {
        let resolver = PyResolver::new();
        let source = r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"
"#;
        let result = resolver.parse_file(Path::new("test.py"), source);
        let funcs: Vec<_> = result.definitions.iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function).collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "greet");
        assert!(funcs[0].type_hints_present);
        assert!(funcs[0].is_public);
    }

    #[test]
    fn test_py_resolver_private_function() {
        let resolver = PyResolver::new();
        let source = r#"
def _internal_helper(x: int) -> int:
    return x + 1
"#;
        let result = resolver.parse_file(Path::new("test.py"), source);
        let funcs: Vec<_> = result.definitions.iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function).collect();
        assert_eq!(funcs.len(), 1);
        assert!(!funcs[0].is_public);
    }

    #[test]
    fn test_py_resolver_no_type_hints() {
        let resolver = PyResolver::new();
        let source = r#"
def greet(name):
    return f"Hello, {name}!"
"#;
        let result = resolver.parse_file(Path::new("test.py"), source);
        let funcs: Vec<_> = result.definitions.iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function).collect();
        assert_eq!(funcs.len(), 1);
        assert!(!funcs[0].type_hints_present);
    }

    #[test]
    fn test_py_resolver_caches_results() {
        let resolver = PyResolver::new();
        let source = "def hello(): pass";
        let path = Path::new("cached.py");
        resolver.parse_file(path, source);
        let defs = resolver.resolve_definitions(path);
        let funcs: Vec<_> = defs.iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function).collect();
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn test_py_resolver_same_file_call_edge() {
        let resolver = PyResolver::new();
        let source = r#"
def helper():
    return 1

def main():
    helper()
"#;
        let path = Path::new("edge.py");
        resolver.parse_file(path, source);
        let edge = resolver.resolve_call_edge(&CallSite {
            file_path: "edge.py".into(),
            line: 6,
            callee_name: "helper".into(),
            receiver: None,
        });
        assert!(edge.is_some());
        assert_eq!(edge.unwrap().target_name, "helper");
    }

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
