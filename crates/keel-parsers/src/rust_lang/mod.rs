mod helpers;
pub mod mod_resolution;
pub mod trait_resolution;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ReferenceKind,
    ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;
use helpers::{find_import_for_name, resolve_rust_use_path, rust_is_public};

/// A recorded `impl Trait for Type` block, with the methods defined inside.
#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_name: String,
    pub type_name: String,
    pub methods: Vec<String>,
    pub file_path: String,
}

/// Tier 1 (tree-sitter) + Tier 2 (heuristic) resolver for Rust.
pub struct RustLangResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
    mod_paths: Mutex<HashMap<String, PathBuf>>,
    trait_impls: Mutex<Vec<TraitImpl>>,
    impl_map: Mutex<HashMap<String, Vec<String>>>,
    content_cache: Mutex<HashMap<PathBuf, String>>,
    generic_bounds: Mutex<HashMap<String, Vec<String>>>,
    supertrait_bounds: Mutex<HashMap<String, Vec<String>>>,
    associated_types: Mutex<Vec<(String, String, String)>>,
}

impl RustLangResolver {
    /// Creates a new `RustLangResolver` with empty caches.
    pub fn new() -> Self {
        RustLangResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
            mod_paths: Mutex::new(HashMap::new()),
            trait_impls: Mutex::new(Vec::new()),
            impl_map: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
            generic_bounds: Mutex::new(HashMap::new()),
            supertrait_bounds: Mutex::new(HashMap::new()),
            associated_types: Mutex::new(Vec::new()),
        }
    }

    /// Returns a snapshot of the resolved `mod foo;` declaration paths.
    pub fn get_mod_paths(&self) -> HashMap<String, PathBuf> {
        self.mod_paths.lock().unwrap().clone()
    }

    /// Returns extracted associated type implementations as `(trait, type_name, concrete_type)` triples.
    pub fn get_associated_types(&self) -> Vec<(String, String, String)> {
        self.associated_types.lock().unwrap().clone()
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

        // Tier 2: extract `mod foo;` declarations and resolve to file paths
        let mod_decls = mod_resolution::build_mod_path_map(content, dir);
        {
            let mut mod_paths = self.mod_paths.lock().unwrap();
            for (name, path_buf) in &mod_decls {
                mod_paths.insert(name.clone(), path_buf.clone());
            }
        }

        // Create import entries for mod declarations so resolve_call_edge
        // can find them when resolving `module::func` calls
        for (name, mod_path) in &mod_decls {
            result.imports.push(Import {
                source: mod_path.to_string_lossy().to_string(),
                imported_names: vec![name.clone()],
                file_path: path.to_string_lossy().to_string(),
                line: 0,
                is_relative: true,
            });
        }

        // Tier 2: extract impl blocks, generic bounds, supertraits, assoc types
        let file_str = path.to_string_lossy().to_string();
        self.trait_impls
            .lock()
            .unwrap()
            .extend(helpers::extract_trait_impls(content, &file_str));
        for (tn, ms) in helpers::extract_impl_methods(content) {
            self.impl_map
                .lock()
                .unwrap()
                .entry(tn)
                .or_default()
                .extend(ms);
        }
        {
            let mut gb = self.generic_bounds.lock().unwrap();
            for (k, v) in trait_resolution::extract_generic_bounds(content) {
                gb.entry(k).or_default().extend(v);
            }
            for (k, v) in trait_resolution::extract_where_clause_bounds(content) {
                gb.entry(k).or_default().extend(v);
            }
        }
        for (k, v) in trait_resolution::extract_supertrait_bounds(content) {
            self.supertrait_bounds
                .lock()
                .unwrap()
                .entry(k)
                .or_default()
                .extend(v);
        }
        self.associated_types
            .lock()
            .unwrap()
            .extend(trait_resolution::extract_associated_type_impls(content));

        // Tier 2: extract derive macros and attribute macros as references
        for (name, line) in helpers::extract_derive_attrs(content) {
            result.references.push(Reference {
                name,
                file_path: file_str.clone(),
                line,
                kind: ReferenceKind::TypeRef,
                resolved_to: None,
            });
        }
        for (name, line) in helpers::extract_attribute_macros(content) {
            result.references.push(Reference {
                name,
                file_path: file_str.clone(),
                line,
                kind: ReferenceKind::Call,
                resolved_to: None,
            });
        }

        // Cache raw content for cross-file analysis
        self.content_cache
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_string());

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

    fn supported_extensions(&self) -> &[&str] {
        &["rs"]
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

        // Tier 2: macro invocation resolution (callee ends with `!`)
        if callee.ends_with('!') {
            let macro_name = &callee[..callee.len() - 1];
            // Same-file macro_rules definition
            let same_file = caller_result
                .definitions
                .iter()
                .any(|d| d.name == macro_name);
            if same_file {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: macro_name.to_string(),
                    confidence: 0.60,
                    resolution_tier: "tier2".into(),
                });
            }
            // Cross-file: search all cached parse results
            for (path, pr) in cache.iter() {
                if path == &caller_file {
                    continue;
                }
                if pr.definitions.iter().any(|d| d.name == macro_name) {
                    return Some(ResolvedEdge {
                        target_file: path.to_string_lossy().to_string(),
                        target_name: macro_name.to_string(),
                        confidence: 0.50,
                        resolution_tier: "tier2".into(),
                    });
                }
            }
            return None;
        }

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
                resolution_tier: "tier1".into(),
            });
        }

        // Check qualified path calls (e.g., module::func)
        if let Some(sep_pos) = callee.rfind("::") {
            let func_name = &callee[sep_pos + 2..];
            let module_path = &callee[..sep_pos];

            // Check mod_paths first for `mod foo;` declared modules
            let mod_paths = self.mod_paths.lock().unwrap();
            if let Some(mod_file) = mod_paths.get(module_path) {
                return Some(ResolvedEdge {
                    target_file: mod_file.to_string_lossy().to_string(),
                    target_name: func_name.to_string(),
                    confidence: 0.85,
                    resolution_tier: "tier2".into(),
                });
            }
            drop(mod_paths);

            // Look for matching import of the module
            let module_import = caller_result.imports.iter().find(|imp| {
                imp.source.ends_with(module_path)
                    || imp.source.ends_with(&format!("{module_path}.rs"))
                    || imp.source.ends_with(&format!("{module_path}/mod.rs"))
            });

            if let Some(imp) = module_import {
                return Some(ResolvedEdge {
                    target_file: imp.source.clone(),
                    target_name: func_name.to_string(),
                    confidence: 0.80,
                    resolution_tier: "tier1".into(),
                });
            }
        }

        // Tier 2: generic type parameter resolution via trait bounds
        if let Some(receiver) = &call_site.receiver {
            let gb = self.generic_bounds.lock().unwrap();
            if gb.contains_key(receiver.as_str()) {
                let gb_clone = gb.clone();
                drop(gb);
                let sb = self.supertrait_bounds.lock().unwrap().clone();
                let ti = self.trait_impls.lock().unwrap().clone();
                if let Some(edge) = trait_resolution::resolve_generic_method_call(
                    receiver,
                    callee,
                    &gb_clone,
                    &HashMap::new(),
                    &ti,
                    &sb,
                    &call_site.file_path,
                ) {
                    return Some(edge);
                }
            }
        }

        // Tier 2: receiver-based resolution (method calls) -- checked before
        // generic same-file definition match to get precise confidence.
        if let Some(receiver) = &call_site.receiver {
            if receiver == "self" {
                // Resolve self.method() by checking all impl blocks
                let impl_map = self.impl_map.lock().unwrap();
                for (type_name, methods) in impl_map.iter() {
                    if methods.iter().any(|m| m == callee) {
                        let cc = self.content_cache.lock().unwrap();
                        let is_generic =
                            cc.values().any(|c| helpers::is_generic_impl(c, type_name));
                        let confidence = if is_generic { 0.60 } else { 0.85 };
                        return Some(ResolvedEdge {
                            target_file: call_site.file_path.clone(),
                            target_name: callee.clone(),
                            confidence,
                            resolution_tier: "tier2".into(),
                        });
                    }
                }
            }

            // Tier 2: trait method resolution via receiver type
            let trait_impls = self.trait_impls.lock().unwrap();
            // Concrete type: receiver matches a known impl type
            if let Some(ti) = trait_impls
                .iter()
                .find(|ti| ti.type_name == *receiver && ti.methods.iter().any(|m| m == callee))
            {
                return Some(ResolvedEdge {
                    target_file: ti.file_path.clone(),
                    target_name: callee.clone(),
                    confidence: 0.70,
                    resolution_tier: "tier2".into(),
                });
            }
            // dyn Trait: receiver looks like "dyn TraitName"
            if let Some(trait_name) = receiver.strip_prefix("dyn ") {
                let candidates: Vec<_> = trait_impls
                    .iter()
                    .filter(|ti| {
                        ti.trait_name == trait_name && ti.methods.iter().any(|m| m == callee)
                    })
                    .collect();
                if let Some(first) = candidates.first() {
                    return Some(ResolvedEdge {
                        target_file: first.file_path.clone(),
                        target_name: callee.clone(),
                        confidence: 0.40,
                        resolution_tier: "tier2".into(),
                    });
                }
            }

            // Tier 2: receiver is a known type -> check impl_map
            let impl_map = self.impl_map.lock().unwrap();
            if let Some(methods) = impl_map.get(receiver.as_str()) {
                if methods.iter().any(|m| m == callee) {
                    let cc = self.content_cache.lock().unwrap();
                    let is_generic = cc.values().any(|c| helpers::is_generic_impl(c, receiver));
                    let confidence = if is_generic { 0.60 } else { 0.80 };
                    return Some(ResolvedEdge {
                        target_file: call_site.file_path.clone(),
                        target_name: callee.clone(),
                        confidence,
                        resolution_tier: "tier2".into(),
                    });
                }
            }
        }

        // Same file definition (no receiver -- bare function call)
        for def in &caller_result.definitions {
            if def.name == *callee {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: callee.clone(),
                    confidence: 0.95,
                    resolution_tier: "tier1".into(),
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests;
