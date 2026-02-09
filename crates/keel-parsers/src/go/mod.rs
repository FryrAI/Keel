use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

/// Tier 1 + Tier 2 resolver for Go.
///
/// - Tier 1: tree-sitter-go for structural extraction.
/// - Tier 2: package-path heuristics (Go's explicit package system is sufficient).
pub struct GoResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
}

impl GoResolver {
    pub fn new() -> Self {
        GoResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("go", path, content) {
            Ok(r) => r,
            Err(_) => ParseResult {
                definitions: vec![],
                references: vec![],
                imports: vec![],
                external_endpoints: vec![],
            },
        };

        // Tier 2: enhance definitions with Go-specific analysis
        for def in &mut result.definitions {
            // In Go, exported symbols start with uppercase
            def.is_public = def
                .name
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase());
            // Go is statically typed — type hints always present for typed funcs
            def.type_hints_present = go_has_type_hints(&def.signature);
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

impl Default for GoResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageResolver for GoResolver {
    fn language(&self) -> &str {
        "go"
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

        // Go call patterns: `pkg.Func()` or `Func()` (same package)
        let callee = &call_site.callee_name;

        // Check if this is a qualified call (pkg.Func)
        if let Some(dot_pos) = callee.find('.') {
            let pkg_alias = &callee[..dot_pos];
            let func_name = &callee[dot_pos + 1..];

            // Find matching import
            let import = caller_result.imports.iter().find(|imp| {
                // Import alias matches, or last segment of path matches
                let alias = go_package_alias(&imp.source);
                alias == pkg_alias
            });

            if let Some(imp) = import {
                return Some(ResolvedEdge {
                    target_file: imp.source.clone(),
                    target_name: func_name.to_string(),
                    confidence: 0.75, // imported package
                });
            }
        }

        // Unqualified call — check same file definitions
        for def in &caller_result.definitions {
            if def.name == *callee {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: callee.clone(),
                    confidence: 0.90, // same package
                });
            }
        }

        None
    }
}

/// Extract Go package alias from an import path.
/// e.g., `"fmt"` -> `"fmt"`, `"net/http"` -> `"http"`.
fn go_package_alias(import_path: &str) -> &str {
    let cleaned = import_path.trim_matches('"');
    cleaned.rsplit('/').next().unwrap_or(cleaned)
}

/// Check if a Go function signature has type information.
/// Go functions always have types if they have parameters.
fn go_has_type_hints(signature: &str) -> bool {
    // Go is statically typed — if there are params, they have types
    if let Some(paren_start) = signature.find('(') {
        if let Some(paren_end) = signature[paren_start..].find(')') {
            let params = &signature[paren_start + 1..paren_start + paren_end];
            // Empty params or params with type annotations
            return params.is_empty()
                || params.contains(' ')
                || params.contains("int")
                || params.contains("string");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_resolver_parse_function() {
        let resolver = GoResolver::new();
        let source = r#"
package main

func Greet(name string) string {
    return "Hello, " + name
}
"#;
        let result = resolver.parse_file(Path::new("test.go"), source);
        assert_eq!(result.definitions.len(), 1);
        assert_eq!(result.definitions[0].name, "Greet");
        assert!(result.definitions[0].is_public);
    }

    #[test]
    fn test_go_resolver_private_function() {
        let resolver = GoResolver::new();
        let source = r#"
package main

func greet(name string) string {
    return "Hello, " + name
}
"#;
        let result = resolver.parse_file(Path::new("test.go"), source);
        assert_eq!(result.definitions.len(), 1);
        assert!(!result.definitions[0].is_public);
    }

    #[test]
    fn test_go_resolver_caches_results() {
        let resolver = GoResolver::new();
        let source = "package main\nfunc Hello() {}";
        let path = Path::new("cached.go");
        resolver.parse_file(path, source);
        let defs = resolver.resolve_definitions(path);
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn test_go_resolver_same_file_call_edge() {
        let resolver = GoResolver::new();
        let source = r#"
package main

func helper() int { return 1 }
func main() { helper() }
"#;
        let path = Path::new("edge.go");
        resolver.parse_file(path, source);
        let edge = resolver.resolve_call_edge(&CallSite {
            file_path: "edge.go".into(),
            line: 5,
            callee_name: "helper".into(),
            receiver: None,
        });
        assert!(edge.is_some());
        let edge = edge.unwrap();
        assert_eq!(edge.target_name, "helper");
        assert!(edge.confidence >= 0.90);
    }

    #[test]
    fn test_go_package_alias() {
        assert_eq!(go_package_alias("\"fmt\""), "fmt");
        assert_eq!(go_package_alias("\"net/http\""), "http");
        assert_eq!(go_package_alias("\"github.com/user/repo/pkg\""), "pkg");
    }
}
