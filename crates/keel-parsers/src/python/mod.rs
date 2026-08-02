mod helpers;
pub mod package_resolution;
mod star_imports;
pub mod ty;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseCache, ParseResult, Reference, ResolvedEdge,
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
    cache: ParseCache,
    ty_client: Option<Box<dyn ty::TyClient>>,
    /// Memoizes dotted package-chain resolution per `(caller_dir, module_path)`.
    /// Resolving many call sites in one file re-hit the same dotted imports, so
    /// without this each `(reference × dotted-import)` pair re-`stat`ed the whole
    /// filesystem chain. Instance-scoped: the filesystem is stable for a
    /// resolver's lifetime (a single map/compile run).
    pkg_chain_cache: Mutex<HashMap<(PathBuf, String), Option<PathBuf>>>,
}

impl PyResolver {
    /// Creates a new `PyResolver` with empty caches and no ty client.
    pub fn new() -> Self {
        PyResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: ParseCache::default(),
            ty_client: None,
            pkg_chain_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a `PyResolver` with a ty subprocess client for Tier 2 resolution.
    pub fn with_ty(ty_client: Box<dyn ty::TyClient>) -> Self {
        PyResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: ParseCache::default(),
            ty_client: Some(ty_client),
            pkg_chain_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a `PyResolver`, auto-detecting a `ty` subprocess on PATH for the
    /// documented Tier-2 enhancer. Falls back to heuristics-only when `ty` is
    /// not installed, so callers never need to branch on availability.
    pub fn detect() -> Self {
        match ty::RealTyClient::detect() {
            Some(client) => Self::with_ty(Box::new(client)),
            None => Self::new(),
        }
    }

    /// Returns whether a ty subprocess client is configured and available.
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

        self.cache.insert(path, result.clone());
        result
    }
}

impl crate::boundary::BoundaryLiterals for PyResolver {
    fn tier1_parser(&self) -> &Mutex<TreeSitterParser> {
        &self.parser
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
        // Keep in sync with the Python rows of `treesitter::detect_language`.
        &["py", "pyi"]
    }

    fn parse_file(&self, path: &Path, content: &str) -> ParseResult {
        self.parse_and_cache(path, content)
    }

    fn resolve_definitions(&self, file: &Path) -> Vec<Definition> {
        self.cache.definitions_for(file)
    }

    fn resolve_references(&self, file: &Path) -> Vec<Reference> {
        self.cache.references_for(file)
    }

    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge> {
        let cache = self.cache.lock();
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
                if let Some(resolved) = self.resolve_pkg_chain(&caller_dir, &imp.source) {
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

        drop(cache);

        // Tier 2 (ty): the documented Python enhancer. When a `ty` subprocess
        // is available, ask it to type-check the caller and use any definition
        // it reports for the callee. Failure/absence is isolated — resolution
        // simply falls through to the heuristic result (None).
        self.resolve_with_ty(call_site)
    }
}

impl PyResolver {
    /// Resolve a dotted package chain through the filesystem, memoized per
    /// `(caller_dir, module_path)` so repeated call sites sharing a caller and
    /// import don't re-`stat` the same chain. Negative results are cached too:
    /// the filesystem is stable for the resolver's lifetime.
    fn resolve_pkg_chain(&self, caller_dir: &Path, module_path: &str) -> Option<PathBuf> {
        let key = (caller_dir.to_path_buf(), module_path.to_string());
        if let Some(hit) = self.pkg_chain_cache.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let resolved = package_resolution::resolve_python_package_chain(caller_dir, module_path);
        self.pkg_chain_cache
            .lock()
            .unwrap()
            .insert(key, resolved.clone());
        resolved
    }

    /// Resolve a call site through the `ty` subprocess (Tier 2), if configured.
    ///
    /// Returns `None` when ty is absent, times out, errors, or reports no
    /// definition matching the callee — the caller then keeps the heuristic
    /// (unresolved) result.
    fn resolve_with_ty(&self, call_site: &CallSite) -> Option<ResolvedEdge> {
        let client = self.ty_client.as_ref()?;
        if !client.is_available() {
            return None;
        }
        let caller = PathBuf::from(&call_site.file_path);
        let bare = call_site
            .callee_name
            .rsplit(['.', ':'])
            .next()
            .unwrap_or(&call_site.callee_name);
        let result = client.check_file(&caller).ok()?;
        let def = result
            .definitions
            .iter()
            .find(|d| d.name == bare || d.name == call_site.callee_name)?;
        Some(ResolvedEdge {
            target_file: def.file_path.clone(),
            target_name: def.name.clone(),
            confidence: 0.90,
            resolution_tier: "tier2_ty".into(),
        })
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
