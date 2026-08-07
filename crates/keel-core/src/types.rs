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

/// Edge kinds that make one SYMBOL depend on another: a resolved call, and a
/// name used as a value (callback, handler table, template expression).
///
/// The adjacency surfaces — `discover`, `focus`, `search`, the fan-in counts —
/// answer "what depends on this?", and a `uses` edge answers it. Severity
/// checks deliberately do NOT use this set: E001/E004/E005 and the fix planner
/// filter [`EdgeKind::Calls`] themselves, because only a parsed call site
/// carries an argument list.
pub const SYMBOL_DEP_KINDS: &[EdgeKind] = &[EdgeKind::Calls, EdgeKind::Uses];

/// Edge kinds that make one MODULE depend on another.
///
/// A resolved call or an import is a load-order relationship between files; a
/// `uses` edge (a name handed around as a value) is not, and `contains` is
/// intra-file by construction.
pub const MODULE_DEP_KINDS: &[EdgeKind] = &[EdgeKind::Calls, EdgeKind::Imports];

/// Render edge kinds as the body of a SQL `IN (...)` list — `'calls','uses'`.
///
/// The SQL half of the two sets above, so a literal in a query string cannot
/// drift from the Rust predicate it is supposed to mirror.
pub fn sql_in_list(kinds: &[EdgeKind]) -> String {
    kinds
        .iter()
        .map(|k| format!("'{}'", k.as_str()))
        .collect::<Vec<_>>()
        .join(",")
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
    /// McCabe cyclomatic complexity, from `Definition::complexity`. 1 for
    /// modules and (for now) classes — there is no meaningful "class
    /// complexity". Zero means *not measured*: rows written before the column
    /// existed read back as 0 until the next `keel map`/`keel compile` touches
    /// them.
    #[serde(default)]
    pub complexity: u32,
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
    /// Fingerprint of the identifier/literal-normalized ("Type-2") body — see
    /// `hash_t2::compute_t2_hash`. Empty when the Type-2 form did not clear
    /// `hash_t2::MIN_T2_NORMALIZED_LEN`, and on rows written before the column
    /// existed; empty is never a match, only a "not indexed for Type-2".
    pub t2_hash: String,
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

/// An architectural boundary a node belongs to.
///
/// Two schemes coexist, deliberately kept distinct so a package named `web`
/// never compares equal to a top-level `web/` directory:
///
/// - [`Boundary::Package`] — the node carries a declared monorepo package
///   (`nodes.package`, populated by `keel map` in a detected workspace).
/// - [`Boundary::Directory`] — the node has no declared package, so its
///   boundary is the FIRST path segment of its file path (`frontend/src/x.ts`
///   → `frontend`). Files sitting directly at the repo root have no boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Boundary {
    /// A declared monorepo package (`nodes.package`).
    Package(String),
    /// The first path segment of a file with no declared package.
    Directory(String),
}

impl Boundary {
    /// Derive the boundary of a node from its declared package and file path.
    ///
    /// Returns `None` for a root-level file with no declared package: there is
    /// no directory segment to name, and guessing one would produce confident
    /// wrong answers.
    pub fn of(package: Option<&str>, file_path: &str) -> Option<Self> {
        if let Some(p) = package.filter(|p| !p.is_empty()) {
            return Some(Boundary::Package(p.to_string()));
        }
        let normalized = file_path.replace('\\', "/");
        let (head, rest) = normalized.split_once('/')?;
        if head.is_empty() || rest.is_empty() {
            return None;
        }
        Some(Boundary::Directory(head.to_string()))
    }

    /// Human-readable name used in violation messages and deny-list matching.
    pub fn label(&self) -> &str {
        match self {
            Boundary::Package(p) => p,
            Boundary::Directory(d) => d,
        }
    }
}

/// A stored node reduced to the three columns boundary analysis needs.
///
/// Returned by the boundary queries instead of a full `GraphNode` so the
/// per-file hot-path cost stays one indexed query with no relation loading.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryTarget {
    /// Node name (empty when the query does not need it).
    pub name: String,
    /// Declared package, if any.
    pub package: Option<String>,
    /// File the node lives in.
    pub file_path: String,
}

impl BoundaryTarget {
    /// The boundary this target belongs to, or `None` for a root-level file
    /// with no declared package.
    pub fn boundary(&self) -> Option<Boundary> {
        Boundary::of(self.package.as_deref(), &self.file_path)
    }
}

/// What W009 needs to know about a compiled file's module — the directory
/// holding it, not the file itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModuleBoundaryInfo {
    /// Declared package shared by the module's stored nodes, if any. Lets a
    /// brand-new file inherit its siblings' package instead of falling back to
    /// a directory boundary that would read as cross-package against them.
    pub package: Option<String>,
    /// Targets of every stored `calls` edge leaving the module — the
    /// grandfathered boundary set. Empty means this area of the repo was never
    /// mapped, so "every dependency is new" would be an artifact of the
    /// missing graph rather than a change.
    pub call_targets: Vec<BoundaryTarget>,
}

impl ModuleBoundaryInfo {
    /// Whether the module has any stored call edges at all — W009's
    /// bootstrap guard.
    pub fn is_mapped(&self) -> bool {
        !self.call_targets.is_empty()
    }
}

/// Raw graph facts the `keel quality` metrics are computed from.
///
/// One bundle rather than four store methods, because they are always read
/// together and a partial read would produce a snapshot whose metrics describe
/// different graph states. Everything here is deduplicated by the store: a file
/// with two stored module rows (hash-salt collisions do happen) contributes one
/// entry, or the size metric would count it twice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QualityInputs {
    /// One entry per mapped file: `(file_path, line_count)`.
    pub file_lines: Vec<(String, u32)>,
    /// Private, non-associated functions with no incoming `calls`/`uses` edge:
    /// `(file_path, name)`. Naming and file-class exemptions are applied by the
    /// caller, which owns those rules.
    pub uncalled_private_fns: Vec<(String, String)>,
    /// Distinct cross-file module dependencies: `(source_file, target_file)`.
    pub module_deps: Vec<(String, String)>,
    /// Stored dependency edges (`calls` + `uses`) with a resolvable target.
    pub dependency_edges: u64,
    /// How many of those cross a file boundary.
    pub cross_file_dependency_edges: u64,
    /// One entry per function node: `(file_path, complexity, sloc)`, where
    /// `sloc` is `line_end - line_start + 1` — the same span source as
    /// `file_lines`. File-class and test exemptions are the caller's job, as
    /// with every other field here.
    pub complexity_by_fn: Vec<(String, u32, u32)>,
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
