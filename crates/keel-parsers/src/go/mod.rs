use std::path::Path;

use crate::resolver::{
    CallSite, Definition, LanguageResolver, ParseResult, Reference, ResolvedEdge,
};

/// Tier 1 + Tier 2 resolver for Go.
///
/// - Tier 1: tree-sitter-go for structural extraction.
/// - Tier 2: tree-sitter heuristics (sufficient for Go's explicit package system).
///
/// Go's simple import model means Tier 2 does not require an external tool;
/// tree-sitter queries plus package-path heuristics reach adequate accuracy.
pub struct GoResolver;

impl GoResolver {
    pub fn new() -> Self {
        GoResolver
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

    fn parse_file(&self, _path: &Path, _content: &str) -> ParseResult {
        // TODO: Implement tree-sitter Go parsing (Tier 1)
        // TODO: Add package-path heuristic resolution (Tier 2)
        ParseResult {
            definitions: vec![],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
        }
    }

    fn resolve_definitions(&self, _file: &Path) -> Vec<Definition> {
        // TODO: Extract function, struct, interface, and package definitions via tree-sitter
        vec![]
    }

    fn resolve_references(&self, _file: &Path) -> Vec<Reference> {
        // TODO: Extract call-sites, import refs, and type refs via tree-sitter
        vec![]
    }

    fn resolve_call_edge(&self, _call_site: &CallSite) -> Option<ResolvedEdge> {
        // TODO: Resolve via package-path heuristics (Tier 2)
        None
    }
}
