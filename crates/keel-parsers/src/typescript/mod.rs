pub(crate) mod helpers;
pub(crate) mod semantic;
pub(crate) mod svelte;
pub(crate) mod tsconfig;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "frontend_tests.rs"]
mod frontend_tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
use self::tsconfig::AliasMap;

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
    /// Base path aliases loaded from the project root — the fallback layer that
    /// fills gaps a per-file (nearer) tsconfig does not define.
    path_aliases: Mutex<AliasMap>,
    /// Project root: the ceiling for nearest-tsconfig discovery. `None` until a
    /// root is supplied via [`TsResolver::with_project_root`].
    project_root: Mutex<Option<PathBuf>>,
    /// Cache: a file's directory -> the aliases in effect for it (nearest
    /// tsconfig merged over the project-root base). Keeps per-file discovery
    /// fast — the walk-and-parse happens once per directory.
    dir_aliases: Mutex<HashMap<PathBuf, Arc<AliasMap>>>,
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
                ".svelte".into(),
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
            path_aliases: Mutex::new(AliasMap::new()),
            project_root: Mutex::new(None),
            dir_aliases: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a `TsResolver` with path aliases loaded from `project_root`.
    ///
    /// Discovers `<project_root>/tsconfig.json` (or `jsconfig.json`), follows its
    /// `extends` chain and project `references`, and merges every
    /// `compilerOptions.paths` block. For SvelteKit apps without a generated
    /// `.svelte-kit/tsconfig.json`, the default `$lib -> src/lib` is registered.
    pub fn with_project_root(project_root: &Path) -> Self {
        let resolver = Self::new();
        resolver.load_tsconfig_paths(project_root);
        resolver
    }

    /// Loads tsconfig.json path aliases from a project root, including
    /// `extends` ancestors and referenced projects, and records the root as the
    /// ceiling for per-file (nearest-tsconfig) discovery.
    pub fn load_tsconfig_paths(&self, project_root: &Path) {
        let loaded = tsconfig::load_aliases(project_root);
        {
            let mut aliases = self.path_aliases.lock().unwrap();
            for (alias, target) in loaded {
                aliases.entry(alias).or_insert(target);
            }
        }
        *self.project_root.lock().unwrap() = Some(project_root.to_path_buf());
        // The root now bounds discovery: drop any aliases cached before it was
        // known so they get recomputed against the correct ceiling.
        self.dir_aliases.lock().unwrap().clear();
    }

    /// Returns the path aliases in effect for files in `dir`.
    ///
    /// Computed from the nearest tsconfig at or above `dir` (per-file
    /// discovery, bounded by the project root) merged over the project-root
    /// base, and cached per directory so repeated files in the same package are
    /// cheap.
    fn aliases_for_dir(&self, dir: &Path) -> Arc<AliasMap> {
        if let Some(hit) = self.dir_aliases.lock().unwrap().get(dir) {
            return hit.clone();
        }

        let ceiling = self.project_root.lock().unwrap().clone();
        let mut merged = tsconfig::load_aliases_for_file(dir, ceiling.as_deref());
        // Project-root aliases fill gaps the nearest tsconfig does not define.
        for (alias, target) in self.path_aliases.lock().unwrap().iter() {
            merged
                .entry(alias.clone())
                .or_insert_with(|| target.clone());
        }

        let arc = Arc::new(merged);
        self.dir_aliases
            .lock()
            .unwrap()
            .insert(dir.to_path_buf(), arc.clone());
        arc
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        // Svelte components: keep only the <script> bodies, blanked in place so
        // byte offsets and line numbers still match the original file. Keep the
        // original around to recover template-driven references afterwards.
        let is_svelte = svelte::is_svelte_file(path);
        let original_content = content;
        let svelte_source;
        let content = if is_svelte {
            svelte_source = svelte::extract_script_source(content);
            svelte_source.as_str()
        } else {
            content
        };

        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file(grammar_for_path(path), path, content) {
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

        // Svelte: the template markup was blanked before parsing, so functions
        // used only from markup (`on:click={handler}`, `{#if x}{helper()}`)
        // would read as dead. Recover them with a lexical scan of the template.
        if is_svelte {
            let defined: HashSet<String> = result
                .definitions
                .iter()
                .filter(|d| {
                    matches!(
                        d.kind,
                        keel_core::types::NodeKind::Function | keel_core::types::NodeKind::Class
                    )
                })
                .map(|d| d.name.clone())
                .collect();
            let template_refs = svelte::extract_template_references(
                original_content,
                &defined,
                &path.to_string_lossy(),
            );
            result.references.extend(template_refs);
        }

        // Tier 2: resolve import paths using oxc_resolver + path aliases. Alias
        // discovery is per-file: the nearest tsconfig at or above this file wins
        // (so monorepo packages without a root tsconfig still resolve).
        let dir = path.parent().unwrap_or(Path::new("."));
        let aliases = self.aliases_for_dir(dir);
        for imp in &mut result.imports {
            // SvelteKit virtual modules ($app/*, $env/*) are framework-provided
            // and never exist on disk — leave them as external specifiers.
            if tsconfig::is_sveltekit_framework_module(&imp.source) {
                continue;
            }

            let candidates = resolve_path_alias(&imp.source, &aliases);
            if candidates.is_empty() {
                // No alias matched: resolve the original specifier directly.
                if let Ok(resolved) = self.module_resolver.resolve(dir, &imp.source) {
                    imp.source = resolved.full_path().to_string_lossy().to_string();
                }
                continue;
            }

            // An alias matched. Try each rewritten target in declaration order,
            // keeping the first that exists on disk (oxc verifies the file).
            // This honours `paths` fallback arrays such as
            // ["src/app/*", "generated/app/*"].
            let mut resolved_on_disk = false;
            for candidate in &candidates {
                if let Ok(resolved) = self.module_resolver.resolve(dir, candidate) {
                    imp.source = resolved.full_path().to_string_lossy().to_string();
                    resolved_on_disk = true;
                    break;
                }
            }
            if !resolved_on_disk {
                // None exist yet (e.g. unbuilt generated output): fall back to
                // the first target, matching TypeScript's declaration order.
                imp.source = candidates[0].clone();
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

/// Selects the tree-sitter grammar for a TS-family file.
///
/// `.tsx` and `.jsx` need the TSX grammar — the TypeScript grammar cannot parse
/// JSX elements, and parsing them with it silently drops every definition in the
/// file. Everything else (`.ts`, `.js`, `.mjs`, `.cjs`, `.svelte` script blocks)
/// uses the TypeScript grammar.
pub(crate) fn grammar_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tsx") | Some("jsx") => "tsx",
        _ => "typescript",
    }
}

impl LanguageResolver for TsResolver {
    fn language(&self) -> &str {
        "typescript"
    }

    fn supported_extensions(&self) -> &[&str] {
        // Keep in sync with the TS-family rows of `treesitter::detect_language`.
        &[
            "ts", "mts", "cts", "tsx", "js", "mjs", "cjs", "jsx", "svelte",
        ]
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
