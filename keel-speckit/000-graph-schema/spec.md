# Spec 000: Graph Schema — Bedrock Data Structures

```yaml
tags: [keel, spec, graph-schema, bedrock]
owner: Agent A (Foundation)
dependencies: none — this is the root spec
prd_sections: [5, 10.3, 11, 23]
priority: P0 — everything depends on this
```

## Summary

This spec defines keel's core data model: the graph schema (nodes, edges, modules), the hash design, the SQLite storage layer, and the schema evolution strategy. Every other spec depends on these structures. This is the bedrock — get it right first.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 5 | Hash design — xxHash64, base62, canonical signature, collision handling |
| 10.3 | Hash collision handling — detection, disambiguation, invariant |
| 11 | Full graph schema — NodeKind, GraphNode, GraphEdge, ExternalEndpoint, ModuleProfile |
| 23 | Schema versioning, upgrade behavior, backward compatibility |

---

## Graph Schema

### Node Types

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Module,    // A file or directory-level module
    Class,     // A class, struct, trait, interface
    Function,  // A standalone function or method
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,                              // Internal graph ID (petgraph NodeIndex)
    pub hash: String,                         // base62(xxhash64(...)), 11 chars
    pub kind: NodeKind,
    pub name: String,                         // Function/class/module name
    pub signature: String,                    // Full normalized signature
    pub file_path: String,                    // Relative to project root
    pub line_start: u32,
    pub line_end: u32,
    pub docstring: Option<String>,            // First line of docstring, if present
    pub is_public: bool,                      // Exported / public visibility
    pub type_hints_present: bool,             // All params and return type annotated?
    pub has_docstring: bool,                  // Docstring present?
    pub external_endpoints: Vec<ExternalEndpoint>,
    pub previous_hashes: Vec<String>,         // Last 3 hashes for rename tracking
    pub module_id: u64,                       // Parent module node ID
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalEndpoint {
    pub kind: String,      // "HTTP", "gRPC", "GraphQL", "MessageQueue"
    pub method: String,    // "POST", "GET", etc. (for HTTP)
    pub path: String,      // "/api/users/:id"
    pub direction: String, // "serves" or "calls"
}
```

### Edge Types

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Calls,      // Function A calls function B
    Imports,    // Module A imports from module B
    Inherits,   // Class A extends/implements class B
    Contains,   // Module contains function/class
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source_id: u64,
    pub target_id: u64,
    pub kind: EdgeKind,
    pub file_path: String,   // Where the reference occurs
    pub line: u32,           // Line number of the reference
}
```

### Module Placement Profile

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleProfile {
    pub module_id: u64,
    pub path: String,                             // e.g., "src/auth/"
    pub function_count: u32,
    pub function_name_prefixes: Vec<String>,       // Common prefixes (e.g., ["validate", "check", "verify"])
    pub primary_types: Vec<String>,                // Most-used types in signatures
    pub import_sources: Vec<String>,               // Modules this module imports from
    pub export_targets: Vec<String>,               // Modules that import from this module
    pub external_endpoint_count: u32,
    pub responsibility_keywords: Vec<String>,      // Derived from function names + docstrings
}
```

---

## Hash Design

### Computation

```
hash = base62(xxhash64(canonical_signature + body_normalized + docstring))
```

**Components:**
1. **Canonical signature** — normalized function declaration: name, params with types where available, return type where available. Whitespace and comments stripped.
2. **Body normalized** — AST-based normalization via tree-sitter. Strip comments, normalize whitespace. NOT raw text (avoids hash churn from formatting changes).
3. **Docstring** — included in hash input. Forces hash change when documentation changes, ensuring the map stays current.

**Properties:**
- Deterministic: same function content = same hash
- Compact: 11 chars (base62 encoding of 64-bit hash)
- Content-addressed: captures signature + body + docstring
- Collision-resistant within a single codebase

### Collision Handling (PRD 10.3)

xxHash64 provides 2^64 space. For 10,000 functions, birthday paradox collision probability is ~2.7 x 10^-12 — negligible. But keel must handle collisions defensively:

1. **Detection:** When computing a new hash, check if it already exists in the graph for a different function. If so, append a disambiguator (file path hash) and re-hash.
2. **Reporting:** Emit `INFO: Hash collision detected and resolved for function 'X' in file 'Y'.`
3. **Invariant:** No two distinct functions in the graph may share the same hash. Enforced at write time.

### Rename Tracking

`previous_hashes` stores the last 3 hashes per node. When a function is renamed:
1. Old hash `xK2p9Lm4Q` disappears, new hash `bR3kL9mWq` appears
2. `compile` detects: old hash gone, new hash appeared, callers still reference old name
3. `discover` and `where` can resolve recently-changed hashes with `RENAMED` flag during the same editing session

---

## SQLite Storage Layer

### Database: `.keel/graph.db` (gitignored)

```sql
-- Schema version tracking
CREATE TABLE keel_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO keel_meta (key, value) VALUES ('schema_version', '1');

