mod helpers;
pub mod package_resolution;
mod star_imports;
pub mod ty;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;
use helpers::{
    extract_python_all, find_import_for_name, py_has_type_hints, resolve_python_relative_import,
    DunderAll,
};

/// Tier 1 + Tier 2 resolver for Python.
///
/// - Tier 1: tree-sitter-python for structural extraction.
/// - Tier 2: heuristic resolution (ty subprocess used when available).
pub struct PyResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
    ty_client: Option<Box<dyn ty::TyClient>>,
}

impl PyResolver {
    pub fn new() -> Self {
        PyResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
            ty_client: None,
        }
    }

    /// Create a PyResolver with a ty client for Tier 2 resolution.
    pub fn with_ty(ty_client: Box<dyn ty::TyClient>) -> Self {
        PyResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
            ty_client: Some(ty_client),
        }
    }

    /// Returns whether a ty client is configured and available.
    pub fn has_ty(&self) -> bool {
        self.ty_client.as_ref().is_some_and(|c| c.is_available())
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
                    if def.kind == keel_core::types::NodeKind::Module {
                        def.is_public = true;
                    } else {
                        def.is_public = names.contains(&def.name);
                    }
                }
                Some(DunderAll::Dynamic) | None => {
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
            if imp.imported_names.contains(&"*".to_string()) {
                return star_imports::resolve_star_import(
                    &cache,
                    &caller_result.imports,
                    &call_site.callee_name,
                    imp,
                );
            }
            return Some(ResolvedEdge {
                target_file: imp.source.clone(),
                target_name: call_site.callee_name.clone(),
                confidence: 0.80,
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

        // Tier 2: try dotted package resolution through filesystem
        let caller_dir = PathBuf::from(&call_site.file_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        for imp in &caller_result.imports {
            if imp.source.contains('.') && !imp.is_relative {
                if let Some(resolved) =
                    package_resolution::resolve_python_package_chain(&caller_dir, &imp.source)
                {
                    if imp
                        .imported_names
                        .iter()
                        .any(|n| n == &call_site.callee_name)
                    {
                        return Some(ResolvedEdge {
                            target_file: resolved.to_string_lossy().to_string(),
                            target_name: call_site.callee_name.clone(),
                            confidence: 0.70,
                            resolution_tier: "tier2".into(),
                        });
                    }
                }
            }
        }

        None
    }
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
        let funcs: Vec<_> = result
            .definitions
            .iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function)
            .collect();
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
        let funcs: Vec<_> = result
            .definitions
            .iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function)
            .collect();
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
        let funcs: Vec<_> = result
            .definitions
            .iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function)
            .collect();
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
        let funcs: Vec<_> = defs
            .iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Function)
            .collect();
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
}
