pub(crate) mod helpers;
pub(crate) mod semantic;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oxc_resolver::{ResolveOptions, Resolver};

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;

use self::helpers::{
    find_import_for_name, is_js_file, js_has_jsdoc_type_hints, resolve_path_alias,
    ts_has_type_hints, ts_is_public,
};
use self::semantic::{analyze_with_oxc, OxcSymbolInfo};

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
    /// Creates a new `TsResolver` with oxc_resolver and empty caches.
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

    /// Loads tsconfig.json path aliases from a project root, including referenced projects.
    pub fn load_tsconfig_paths(&self, project_root: &Path) {
        self.load_tsconfig_paths_inner(project_root, false);
    }

    /// Inner implementation with recursion guard. When `is_ref` is true,
    /// we skip following nested `"references"` to prevent infinite loops.
    fn load_tsconfig_paths_inner(&self, project_root: &Path, is_ref: bool) {
        let tsconfig_path = project_root.join("tsconfig.json");
        let content = match std::fs::read_to_string(&tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        // Load path aliases from compilerOptions.paths (if present)
        if let Some(paths) = json
            .get("compilerOptions")
            .and_then(|co| co.get("paths"))
            .and_then(|p| p.as_object())
        {
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
                        aliases.entry(clean_alias.to_string()).or_insert(resolved);
                    }
                }
            }
        }

        // Load project references (only from the top-level tsconfig, not recursively)
        if !is_ref {
            if let Some(refs) = json.get("references").and_then(|r| r.as_array()) {
                for reference in refs {
                    if let Some(ref_path) = reference.get("path").and_then(|p| p.as_str()) {
                        let ref_root = project_root.join(ref_path);
                        if ref_root.join("tsconfig.json").exists() {
                            self.load_tsconfig_paths_inner(&ref_root, true);
                        }
                    }
                }
            }
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("typescript", path, content) {
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
        // Must drop parser lock before calling analyze_with_oxc (no deadlock)
        drop(parser);

        // Tier 2: run oxc_semantic analysis for precise symbol info
        let oxc_info = analyze_with_oxc(path, content);

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

        // JavaScript JSDoc pass: for .js files, check if functions have
        // @param/@returns annotations in preceding JSDoc comments.
        if is_js_file(path) {
            for def in &mut result.definitions {
                if def.kind == keel_core::types::NodeKind::Function
                    && !def.type_hints_present
                    && js_has_jsdoc_type_hints(content, def.line_start as usize)
                {
                    def.type_hints_present = true;
                }
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

        // Tier 2: extract triple-slash reference directives as implicit imports
        let triple_slash_imports =
            helpers::extract_triple_slash_references(content, dir, &self.module_resolver);
        result.imports.extend(triple_slash_imports);

        // Cache semantic info
        self.semantic_cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), oxc_info);

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

    fn supported_extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx"]
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
                            resolution_tier: "tier2_oxc".into(),
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
                        resolution_tier: "tier2_oxc".into(),
                    });
                }
            }
            drop(semantic_cache);

            // Fallback: import found but no semantic verification
            return Some(ResolvedEdge {
                target_file,
                target_name: call_site.callee_name.clone(),
                confidence: 0.85, // Tier 1 only
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
