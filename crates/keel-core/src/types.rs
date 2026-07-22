use serde::{Deserialize, Serialize};

/// Node types in the structural graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Module,
    Class,
    Function,
}

impl NodeKind {
    /// Returns the lowercase string representation of this node kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Module => "module",
            NodeKind::Class => "class",
            NodeKind::Function => "function",
        }
    }
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Edge types between graph nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Calls,
    Imports,
    Inherits,
    Contains,
    /// A function named as a *value* rather than invoked — passed as a
    /// callback, stored in a table, named in an attribute string. It proves the
    /// target is used (W005 accepts it as a caller) but carries no argument
    /// list, so it must never feed broken-caller/arity checking.
    Uses,
}

impl EdgeKind {
    /// Returns the lowercase string representation of this edge kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Calls => "calls",
            EdgeKind::Imports => "imports",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Contains => "contains",
            EdgeKind::Uses => "uses",
        }
    }
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A node in the structural graph (function, class, or module).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,
    pub hash: String,
    pub kind: NodeKind,
    pub name: String,
    pub signature: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub docstring: Option<String>,
    pub is_public: bool,
    pub type_hints_present: bool,
    pub has_docstring: bool,
    /// True for methods/associated functions on a type (Rust `impl` blocks,
    /// class bodies, Go receiver funcs). Persisted so W002 can exempt a stored
    /// associated method from colliding with a free function of the same name
    /// in a different file, across separate compiles (issue #46).
    #[serde(default)]
    pub is_associated: bool,
    pub external_endpoints: Vec<ExternalEndpoint>,
    pub previous_hashes: Vec<String>,
    pub module_id: u64,
    pub package: Option<String>,
}

/// An external endpoint (HTTP, gRPC, GraphQL, etc.) associated with a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalEndpoint {
    pub kind: String,
    pub method: String,
    pub path: String,
    pub direction: String,
}

/// An edge in the structural graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: u64,
    pub source_id: u64,
    pub target_id: u64,
    pub kind: EdgeKind,
    pub file_path: String,
    pub line: u32,
    /// Resolution confidence (0.0 = guess, 1.0 = certain).
    /// `calls` edges with confidence < 0.80 (dynamic dispatch, ambiguous
    /// resolution) produce WARNINGs instead of ERRORs in enforcement.
    /// `uses` edges never feed severity regardless of confidence — every
    /// error-producing check filters on kind first.
    #[serde(default = "default_edge_confidence")]
    pub confidence: f64,
}

fn default_edge_confidence() -> f64 {
    1.0
}

/// Module responsibility profile for placement scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleProfile {
    pub module_id: u64,
    pub path: String,
    pub function_count: u32,
    pub class_count: u32,
    pub line_count: u32,
    pub function_name_prefixes: Vec<String>,
    pub primary_types: Vec<String>,
    pub import_sources: Vec<String>,
    pub export_targets: Vec<String>,
    pub external_endpoint_count: u32,
    pub responsibility_keywords: Vec<String>,
}

/// Direction for edge traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDirection {
    Incoming,
    Outgoing,
    Both,
}

/// A change to be applied to nodes in the graph.
#[derive(Debug, Clone)]
pub enum NodeChange {
    Add(GraphNode),
    Update(GraphNode),
    Remove(u64),
}

/// A change to be applied to edges in the graph.
#[derive(Debug, Clone)]
pub enum EdgeChange {
    Add(GraphEdge),
    Remove(u64),
}

/// One entry in the body-hash duplicate index.
///
/// Maps a normalized-body fingerprint (`body_hash`) to the node that produced
/// it, so nodes sharing a `body_hash` are candidate duplicate implementations.
/// Populated during `keel map`; queried by W006 duplicate-implementation
/// enforcement.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyIndexEntry {
    /// Fingerprint of the normalized body — see `hash::compute_body_hash`.
    pub body_hash: String,
    /// Identity key of the indexed function.
    ///
    /// Part of the storage primary key `(node_hash, file_path, line)` — a
    /// bare `node_hash` is *not* unique, since disambiguation salts with the
    /// file path only. Not consumed by W006 matching today; matching keys on
    /// `body_hash`.
    pub node_hash: String,
    /// Node name, carried so callers can report duplicates without a join.
    pub name: String,
    /// File the node lives in.
    pub file_path: String,
    /// Line the node starts on.
    pub line: u32,
}

/// One persisted Tier 3 resolution-cache row (`resolution_tier = 'tier3'`).
///
/// Lets SCIP/LSP resolutions survive across `keel map` runs. Keyed by
/// `call_site_hash` (a content-addressed fingerprint of the call site — see
/// `Tier3CacheKey::cache_hash`), so the original `(file_path, line, callee)`
/// is not recoverable from a row, only hashed forward into one. An entry is
/// "resolved" iff `target_file` is `Some`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolutionCacheEntry {
    /// Content-addressed key of the resolved call site.
    pub call_site_hash: String,
    /// File the call resolved to, or `None` when the provider found no target.
    pub target_file: Option<String>,
    /// Symbol the call resolved to, or `None` when unresolved.
    pub target_name: Option<String>,
    /// Confidence of the resolution (`0.0` for an unresolved entry).
    pub confidence: f64,
    /// Provider that produced the result (`scip`/`lsp`), or `None` if unresolved.
    pub provider: Option<String>,
}

/// Errors that can occur during graph operations.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Edge not found: {0}")]
    EdgeNotFound(u64),

    #[error("Duplicate hash: {0}")]
    DuplicateHash(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error(
        "Hash collision detected for hash {hash} between functions '{existing}' and '{new_fn}'"
    )]
    HashCollision {
        hash: String,
        existing: String,
        new_fn: String,
    },

    #[error("Schema migration required: v{from} -> v{to}")]
    SchemaMigration { from: u32, to: u32 },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for GraphError {
    fn from(e: rusqlite::Error) -> Self {
        GraphError::Database(e.to_string())
    }
}
