# Spec 005: Rust Resolution — rust-analyzer-Powered Tier 2

```yaml
tags: [keel, spec, rust, rust-analyzer, ra_ap_ide, tier-2, resolution, lazy-load]
owner: Agent A (Foundation)
dependencies: [Spec 000 (graph schema), Spec 001 (tree-sitter foundation)]
prd_sections: [10.1]
priority: P1 — Rust is keel's own language; dogfooding requires self-analysis
```

## Summary

This spec defines Tier 2 resolution for Rust via the `ra_ap_ide` family of crates from rust-analyzer. These crates provide programmatic access to rust-analyzer's analysis engine, including name resolution, type inference, and SCIP index emission. The API is explicitly unstable (0.0.x versioning), but architecturally mature — rust-analyzer has been the de facto Rust IDE engine since 2020. Due to rust-analyzer's 60s+ startup time for large workspaces (building the full VFS and running `cargo metadata`), integration is **lazy-loaded**: not always-on, invoked only when Tier 1 tree-sitter resolution is insufficient. Trait dispatch is flagged as low confidence (WARNING, not ERROR) since concrete dispatch targets require full type inference that may not be available during incremental updates. This targets ~75-99% resolution precision depending on whether rust-analyzer analysis is warm.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10.1 Tier 2 Rust | rust-analyzer via `ra_ap_ide` crates — lazy-loaded, SCIP emission capable |
| 10.1 Tier 2 General | Tier 2 resolvers run per-language, fill edges the tree-sitter layer cannot resolve |
| 10.1 Precision | Combined Tier 1 + Tier 2 target: ~75-99% for Rust (rust-analyzer dependent) |

---

## Technical Specification

### Scope

**File types:** `.rs`

**Resolution capabilities:**
- `mod` declarations (inline and file-based)
- `use` statements (`use crate::module::Symbol`, `use super::func`)
- `pub` visibility modifiers (`pub`, `pub(crate)`, `pub(super)`, `pub(in path)`)
- Crate-level module tree resolution
- Trait implementations (`impl Trait for Type`)
- Generic type resolution (monomorphization not required — structural resolution sufficient)
- Method resolution on concrete types
- Associated types and constants
- Re-exports (`pub use`)

