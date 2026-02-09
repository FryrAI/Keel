use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oxc_resolver::{ResolveOptions, Resolver};

use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

/// Tier 1 + Tier 2 resolver for TypeScript and JavaScript.
///
/// - Tier 1: tree-sitter-typescript for structural extraction.
/// - Tier 2: oxc_resolver for module resolution.
pub struct TsResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
    module_resolver: Resolver,
}

impl TsResolver {
    pub fn new() -> Self {
        let options = ResolveOptions {
            extensions: vec![
                ".ts".into(),
                ".tsx".into(),
                ".js".into(),
                ".jsx".into(),
                ".mjs".into(),
                ".cjs".into(),
                ".json".into(),
            ],
            condition_names: vec!["import".into(), "require".into(), "default".into()],
            main_fields: vec!["module".into(), "main".into()],
            ..ResolveOptions::default()
        };
        TsResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
            module_resolver: Resolver::new(options),
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("typescript", path, content) {
            Ok(r) => r,
            Err(_) => ParseResult {
                definitions: vec![],
                references: vec![],
                imports: vec![],
                external_endpoints: vec![],
            },
        };

        // Tier 2: enhance definitions with TS-specific type hint detection
        for def in &mut result.definitions {
            def.type_hints_present = ts_has_type_hints(&def.signature);
            def.is_public = ts_is_public(content, def.line_start);
        }

        // Tier 2: resolve import paths using oxc_resolver
        let dir = path.parent().unwrap_or(Path::new("."));
        for imp in &mut result.imports {
            if let Ok(resolved) = self.module_resolver.resolve(dir, &imp.source) {
                imp.source = resolved.full_path().to_string_lossy().to_string();
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

impl Default for TsResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageResolver for TsResolver {
    fn language(&self) -> &str {
        "typescript"
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

        // Find the import that brings the callee into scope
        let import = find_import_for_name(&caller_result.imports, &call_site.callee_name);

        if let Some(imp) = import {
            // Direct import — confidence 0.85
            let target_file = imp.source.clone();
            return Some(ResolvedEdge {
                target_file,
                target_name: call_site.callee_name.clone(),
                confidence: 0.85,
            });
        }

        // Check if callee is defined in the same file
        for def in &caller_result.definitions {
            if def.name == call_site.callee_name {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: call_site.callee_name.clone(),
                    confidence: 0.95,
                });
            }
        }

        None
    }
}

/// Check if a TS/JS function signature has type annotations.
/// TS type hints: parameters have `: type` and/or return type after `): type`.
fn ts_has_type_hints(signature: &str) -> bool {
    // Signature format: "name(params) -> return_type"
    // Check for `:` in params portion (before `->`)
    let params_part = signature.split("->").next().unwrap_or(signature);
    params_part.contains(':')
}

/// Determine if a definition is exported/public in TS/JS.
/// Checks for `export` keyword on or near the definition line.
fn ts_is_public(content: &str, line_start: u32) -> bool {
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
fn find_import_for_name<'a>(imports: &'a [Import], name: &str) -> Option<&'a Import> {
    imports.iter().find(|imp| {
        imp.imported_names.iter().any(|n| n == name)
            || (imp.imported_names.is_empty() && imp.source.ends_with(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_resolver_parse_function() {
        let resolver = TsResolver::new();
        let source = r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let result = resolver.parse_file(Path::new("test.ts"), source);
        assert_eq!(result.definitions.len(), 1);
        assert_eq!(result.definitions[0].name, "greet");
        assert!(result.definitions[0].type_hints_present);
        assert!(result.definitions[0].is_public);
    }

    #[test]
    fn test_ts_resolver_parse_class() {
        let resolver = TsResolver::new();
        let source = r#"
class UserService {
    getUser(id: number): User {
        return this.db.find(id);
    }
}
"#;
        let result = resolver.parse_file(Path::new("service.ts"), source);
        let classes: Vec<_> = result
            .definitions
            .iter()
            .filter(|d| d.kind == keel_core::types::NodeKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "UserService");
    }

    #[test]
    fn test_ts_resolver_caches_results() {
        let resolver = TsResolver::new();
        let source = "function hello() { return 1; }";
        let path = Path::new("cached.ts");
        resolver.parse_file(path, source);
        let defs = resolver.resolve_definitions(path);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "hello");
    }

    #[test]
    fn test_ts_resolver_same_file_call_edge() {
        let resolver = TsResolver::new();
        let source = r#"
function helper() { return 1; }
function main() { helper(); }
"#;
        let path = Path::new("edge.ts");
        resolver.parse_file(path, source);
        let edge = resolver.resolve_call_edge(&CallSite {
            file_path: "edge.ts".into(),
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
    fn test_ts_has_type_hints() {
        assert!(ts_has_type_hints("greet(name: string) -> string"));
        assert!(!ts_has_type_hints("greet(name)"));
    }
}
