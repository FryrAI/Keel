use std::path::Path;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};

/// Tier 1 + Tier 2 resolver for TypeScript and JavaScript.
///
/// - Tier 1: tree-sitter-typescript for structural extraction.
/// - Tier 2: oxc_resolver + oxc_semantic for module/type resolution.
pub struct TsResolver;

impl TsResolver {
    pub fn new() -> Self {
        TsResolver
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

    fn parse_file(&self, _path: &Path, _content: &str) -> ParseResult {
        // TODO: Implement tree-sitter TypeScript parsing (Tier 1)
        // TODO: Integrate oxc_resolver for module resolution (Tier 2)
        ParseResult {
            definitions: vec![],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
        }
    }

    fn resolve_definitions(&self, _file: &Path) -> Vec<Definition> {
        // TODO: Extract function, class, and module definitions via tree-sitter
        vec![]
    }

    fn resolve_references(&self, _file: &Path) -> Vec<Reference> {
        // TODO: Extract call-sites, import refs, and type refs via tree-sitter
        vec![]
    }

    fn resolve_call_edge(&self, _call_site: &CallSite) -> Option<ResolvedEdge> {
        // TODO: Resolve via oxc_resolver + oxc_semantic (Tier 2)
        None
    }
}
