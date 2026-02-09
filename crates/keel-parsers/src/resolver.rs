use std::path::Path;

use keel_core::types::{ExternalEndpoint, NodeKind};

// ---------------------------------------------------------------------------
// FROZEN CONTRACT -- LanguageResolver trait
// Agent A owns this interface. Agent B consumes it.
// Do NOT modify the trait signature without coordinating across all agents.
// ---------------------------------------------------------------------------

/// The core abstraction every language-specific parser must implement.
///
/// Each resolver is responsible for:
/// - Parsing source files into definitions, references, and imports (Tier 1).
/// - Resolving call-site edges with a confidence score (Tier 2).
///
/// Implementors must be `Send + Sync` so they can be shared across rayon
/// parallel iterators.
pub trait LanguageResolver: Send + Sync {
    /// Returns the canonical language name (e.g. "typescript", "python").
    fn language(&self) -> &str;

    /// Parse a single file and return all structural information.
    fn parse_file(&self, path: &Path, content: &str) -> ParseResult;

    /// Return definitions found in `file`. May re-parse or use cached data.
    fn resolve_definitions(&self, file: &Path) -> Vec<Definition>;

    /// Return references (calls, imports, type-refs) found in `file`.
    fn resolve_references(&self, file: &Path) -> Vec<Reference>;

    /// Attempt to resolve a call-site to a concrete target.
    /// Returns `None` when resolution is ambiguous or unsupported.
    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge>;
}

// ---------------------------------------------------------------------------
// Types returned by LanguageResolver methods
// ---------------------------------------------------------------------------

/// Complete parse output for a single source file.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
    pub external_endpoints: Vec<ExternalEndpoint>,
}

/// A definition (function, class, module) extracted from source.
#[derive(Debug, Clone)]
pub struct Definition {
    /// Simple name of the symbol (e.g. "handleRequest").
    pub name: String,
    /// What kind of graph node this maps to.
    pub kind: NodeKind,
    /// Canonical signature string used for hash computation.
    pub signature: String,
    /// Absolute or repo-relative file path.
    pub file_path: String,
    /// First line of the definition (1-based).
    pub line_start: u32,
    /// Last line of the definition (1-based, inclusive).
    pub line_end: u32,
    /// Leading doc-comment, if any.
    pub docstring: Option<String>,
    /// Whether the symbol is exported / public.
    pub is_public: bool,
    /// Whether all parameters and return value carry type annotations.
    pub type_hints_present: bool,
    /// Raw body text (used for hash computation after AST normalization).
    pub body_text: String,
}

/// The flavour of a reference occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A function/method call.
    Call,
    /// An import statement reference.
    Import,
    /// A type annotation or type-level reference.
    TypeRef,
}

/// A reference (usage) of a symbol within a file.
#[derive(Debug, Clone)]
pub struct Reference {
    /// Name of the referenced symbol.
    pub name: String,
    /// File where the reference occurs.
    pub file_path: String,
    /// Line number of the reference (1-based).
    pub line: u32,
    /// What kind of reference this is.
    pub kind: ReferenceKind,
    /// If already resolved, the hash/id of the target definition.
    pub resolved_to: Option<String>,
}

/// An import statement extracted from source.
#[derive(Debug, Clone)]
pub struct Import {
    /// The module specifier / source path.
    pub source: String,
    /// Individual names brought into scope (empty for wildcard/namespace imports).
    pub imported_names: Vec<String>,
    /// File containing the import.
    pub file_path: String,
    /// Line number (1-based).
    pub line: u32,
    /// Whether this is a relative import (e.g. `./foo` or `from .bar`).
    pub is_relative: bool,
}

/// A call site that needs resolution.
#[derive(Debug, Clone)]
pub struct CallSite {
    /// File containing the call.
    pub file_path: String,
    /// Line number of the call (1-based).
    pub line: u32,
    /// Name of the function/method being called.
    pub callee_name: String,
    /// Receiver expression, if this is a method call (e.g. "self", "obj").
    pub receiver: Option<String>,
}

/// The result of successfully resolving a call edge.
#[derive(Debug, Clone)]
pub struct ResolvedEdge {
    /// File containing the target definition.
    pub target_file: String,
    /// Name of the target definition.
    pub target_name: String,
    /// Resolution confidence (0.0 = guess, 1.0 = certain).
    /// Low-confidence edges produce WARNINGs, not ERRORs.
    pub confidence: f64,
}

/// Aggregated index for a single file -- used by the incremental pipeline
/// to decide whether a file needs re-parsing.
#[derive(Debug, Clone)]
pub struct FileIndex {
    /// Absolute or repo-relative file path.
    pub file_path: String,
    /// xxhash64 of the raw file content (for change detection).
    pub content_hash: u64,
    /// All definitions found in this file.
    pub definitions: Vec<Definition>,
    /// All references found in this file.
    pub references: Vec<Reference>,
    /// All imports found in this file.
    pub imports: Vec<Import>,
    /// External endpoints (HTTP routes, gRPC services, etc.).
    pub external_endpoints: Vec<ExternalEndpoint>,
    /// Wall-clock microseconds spent parsing this file.
    pub parse_duration_us: u64,
}
