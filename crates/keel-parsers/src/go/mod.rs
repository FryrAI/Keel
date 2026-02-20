pub mod type_resolution;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};
use crate::treesitter::TreeSitterParser;
use type_resolution::InterfaceInfo;

/// Tier 1 + Tier 2 resolver for Go.
///
/// - Tier 1: tree-sitter-go for structural extraction.
/// - Tier 2: package-path heuristics, receiver methods, embeddings, interfaces.
pub struct GoResolver {
    parser: Mutex<TreeSitterParser>,
    cache: Mutex<HashMap<PathBuf, ParseResult>>,
    /// Maps type name -> vec of (method_name, is_pointer_receiver).
    type_methods: Mutex<HashMap<String, Vec<(String, bool)>>>,
    /// Maps outer struct -> vec of embedded type names.
    embeddings: Mutex<HashMap<String, Vec<String>>>,
    /// Parsed interface definitions with their method signatures.
    interfaces: Mutex<Vec<InterfaceInfo>>,
}

impl GoResolver {
    pub fn new() -> Self {
        GoResolver {
            parser: Mutex::new(TreeSitterParser::new()),
            cache: Mutex::new(HashMap::new()),
            type_methods: Mutex::new(HashMap::new()),
            embeddings: Mutex::new(HashMap::new()),
            interfaces: Mutex::new(Vec::new()),
        }
    }

    fn parse_and_cache(&self, path: &Path, content: &str) -> ParseResult {
        let mut parser = self.parser.lock().unwrap();
        let mut result = match parser.parse_file("go", path, content) {
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

        // Tier 2: enhance definitions with Go-specific analysis
        for def in &mut result.definitions {
            def.is_public = def.name.chars().next().is_some_and(|c| c.is_uppercase());
            def.type_hints_present = go_has_type_hints(&def.signature);
        }

        // Tier 2: extract type methods from receiver patterns
        let file_str = path.to_string_lossy().to_string();
        let tm = type_resolution::build_type_methods(&result, content);
        {
            let mut type_methods = self.type_methods.lock().unwrap();
            for (type_name, methods) in tm {
                type_methods.entry(type_name).or_default().extend(methods);
            }
        }

        // Tier 2: extract struct embeddings
        let emb = type_resolution::extract_embeddings(content);
        {
            let mut embeddings = self.embeddings.lock().unwrap();
            for (outer, inner_list) in emb {
                embeddings.entry(outer).or_default().extend(inner_list);
            }
        }

        // Tier 2: extract interface definitions
        let ifaces = type_resolution::extract_interfaces(&result, content, &file_str);
        {
            let mut interfaces = self.interfaces.lock().unwrap();
            interfaces.extend(ifaces);
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

    /// Resolve a receiver.method() call using type-aware heuristics.
    fn resolve_receiver_call(
        &self,
        receiver: &str,
        method_name: &str,
        file_path: &str,
    ) -> Option<ResolvedEdge> {
        let tm = self.type_methods.lock().unwrap();
        let emb = self.embeddings.lock().unwrap();
        let ifaces = self.interfaces.lock().unwrap();
        type_resolution::resolve_receiver_method(
            receiver,
            method_name,
            file_path,
            &tm,
            &emb,
            &ifaces,
        )
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

    fn supported_extensions(&self) -> &[&str] {
        &["go"]
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
        // Verify the caller file is in cache (early return if not)
        let _ = cache.get(&caller_file)?;

        let callee = &call_site.callee_name;

        // Check if this is a qualified call (pkg.Func or receiver.Method)
        if let Some(dot_pos) = callee.find('.') {
            let receiver_or_pkg = &callee[..dot_pos];
            let func_name = &callee[dot_pos + 1..];

            // First: try receiver method resolution (type-aware)
            drop(cache);
            if let Some(edge) =
                self.resolve_receiver_call(receiver_or_pkg, func_name, &call_site.file_path)
            {
                return Some(edge);
            }
            let cache = self.cache.lock().unwrap();
            let caller_result = cache.get(&caller_file)?;

            // Second: try import-based package resolution
            let import = caller_result.imports.iter().find(|imp| {
                if imp.imported_names.contains(&"_".to_string()) {
                    return false;
                }
                let alias = go_package_alias(&imp.source);
                alias == receiver_or_pkg
            });

            if let Some(imp) = import {
                let confidence = if func_name.chars().next().is_some_and(|c| c.is_lowercase()) {
                    0.40
                } else {
                    0.75
                };
                return Some(ResolvedEdge {
                    target_file: imp.source.clone(),
                    target_name: func_name.to_string(),
                    confidence,
                    resolution_tier: if confidence < 0.75 {
                        "tier2_heuristic".into()
                    } else {
                        "tier1".into()
                    },
                });
            }
            // Re-release cache before unqualified checks below
            drop(cache);
        } else {
            // Release initial lock for unqualified path
            drop(cache);
        }

        // Re-acquire for unqualified calls
        let cache = self.cache.lock().unwrap();
        let caller_result = cache.get(&caller_file)?;

        // Unqualified call -- check same file definitions first
        for def in &caller_result.definitions {
            if def.name == *callee {
                return Some(ResolvedEdge {
                    target_file: call_site.file_path.clone(),
                    target_name: callee.clone(),
                    confidence: 0.90,
                    resolution_tier: "tier1".into(),
                });
            }
        }

        // Unqualified call -- cross-file same-package resolution
        if let Some(caller_dir) = caller_file.parent() {
            for (path, result) in cache.iter() {
                if path == &caller_file {
                    continue;
                }
                if path.parent() == Some(caller_dir) {
                    for def in &result.definitions {
                        if def.name == *callee {
                            return Some(ResolvedEdge {
                                target_file: path.to_string_lossy().to_string(),
                                target_name: callee.clone(),
                                confidence: 0.80,
                                resolution_tier: "tier2_heuristic".into(),
                            });
                        }
                    }
                }
            }
        }

        // Unqualified call -- check dot imports
        for imp in &caller_result.imports {
            if imp.imported_names.contains(&".".to_string()) {
                return Some(ResolvedEdge {
                    target_file: imp.source.clone(),
                    target_name: callee.clone(),
                    confidence: 0.60,
                    resolution_tier: "tier2_heuristic".into(),
                });
            }
        }

        None
    }
}

/// Extract Go package alias from an import path.
fn go_package_alias(import_path: &str) -> &str {
    let cleaned = import_path.trim_matches('"');
    cleaned.rsplit('/').next().unwrap_or(cleaned)
}

/// Check if a Go function signature has type information.
fn go_has_type_hints(signature: &str) -> bool {
    if let Some(paren_start) = signature.find('(') {
        if let Some(paren_end) = signature[paren_start..].find(')') {
            let params = &signature[paren_start + 1..paren_start + paren_end];
            return params.is_empty()
                || params.contains(' ')
                || params.contains("int")
                || params.contains("string");
        }
    }
    false
}

#[cfg(test)]
mod tests;