-- Nodes
CREATE TABLE nodes (
    id INTEGER PRIMARY KEY,
    hash TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('module', 'class', 'function')),
    name TEXT NOT NULL,
    signature TEXT NOT NULL DEFAULT '',
    file_path TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    docstring TEXT,
    is_public INTEGER NOT NULL DEFAULT 0,
    type_hints_present INTEGER NOT NULL DEFAULT 0,
    has_docstring INTEGER NOT NULL DEFAULT 0,
    module_id INTEGER REFERENCES nodes(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_nodes_hash ON nodes(hash);
CREATE INDEX idx_nodes_file ON nodes(file_path);
CREATE INDEX idx_nodes_module ON nodes(module_id);
CREATE INDEX idx_nodes_kind ON nodes(kind);

-- Previous hashes for rename tracking
CREATE TABLE previous_hashes (
    node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (node_id, hash)
);

-- External endpoints
CREATE TABLE external_endpoints (
    id INTEGER PRIMARY KEY,
    node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    method TEXT NOT NULL DEFAULT '',
    path TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('serves', 'calls'))
);
CREATE INDEX idx_endpoints_node ON external_endpoints(node_id);

-- Edges
CREATE TABLE edges (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    target_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('calls', 'imports', 'inherits', 'contains')),
    file_path TEXT NOT NULL,
    line INTEGER NOT NULL
);
CREATE INDEX idx_edges_source ON edges(source_id);
CREATE INDEX idx_edges_target ON edges(target_id);
CREATE INDEX idx_edges_kind ON edges(kind);

-- Module profiles
CREATE TABLE module_profiles (
    module_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    function_count INTEGER NOT NULL DEFAULT 0,
    function_name_prefixes TEXT NOT NULL DEFAULT '[]',  -- JSON array
    primary_types TEXT NOT NULL DEFAULT '[]',            -- JSON array
    import_sources TEXT NOT NULL DEFAULT '[]',           -- JSON array
    export_targets TEXT NOT NULL DEFAULT '[]',           -- JSON array
    external_endpoint_count INTEGER NOT NULL DEFAULT 0,
    responsibility_keywords TEXT NOT NULL DEFAULT '[]'   -- JSON array
);