**Out of scope (deferred to Tier 3 LSP or future work):**
- Procedural macro expansion (requires `cargo expand` or rust-analyzer's macro engine)
- Const generics evaluation
- Lifetime inference
- `unsafe` block analysis
- Build script (`build.rs`) output

### Why Lazy-Loading Is Required

rust-analyzer's startup sequence for a large workspace:
1. Run `cargo metadata` to discover crate graph (~2-5s)
2. Build the VFS (virtual filesystem) of all source files (~5-15s)
3. Parse all files and build initial analysis database (~30-60s)
4. Compute name resolution and type inference incrementally (ongoing)

For a workspace like `rust-analyzer` itself (~500 crates, ~200k LOC), initial load exceeds 60 seconds. This is unacceptable for `keel compile` (<200ms target) or even `keel map` (seconds target).

**Strategy:**
- **First `keel map`:** Tree-sitter heuristics only (Tier 1). Fast.
- **Background:** Kick off rust-analyzer warm-up asynchronously.
- **Subsequent `keel map` / `keel compile`:** If rust-analyzer is warm, use it. If not, use Tier 1 results with a note that Tier 2 will be available shortly.
- **SCIP export:** Once warm, emit SCIP index for batch resolution of all cross-file references.

### rust-analyzer Integration via `ra_ap_ide`

```rust
use ra_ap_ide::{AnalysisHost, Change, FileId, FilePosition};
use ra_ap_project_model::{CargoConfig, ProjectManifest, ProjectWorkspace};
use ra_ap_vfs::{Vfs, VfsPath};

pub struct RustAnalyzerResolver {
    state: RaState,
    project_root: PathBuf,
}

enum RaState {
    /// rust-analyzer has not been started yet.
    Cold,
    /// rust-analyzer is warming up in the background.
    Warming { handle: JoinHandle<Result<RaSession, RaError>> },
    /// rust-analyzer is ready for queries.
    Warm(RaSession),
    /// rust-analyzer failed to start.
    Failed(String),
}

struct RaSession {
    host: AnalysisHost,
    vfs: Vfs,
    file_map: HashMap<PathBuf, FileId>,
}

impl RustAnalyzerResolver {
    /// Create a cold resolver. Does NOT start rust-analyzer.
    pub fn new(project_root: &Path) -> Self {
        Self {
            state: RaState::Cold,
            project_root: project_root.to_path_buf(),
        }
    }

    /// Start rust-analyzer warm-up in a background thread.
    pub fn start_warmup(&mut self) {
        if matches!(self.state, RaState::Cold) {
            let root = self.project_root.clone();
            let handle = std::thread::spawn(move || {
                Self::initialize_ra(&root)
            });
            self.state = RaState::Warming { handle };
        }
    }

    /// Check if rust-analyzer is ready. Non-blocking.
    pub fn is_ready(&mut self) -> bool {
        match &self.state {
            RaState::Warm(_) => true,
            RaState::Warming { .. } => {
                // Check if the background thread has completed
                self.try_complete_warmup();
                matches!(self.state, RaState::Warm(_))
            }
            _ => false,
        }
    }

    /// Resolve a symbol at a given file position.
    /// Returns None if rust-analyzer is not warm.
    pub fn resolve_at_position(
        &self,
        file_path: &Path,
        line: u32,
        column: u32,
    ) -> Option<Result<RustResolution, RaError>> {
        match &self.state {
            RaState::Warm(session) => {
                let file_id = session.file_map.get(file_path)?;
                let position = FilePosition {
                    file_id: *file_id,
                    offset: session.line_col_to_offset(*file_id, line, column),
                };
                let analysis = session.host.analysis();
                Some(Self::do_resolve(&analysis, position))
            }
            _ => None, // Not ready — caller should use Tier 1 results
        }
    }

    /// Initialize rust-analyzer for the project.
    fn initialize_ra(project_root: &Path) -> Result<RaSession, RaError> {
        let cargo_toml = project_root.join("Cargo.toml");
        let manifest = ProjectManifest::from_manifest_file(cargo_toml)
            .map_err(RaError::ManifestError)?;

        let cargo_config = CargoConfig::default();
        let workspace = ProjectWorkspace::load(
            manifest,
            &cargo_config,
            &|_| {},
        ).map_err(RaError::WorkspaceError)?;

        let mut vfs = Vfs::default();
        let mut host = AnalysisHost::default();
        let mut change = Change::new();

        // Load all source files into VFS
        let file_map = Self::load_vfs(&workspace, &mut vfs, &mut change)?;

        host.apply_change(change);

        Ok(RaSession { host, vfs, file_map })
    }
}

#[derive(Debug, Clone)]
pub struct RustResolution {
    pub resolved_file: PathBuf,
    pub resolved_line: u32,
    pub symbol_name: String,
    pub visibility: RustVisibility,
    pub confidence: f64,
    pub resolution_kind: RustResolutionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RustVisibility {
    Public,           // pub
    Crate,            // pub(crate)
    Super,            // pub(super)
    InPath(String),   // pub(in path)
    Private,          // no modifier
}

#[derive(Debug, Clone, PartialEq)]
pub enum RustResolutionKind {
    UseStatement,
    ModDeclaration,
    TraitImpl,
    MethodCall,
    AssociatedItem,
    GenericInstantiation,
}
```

### SCIP Emission

rust-analyzer has built-in capability to emit SCIP (Source Code Intelligence Protocol) indexes, which keel can consume for batch resolution:

```rust
impl RustAnalyzerResolver {
    /// Emit a SCIP index for the entire workspace.
    /// This is more efficient than per-symbol queries for full `keel map`.
    pub fn emit_scip(&self) -> Option<Result<ScipIndex, RaError>> {
        match &self.state {
            RaState::Warm(session) => {
                let analysis = session.host.analysis();
                Some(Self::generate_scip(&analysis, &session.vfs))
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ScipIndex {
    pub occurrences: Vec<ScipOccurrence>,
}

#[derive(Debug)]
pub struct ScipOccurrence {
    pub file_path: PathBuf,
    pub line: u32,
    pub column: u32,
    pub symbol: String,        // SCIP symbol string
    pub definition_file: PathBuf,
    pub definition_line: u32,
    pub role: ScipRole,
}

#[derive(Debug, PartialEq)]
pub enum ScipRole {
    Definition,
    Reference,
}
```

### Rust-Specific Resolution Details

#### `mod` Declarations

```rust
// src/lib.rs
mod auth;       // resolves to src/auth.rs or src/auth/mod.rs
mod db {        // inline module
    pub fn connect() {}
}
```

**Strategy:**
1. Parse `mod` declarations via tree-sitter
2. For non-inline `mod`: check for `{name}.rs` then `{name}/mod.rs` (Rust 2018 edition preferred layout)
3. Create `Contains` edges from parent module to child module
4. For inline `mod`: symbols are in the same file, tree-sitter handles directly

#### `use` Statements

```rust
use crate::auth::login;           // absolute from crate root
use super::helpers::hash;         // relative to parent module
use std::collections::HashMap;    // external crate
use self::utils::format;          // relative to current module
pub use crate::models::User;      // re-export
```

**Strategy:**
1. Parse `use` tree via tree-sitter, extracting the path segments
2. Resolve `crate::` to the crate root (lib.rs or main.rs)
3. Resolve `super::` by walking up the module tree
4. Resolve `self::` to the current module
5. External crate uses: track as external dependency (no source resolution)
6. `pub use` re-exports: create both an `Imports` edge and record the re-export for consumers

#### `pub` Visibility

```rust
pub fn public_func() {}           // visible everywhere
pub(crate) fn crate_func() {}     // visible within the crate
pub(super) fn super_func() {}     // visible in parent module
pub(in crate::auth) fn scoped() {} // visible in specific path
fn private_func() {}               // visible in current module only
```

**Strategy:**
- Parse visibility modifier from tree-sitter AST
- Map to `RustVisibility` enum
- Cross-module resolution validates visibility: if a private function is referenced from another module, flag as ERROR (compile error)
- `pub(crate)` and `pub(super)` scoping validated against the module tree

#### Trait Implementations

```rust
trait Authenticate {
    fn login(&self, token: &str) -> Result<User, AuthError>;
}

impl Authenticate for JwtAuth {
    fn login(&self, token: &str) -> Result<User, AuthError> { ... }
}
```

**Strategy:**
- `impl Trait for Type` blocks are detected via tree-sitter
- An `Inherits` edge is created from the implementing type to the trait
- Method dispatch through trait objects (`dyn Authenticate`) is flagged as **low confidence (0.60)** with `WARNING: trait dispatch — concrete type unknown at call site`
- Static dispatch (`JwtAuth::login()`) resolves with full confidence (0.93)
- Tier 3 LSP can promote trait dispatch confidence with full type inference

#### Generic Type Resolution

```rust
fn process<T: Serialize + Debug>(item: T) -> String { ... }
```

**Strategy:**
- Extract generic bounds from tree-sitter
- Create edges to the bound traits (`Serialize`, `Debug`)
- Method calls on generic parameters are resolved against the trait bounds
- Monomorphization is NOT required — keel only needs structural edges
- Generic calls flagged with moderate confidence (0.80)

### Integration with Graph Schema

Tier 2 Rust resolution produces:
- `GraphEdge { kind: EdgeKind::Imports }` for `use` statements
- `GraphEdge { kind: EdgeKind::Calls }` for resolved cross-module function/method calls
- `GraphEdge { kind: EdgeKind::Inherits }` for trait implementations and struct embedding
- `GraphEdge { kind: EdgeKind::Contains }` for `mod` declaration containment
- Resolution results cached in `resolution_cache` table with `resolution_tier: "tier2_rust"` or `"tier1_rust_heuristic"` (when ra is cold)

### Confidence Scoring

| Resolution Path | Confidence |
|----------------|------------|
| rust-analyzer resolved `use` path | 0.97 |
| rust-analyzer resolved method call (concrete type) | 0.95 |
| rust-analyzer SCIP occurrence | 0.97 |
| Tree-sitter `mod` declaration to file | 0.93 |
| Tree-sitter `use crate::` absolute path | 0.88 |
| Tree-sitter `use super::` relative path | 0.85 |
| `pub use` re-export traced to definition | 0.90 |
| Generic type parameter method via trait bounds | 0.80 |
| Trait object dispatch (`dyn Trait`) | 0.60 |
| Trait dispatch (static, but complex trait hierarchy) | 0.70 |

### Lazy-Load Lifecycle

```
keel map (first run on Rust project):
  1. Tree-sitter Tier 1 runs immediately (~seconds)
  2. Tier 2 heuristic resolution runs (use paths, mod declarations)
  3. Background: rust-analyzer starts warming up
  4. Graph populated with Tier 1 + heuristic Tier 2 results
  5. INFO: "rust-analyzer warming up — Tier 2 results available on next run"

keel map (subsequent runs, ra warm):
  1. Tree-sitter Tier 1 runs immediately
  2. rust-analyzer Tier 2 runs — full resolution via ra_ap_ide or SCIP
  3. Graph populated with high-confidence Tier 2 results
  4. Previous heuristic results promoted or corrected

keel compile (ra warm):
  1. Incremental: only changed files re-analyzed
  2. rust-analyzer provides targeted resolution for changed call sites
  3. <200ms target maintained via incremental queries
```

---

## Acceptance Criteria

**GIVEN** a Rust project with `mod auth;` in `lib.rs` and `src/auth.rs` exists
**WHEN** resolution runs
**THEN** a `Contains` edge is created from the root module to the `auth` module with confidence >= 0.93

**GIVEN** `use crate::models::User;` in a file and `User` is defined in `src/models.rs`
**WHEN** resolution runs (tree-sitter heuristic, ra cold)
**THEN** the `Imports` edge resolves to `src/models.rs` with confidence >= 0.85

**GIVEN** rust-analyzer is warm and the same `use crate::models::User` is resolved
**WHEN** resolution runs
**THEN** the `Imports` edge resolves with confidence >= 0.97

**GIVEN** a `pub(crate)` function referenced from another module within the same crate
**WHEN** resolution runs
**THEN** the edge is created with correct visibility annotation and no error flag

**GIVEN** `impl Serialize for MyStruct` in a file
**WHEN** resolution runs
**THEN** an `Inherits` edge is created from `MyStruct` to `Serialize`

**GIVEN** a method call on a `dyn Trait` parameter
**WHEN** resolution runs
**THEN** the edge is created with confidence 0.60 and `WARNING: trait dispatch` annotation

**GIVEN** a generic function `fn process<T: Clone>(item: T)` calling `item.clone()`
**WHEN** resolution runs
**THEN** the `Calls` edge targets the `Clone::clone` trait method with confidence >= 0.80

**GIVEN** it is the first `keel map` on a Rust project (rust-analyzer cold)
**WHEN** resolution completes
**THEN** Tier 1 + heuristic Tier 2 results are returned, and an `INFO` message indicates rust-analyzer is warming up in the background

---

## Test Strategy

**Oracle:** LSP ground truth (Oracle 1 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])
- Run rust-analyzer on test repos via LSP protocol
- Capture "Go to Definition" results for all `use` statements and cross-module calls
- Compare keel Tier 2 resolution (both heuristic and ra-powered modes) against LSP results
- Measure precision as: correct edges / total edges produced

