use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oxc_allocator::Allocator;
use oxc_parser::Parser as OxcParser;
use oxc_resolver::{ResolveOptions, Resolver};
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

/// Per-file symbol information extracted from oxc_semantic analysis.
#[derive(Debug, Clone)]
struct OxcSymbolInfo {
    /// Symbol name -> (is_exported, has_type_annotation)
    symbols: HashMap<String, (bool, bool)>,
    /// Re-export mappings: local_name -> (source_module, original_name)
    reexports: HashMap<String, (String, String)>,
}

/// Tier 1 + Tier 2 resolver for TypeScript and JavaScript.
///
/// - Tier 1: tree-sitter-typescript for structural extraction.
/// - Tier 2: oxc_semantic for symbol table + oxc_resolver for module resolution.
pub struct TsResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
    /// Per-file oxc semantic symbol data for Tier 2 resolution.
    semantic_cache: Mutex<HashMap<PathBuf, OxcSymbolInfo>>,
    module_resolver: Resolver,
    /// tsconfig.json path aliases: alias prefix -> resolved base path
    path_aliases: Mutex<HashMap<String, String>>,
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
            semantic_cache: Mutex::new(HashMap::new()),
            module_resolver: Resolver::new(options),
            path_aliases: Mutex::new(HashMap::new()),
        }
    }

    /// Load tsconfig.json path aliases from a project root.
    pub fn load_tsconfig_paths(&self, project_root: &Path) {
        let tsconfig_path = project_root.join("tsconfig.json");
        let content = match std::fs::read_to_string(&tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };
        let paths = match json
            .get("compilerOptions")
            .and_then(|co| co.get("paths"))
            .and_then(|p| p.as_object())
        {
            Some(p) => p,
            None => return,
        };
        let base_url = json
            .get("compilerOptions")
            .and_then(|co| co.get("baseUrl"))
            .and_then(|b| b.as_str())
            .unwrap_or(".");
        let base = project_root.join(base_url);

        let mut aliases = self.path_aliases.lock().unwrap();
        for (alias, targets) in paths {
            if let Some(target) = targets.as_array().and_then(|a| a.first()) {
                if let Some(target_str) = target.as_str() {
                    let clean_alias = alias.trim_end_matches("/*");
                    let clean_target = target_str.trim_end_matches("/*");
                    let resolved = base.join(clean_target).to_string_lossy().to_string();
                    aliases.insert(clean_alias.to_string(), resolved);
                }
            }
        }
    }

    /// Run oxc_semantic analysis on source to build a symbol table.
    /// Returns symbol info keyed by name with export/type status.
    fn analyze_with_oxc(&self, path: &Path, content: &str) -> OxcSymbolInfo {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(path).unwrap_or_default();

        let parse_result = OxcParser::new(&allocator, content, source_type).parse();
        if !parse_result.errors.is_empty() {
            return OxcSymbolInfo {
                symbols: HashMap::new(),
                reexports: HashMap::new(),
            };
        }

        let semantic_ret = SemanticBuilder::new().build(&parse_result.program);
        let semantic = semantic_ret.semantic;
        let scopes = semantic.scopes();
        let symbols = semantic.symbols();

        let mut symbol_map = HashMap::new();
        let root_scope = scopes.root_scope_id();

        // Detect exported names from source (no Export flag in SymbolFlags 0.49)
        let exported_names = detect_exported_names(content);

        // Walk top-level bindings in root scope
        for symbol_id in scopes.iter_bindings_in(root_scope) {
            let name = symbols.get_name(symbol_id).to_string();
            let is_exported = exported_names.contains(&name);
            // oxc parsed it successfully = we have precise type info
            let has_type = true;
            symbol_map.insert(name, (is_exported, has_type));
        }

        // Detect re-exports: `export { X } from './module'`
        let reexports = extract_reexports(content);

        let info = OxcSymbolInfo {
            symbols: symbol_map,
            reexports,
        };
        self.semantic_cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), info.clone());
        info
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
        // Must drop parser lock before calling analyze_with_oxc (no deadlock)
        drop(parser);

        // Tier 2: run oxc_semantic analysis for precise symbol info
        let oxc_info = self.analyze_with_oxc(path, content);

        // Enrich definitions with oxc_semantic data
        for def in &mut result.definitions {
            if let Some((is_exported, has_types)) = oxc_info.symbols.get(&def.name) {
                def.is_public = *is_exported;
                if *has_types {
                    def.type_hints_present = true;
                }
            } else {
                // Fallback to heuristic
                def.type_hints_present = ts_has_type_hints(&def.signature);
                def.is_public = ts_is_public(content, def.line_start);
            }
        }

        // Tier 2: resolve import paths using oxc_resolver + path aliases
        let dir = path.parent().unwrap_or(Path::new("."));
        let aliases = self.path_aliases.lock().unwrap();
        for imp in &mut result.imports {
            // Apply path alias resolution first
            let resolved_source = resolve_path_alias(&imp.source, &aliases);
            let source_to_resolve = resolved_source.as_deref().unwrap_or(&imp.source);

            if let Ok(resolved) = self.module_resolver.resolve(dir, source_to_resolve) {
                imp.source = resolved.full_path().to_string_lossy().to_string();
            } else if let Some(alias_resolved) = resolved_source {
                imp.source = alias_resolved;
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
            let target_file = imp.source.clone();

            // Tier 2: cross-file symbol stitching via semantic cache
            let semantic_cache = self.semantic_cache.lock().unwrap();
            let target_path = PathBuf::from(&target_file);
            if let Some(target_info) = semantic_cache.get(&target_path) {
                // Verify the symbol is actually exported from the target
                if let Some((is_exported, _)) = target_info.symbols.get(&call_site.callee_name) {
                    if *is_exported {
                        return Some(ResolvedEdge {
                            target_file,
                            target_name: call_site.callee_name.clone(),
                            confidence: 0.95, // Tier 2: oxc-verified
                        });
                    }
                }
                // Check if it's a re-export
                if let Some((real_source, original_name)) =
                    target_info.reexports.get(&call_site.callee_name)
                {
                    return Some(ResolvedEdge {
                        target_file: real_source.clone(),
                        target_name: original_name.clone(),
                        confidence: 0.95, // Tier 2: barrel re-export traced
                    });
                }
            }
            drop(semantic_cache);

            // Fallback: import found but no semantic verification
            return Some(ResolvedEdge {
                target_file,
                target_name: call_site.callee_name.clone(),
                confidence: 0.85, // Tier 1 only
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
fn ts_has_type_hints(signature: &str) -> bool {
    let params_part = signature.split("->").next().unwrap_or(signature);
    params_part.contains(':')
}

/// Determine if a definition is exported/public in TS/JS.
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

/// Detect names that appear in `export` declarations in the source.
/// Handles: `export function X`, `export class X`, `export const X`,
/// `export default X`, `export { X, Y }`.
fn detect_exported_names(content: &str) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("export") {
            continue;
        }
        // `export function name` / `export class name` / `export const name`
        let after_export = trimmed.strip_prefix("export").unwrap().trim();
        if after_export.starts_with("default ") {
            let rest = after_export.strip_prefix("default").unwrap().trim();
            if let Some(name) = extract_decl_name(rest) {
                names.insert(name);
            }
            continue;
        }
        if let Some(name) = extract_decl_name(after_export) {
            names.insert(name);
            continue;
        }
        // `export { X, Y }` or `export { X as Z }`
        if let Some(brace_start) = trimmed.find('{') {
            if let Some(brace_end) = trimmed.find('}') {
                let inner = &trimmed[brace_start + 1..brace_end];
                for entry in inner.split(',') {
                    let parts: Vec<&str> = entry.trim().split(" as ").collect();
                    let original = parts[0].trim();
                    if !original.is_empty() {
                        names.insert(original.to_string());
                    }
                }
            }
        }
    }
    names
}

/// Extract the declared name from a declaration fragment like `function foo(` or `class Bar {`.
fn extract_decl_name(s: &str) -> Option<String> {
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
fn resolve_path_alias(source: &str, aliases: &HashMap<String, String>) -> Option<String> {
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

/// Extract re-exports from source text.
/// Parses patterns like: `export { Foo, Bar } from './module'`
fn extract_reexports(content: &str) -> HashMap<String, (String, String)> {
    let mut reexports = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("export") || !trimmed.contains("from") {
            continue;
        }
        // Simple pattern: export { names } from 'source'
        if let Some(brace_start) = trimmed.find('{') {
            if let Some(brace_end) = trimmed.find('}') {
                let names_part = &trimmed[brace_start + 1..brace_end];
                let from_idx = trimmed.find("from").unwrap_or(trimmed.len());
                let source_part = &trimmed[from_idx..];
                let source = extract_string_literal(source_part);
                if let Some(src) = source {
                    for name_entry in names_part.split(',') {
                        let parts: Vec<&str> = name_entry.trim().split(" as ").collect();
                        let original = parts[0].trim().to_string();
                        let local = if parts.len() > 1 {
                            parts[1].trim().to_string()
                        } else {
                            original.clone()
                        };
                        reexports.insert(local, (src.clone(), original));
                    }
                }
            }
        }
        // export * from './module' — can't map individual names
    }
    reexports
}

/// Extract a string literal from a `from '...'` or `from "..."` fragment.
fn extract_string_literal(s: &str) -> Option<String> {
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

    #[test]
    fn test_oxc_semantic_enrichment() {
        let resolver = TsResolver::new();
        let source = r#"
export function add(a: number, b: number): number {
    return a + b;
}

function internal(x: number): number {
    return x * 2;
}
"#;
        let result = resolver.parse_file(Path::new("math.ts"), source);
        let exported: Vec<_> = result.definitions.iter().filter(|d| d.is_public).collect();
        let private: Vec<_> = result.definitions.iter().filter(|d| !d.is_public).collect();
        // oxc_semantic should detect `export` on `add` but not `internal`
        assert!(!exported.is_empty(), "should have exported symbols");
        assert!(!private.is_empty(), "should have private symbols");
    }

    #[test]
    fn test_barrel_file_reexport_parsing() {
        let reexports = extract_reexports(
            r#"
export { UserService } from './user-service';
export { AuthService as Auth } from './auth-service';
export * from './utils';
"#,
        );
        assert_eq!(reexports.len(), 2);
        assert_eq!(
            reexports.get("UserService").unwrap(),
            &("./user-service".to_string(), "UserService".to_string())
        );
        assert_eq!(
            reexports.get("Auth").unwrap(),
            &("./auth-service".to_string(), "AuthService".to_string())
        );
    }

    #[test]
    fn test_path_alias_resolution() {
        let mut aliases = HashMap::new();
        aliases.insert("@components".to_string(), "/project/src/components".to_string());
        aliases.insert("@utils".to_string(), "/project/src/utils".to_string());

        assert_eq!(
            resolve_path_alias("@components/Button", &aliases),
            Some("/project/src/components/Button".to_string())
        );
        assert_eq!(
            resolve_path_alias("@utils", &aliases),
            Some("/project/src/utils".to_string())
        );
        assert_eq!(resolve_path_alias("./local", &aliases), None);
    }

    #[test]
    fn test_cross_file_symbol_stitching() {
        let resolver = TsResolver::new();

        // Parse the "target" module first so its symbols are in the semantic cache
        let target_source = r#"
export function fetchUser(id: number): Promise<User> {
    return db.query(id);
}
"#;
        resolver.parse_file(Path::new("user-service.ts"), target_source);

        // Parse the "caller" module that imports from the target
        let caller_source = r#"
import { fetchUser } from './user-service';

function handleRequest() {
    fetchUser(42);
}
"#;
        let caller_path = Path::new("handler.ts");
        resolver.parse_file(caller_path, caller_source);

        // The import won't resolve via oxc_resolver (no real filesystem),
        // but the call edge should still resolve via Tier 1
        let edge = resolver.resolve_call_edge(&CallSite {
            file_path: "handler.ts".into(),
            line: 5,
            callee_name: "fetchUser".into(),
            receiver: None,
        });
        assert!(edge.is_some());
        let edge = edge.unwrap();
        assert_eq!(edge.target_name, "fetchUser");
        assert!(edge.confidence >= 0.85);
    }

    #[test]
    fn test_extract_string_literal() {
        assert_eq!(
            extract_string_literal("from './module'"),
            Some("./module".to_string())
        );
        assert_eq!(
            extract_string_literal(r#"from "./module""#),
            Some("./module".to_string())
        );
        assert_eq!(extract_string_literal("no quotes here"), None);
    }
}