-- Resolution cache (Tier 2/3 results)
CREATE TABLE resolution_cache (
    call_site_hash TEXT PRIMARY KEY,  -- hash of (file + line + call expression)
    resolved_node_id INTEGER REFERENCES nodes(id),
    confidence REAL NOT NULL,
    resolution_tier TEXT NOT NULL,
    cached_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Circuit breaker state (session-scoped, cleaned on restart)
CREATE TABLE circuit_breaker (
    error_code TEXT NOT NULL,
    hash TEXT NOT NULL,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    last_failure_at TEXT NOT NULL DEFAULT (datetime('now')),
    downgraded INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (error_code, hash)
);
```

### Manifest: `.keel/manifest.json` (committed)

Lightweight, human-readable. Needed for cross-repo linking (Phase 2).

```json
{
  "schema_version": 1,
  "keel_version": "2.0.0",
  "generated_at": "2026-02-08T10:00:00Z",
  "summary": {
    "total_nodes": 342,
    "total_edges": 891,
    "modules": 28,
    "functions": 287,
    "classes": 27,
    "external_endpoints": 15,
    "languages": ["typescript", "python"]
  },
  "modules": [
    {
      "path": "src/auth/",
      "function_count": 5,
      "external_endpoints": 3
    }
  ]
}
```

---

## Schema Evolution (PRD 23)

### Version tracking
- `graph.db` stores `schema_version` in `keel_meta` table
- `manifest.json` includes `"schema_version"` field
- `config.toml` includes `version` field

### Upgrade behavior
1. **Minor changes** (new columns, new optional fields): Automatic `ALTER TABLE`. No data loss. Transparent.
2. **Major changes** (new node types, changed edge semantics): `keel: Graph schema v1 -> v2 migration requires rebuild. Running 'keel map'...` — automatic rebuild.
3. **Manifest compatibility:** Team members on older keel see warning: `keel: manifest.json was generated by keel v2.x. Some features may not work. Run 'keel map' to update.`

### Backward compatibility guarantee
- keel N can always read graphs from keel N-1 (one version back)
- keel N can always rebuild from source (infinite backward compatibility via rebuild)
- `config.toml` is always forward-compatible (unknown keys ignored, not errors)

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> all agents):

**`GraphStore` trait:**
```rust
pub trait GraphStore {
    fn get_node(&self, hash: &str) -> Option<GraphNode>;
    fn get_node_by_id(&self, id: u64) -> Option<GraphNode>;
    fn get_edges(&self, node_id: u64, direction: EdgeDirection) -> Vec<GraphEdge>;
    fn get_module_profile(&self, module_id: u64) -> Option<ModuleProfile>;
    fn get_nodes_in_file(&self, file_path: &str) -> Vec<GraphNode>;
    fn get_all_modules(&self) -> Vec<GraphNode>;
    fn update_nodes(&mut self, changes: Vec<NodeChange>) -> Result<(), GraphError>;
    fn update_edges(&mut self, changes: Vec<EdgeChange>) -> Result<(), GraphError>;
    fn get_previous_hashes(&self, node_id: u64) -> Vec<String>;
}

pub enum EdgeDirection { Incoming, Outgoing, Both }

pub enum NodeChange {
    Add(GraphNode),
    Update(GraphNode),
    Remove(u64),
}

pub enum EdgeChange {
    Add(GraphEdge),
    Remove(u64),
}
```

### Dependencies: None — this is the root spec.

---

## Acceptance Criteria

**GIVEN** a freshly initialized keel project on a TypeScript + Python repo
**WHEN** `keel init` completes
**THEN** `graph.db` contains:
- All functions, classes, and modules as nodes with correct `NodeKind`
- All cross-file call edges with correct `EdgeKind`
- All hashes are 11-char base62 strings
- No duplicate hashes exist
- `manifest.json` is valid JSON with correct `schema_version`
- Module profiles populated with non-empty `responsibility_keywords`

**GIVEN** a function with known content
**WHEN** the hash is computed twice with identical input
**THEN** the hash is identical (deterministic)

**GIVEN** a function whose body is reformatted (whitespace only)
**WHEN** the hash is recomputed
**THEN** the hash is unchanged (AST-based normalization)

**GIVEN** a function whose docstring is changed
**WHEN** the hash is recomputed
**THEN** the hash changes (docstring included in hash input)

**GIVEN** two functions with different content that collide on xxHash64
**WHEN** both are added to the graph
**THEN** the collision is detected, disambiguated, and reported as `INFO`

**GIVEN** a keel v1 `graph.db`
**WHEN** keel v2 binary reads it
**THEN** automatic migration runs (minor) or rebuild triggers (major)

---

## Test Strategy

**Oracle:** Graph correctness (Oracle 1 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])
- Compare node/edge counts against LSP ground truth
- Verify hash determinism with property-based tests
- Verify collision handling with synthetic collision inputs
- Verify schema migration with versioned test databases

**Test files to create:**
- `tests/graph/test_node_creation.rs` (~10 tests)
- `tests/graph/test_edge_creation.rs` (~10 tests)
- `tests/graph/test_hash_computation.rs` (~15 tests)
- `tests/graph/test_hash_collision.rs` (~5 tests)
- `tests/graph/test_module_profile.rs` (~8 tests)
- `tests/graph/test_sqlite_storage.rs` (~12 tests)
- `tests/graph/test_schema_migration.rs` (~5 tests)
- `tests/graph/test_previous_hashes.rs` (~5 tests)

**Estimated test count:** ~70

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| petgraph API changes between versions | Pin version in Cargo.toml |
| SQLite locking under concurrent LLM sessions | File-level locking, <200ms compile means low contention |
| Hash collision in practice | Defensive detection + disambiguation. Log if frequent (suggests wrong hashing input) |
| Module profile keywords too generic | Start conservative — function names + docstring nouns only. Tune thresholds during dogfooding |

---

## Related Specs

- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — populates this schema
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — reads from this schema
- [[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]] — serializes this schema to JSON