**Test repositories:**
- **ripgrep** (BurntSushi/ripgrep) — multi-crate workspace, clean module structure, trait usage
- **axum** (tokio-rs/axum) — heavy generic usage, trait dispatch, tower middleware patterns

**Test files to create:**
- `tests/rust_resolution/test_mod_declarations.rs` (~5 tests)
- `tests/rust_resolution/test_use_paths.rs` (~6 tests)
- `tests/rust_resolution/test_visibility.rs` (~5 tests)
- `tests/rust_resolution/test_trait_impls.rs` (~5 tests)
- `tests/rust_resolution/test_generic_resolution.rs` (~4 tests)
- `tests/rust_resolution/test_lazy_load.rs` (~3 tests)
- `tests/rust_resolution/test_scip_emission.rs` (~2 tests)

**Estimated test count:** ~30

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| `ra_ap_ide` crates are 0.0.x — API breaks on every release | Pin exact version. Wrap all ra_ap calls behind a thin adapter trait. Update adapter when upgrading. |
| 60s+ startup for large workspaces blocks first analysis | Lazy-load design: Tier 1 results returned immediately, Tier 2 arrives when ra is warm. Never block on ra. |
| rust-analyzer memory usage (~1-2GB for large workspaces) | Document minimum memory requirements. Provide `keel.toml` option to disable Rust Tier 2 entirely. |
| Procedural macros not expanded — may miss generated code | Document limitation. Suggest `cargo expand` for users who need macro-generated edges. Phase 2 consideration. |
| Trait dispatch is inherently incomplete without full program analysis | Flag as WARNING with low confidence. Acceptable for keel's map/compile use case. Tier 3 can promote. |
| SCIP format may change between rust-analyzer versions | Pin version. SCIP protocol itself is stable (Sourcegraph maintained). |

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> Agent B):

