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
}

impl EdgeKind {
    /// Returns the lowercase string representation of this edge kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Calls => "calls",
            EdgeKind::Imports => "imports",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Contains => "contains",
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
    /// Edges with confidence < 0.80 (dynamic dispatch, ambiguous resolution)
    /// produce WARNINGs instead of ERRORs in enforcement.
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
