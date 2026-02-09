use std::path::Path;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};

/// Tier 1 + Tier 2 resolver for Rust.
///
/// - Tier 1: tree-sitter-rust for structural extraction.
/// - Tier 2: rust-analyzer (`ra_ap_ide`) lazy-loaded on demand.
///
/// Note: rust-analyzer has a 60s+ startup time and is only invoked for
/// ambiguous references that tree-sitter cannot resolve alone.
pub struct RustLangResolver;

impl RustLangResolver {
    pub fn new() -> Self {
        RustLangResolver
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

    fn parse_file(&self, _path: &Path, _content: &str) -> ParseResult {
        // TODO: Implement tree-sitter Rust parsing (Tier 1)
        // TODO: Integrate rust-analyzer lazy-load for ambiguous refs (Tier 2)
        ParseResult {
            definitions: vec![],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
        }
    }

    fn resolve_definitions(&self, _file: &Path) -> Vec<Definition> {
        // TODO: Extract function, struct, trait, impl, and module definitions via tree-sitter
        vec![]
    }

    fn resolve_references(&self, _file: &Path) -> Vec<Reference> {
        // TODO: Extract call-sites, use-declarations, and type refs via tree-sitter
        vec![]
    }

    fn resolve_call_edge(&self, _call_site: &CallSite) -> Option<ResolvedEdge> {
        // TODO: Resolve via rust-analyzer lazy-load (Tier 2)
        None
    }
}