**`RustAnalyzerResolver` struct:**
- `new(project_root: &Path) -> Self` — create cold resolver
- `start_warmup(&mut self)` — begin background initialization
- `is_ready(&mut self) -> bool` — non-blocking readiness check
- `resolve_at_position(file: &Path, line: u32, col: u32) -> Option<Result<RustResolution, RaError>>` — per-symbol resolution
- `emit_scip() -> Option<Result<ScipIndex, RaError>>` — batch SCIP index

**Lazy-load contract:**
- Callers MUST handle `None` return (ra not ready) by falling back to Tier 1 results
- Callers SHOULD call `start_warmup()` early (e.g., during `keel map` tree-sitter phase)
- Callers MUST NOT block on `is_ready()` in a hot path

**Edge production:**
- All resolved edges conform to `GraphEdge` from [[keel-speckit/000-graph-schema/spec|Spec 000]]
- Resolution results written to `resolution_cache` table with `resolution_tier: "tier2_rust"` or `"tier1_rust_heuristic"`
- Confidence scores attached to every produced edge

### Consumed from other specs:

**From [[keel-speckit/000-graph-schema/spec|Spec 000]]:**
- `GraphNode`, `GraphEdge`, `EdgeKind` types
- `GraphStore` trait for reading/writing graph
- `resolution_cache` table schema

**From [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]:**
- Tree-sitter Rust grammar for parsing `.rs` files (used when ra is cold)
- AST queries for `mod`, `use`, `impl`, function declarations
- Call-site extraction for heuristic resolution

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures this resolver populates
- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — provides Rust AST parsing and heuristic fallback
- [[keel-speckit/002-typescript-resolution/spec|Spec 002: TypeScript Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/003-python-resolution/spec|Spec 003: Python Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/004-go-resolution/spec|Spec 004: Go Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — consumes edges produced here
