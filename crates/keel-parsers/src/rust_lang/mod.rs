use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

/// Tier 1 + Tier 2 resolver for Rust.
///
/// - Tier 1: tree-sitter-rust for structural extraction.
/// - Tier 2: heuristic resolution (no rust-analyzer for now).
pub struct RustLangResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
}

impl RustLangResolver {
    pub fn new() -> Self {
        RustLangResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("rust", path, content) {
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

        // Tier 2: enhance definitions with Rust-specific analysis
        for def in &mut result.definitions {
            def.is_public = rust_is_public(content, def.line_start);
            // Rust is statically typed — type hints always present
            def.type_hints_present = true;
        }

        // Tier 2: resolve `use` paths to potential file locations
        let dir = path.parent().unwrap_or(Path::new("."));
        for imp in &mut result.imports {
            if imp.is_relative {
                if let Some(resolved) = resolve_rust_use_path(dir, &imp.source) {
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

impl Default for RustLangResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageResolver for RustLangResolver {
    fn language(&self) -> &str {
        "rust"
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

        let callee = &call_site.callee_name;

        // Check if callee is brought in via `use` import
        let import = find_import_for_name(&caller_result.imports, callee);

        if let Some(imp) = import {
            let confidence = if imp.source.contains("::") {
                0.80 // direct use path
            } else {
                0.50 // trait method or glob import
            };
            return Some(ResolvedEdge {
                target_file: imp.source.clone(),
                target_name: callee.clone(),
                confidence,
            });
        }

        // Check qualified path calls (e.g., module::func)
        if let Some(sep_pos) = callee.rfind("::") {
            let func_name = &callee[sep_pos + 2..];
            let module_path = &callee[..sep_pos];

            // Look for matching import of the module
            let module_import = caller_result
                .imports
                .iter()
                .find(|imp| imp.source.ends_with(module_path));

            if let Some(imp) = module_import {
                return Some(ResolvedEdge {
                    target_file: imp.source.clone(),
                    target_name: func_name.to_string(),
                    confidence: 0.80,
                });
            }
        }

        // Same file definition
        for def in &caller_result.definitions {
            if def.name == *callee {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: callee.clone(),
                    confidence: 0.95,
                });
            }
        }

        None
    }
}

/// Check if a Rust definition at the given line is `pub`.
/// Handles `pub fn`, `pub(crate) fn`, `pub(super) fn`, `pub(in path) fn`.
fn rust_is_public(content: &str, line_start: u32) -> bool {
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
fn resolve_rust_use_path(dir: &Path, source: &str) -> Option<String> {
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
    // crate:: paths need project root context — return as-is
    None
}

fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports.iter().find(|imp| {
        imp.imported_names.iter().any(|n| n == name)
            || imp.source.ends_with(&format!("::{name}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_resolver_parse_function() {
        let resolver = RustLangResolver::new();
        let source = r#"
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
        let result = resolver.parse_file(Path::new("test.rs"), source);
        assert_eq!(result.definitions.len(), 1);
        assert_eq!(result.definitions[0].name, "greet");
        assert!(result.definitions[0].is_public);
        assert!(result.definitions[0].type_hints_present);
    }

    #[test]
    fn test_rust_resolver_private_function() {
        let resolver = RustLangResolver::new();
        let source = r#"
fn internal_helper(x: i32) -> i32 {
    x + 1
}
"#;
        let result = resolver.parse_file(Path::new("test.rs"), source);
        assert_eq!(result.definitions.len(), 1);
        assert!(!result.definitions[0].is_public);
    }

    #[test]
    fn test_rust_resolver_caches_results() {
        let resolver = RustLangResolver::new();
        let source = "fn hello() {}";
        let path = Path::new("cached.rs");
        resolver.parse_file(path, source);
        let defs = resolver.resolve_definitions(path);
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn test_rust_resolver_same_file_call_edge() {
        let resolver = RustLangResolver::new();
        let source = r#"
fn helper() -> i32 { 1 }
fn main() { helper(); }
"#;
        let path = Path::new("edge.rs");
        resolver.parse_file(path, source);
        let edge = resolver.resolve_call_edge(&CallSite {
            file_path: "edge.rs".into(),
            line: 3,
            callee_name: "helper".into(),
            receiver: None,
        });
        assert!(edge.is_some());
        let edge = edge.unwrap();
        assert_eq!(edge.target_name, "helper");
        assert!(edge.confidence >= 0.90);
    }

    #[test]
    fn test_rust_is_public() {
        assert!(rust_is_public("pub fn greet() {}", 1));
        assert!(!rust_is_public("fn internal() {}", 1));
        assert!(rust_is_public("  pub fn greet() {}", 1));
    }
}
