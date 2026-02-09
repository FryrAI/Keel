# Spec 004: Go Resolution — Tree-sitter Heuristic Tier 2

```yaml
tags: [keel, spec, go, tree-sitter, heuristics, tier-2, resolution]
owner: Agent A (Foundation)
dependencies: [Spec 000 (graph schema), Spec 001 (tree-sitter foundation)]
prd_sections: [10.1]
priority: P1 — Go's explicit module system makes heuristics highly effective
```

## Summary

This spec defines Tier 2 resolution for Go using tree-sitter heuristics only. No external Go analysis library exists for Rust consumption (FFI to `go/packages` or `go/types` is impractical due to Go runtime requirements). Fortunately, Go's language design makes heuristics work remarkably well: imports are explicit (no barrel files, no star imports, no path aliases), package scoping is enforced by the compiler (all cross-package references require the package qualifier), and the capitalization convention unambiguously distinguishes exported (public) from unexported (private) symbols. This targets ~85-92% resolution precision with tree-sitter heuristics alone, sufficient for keel's needs without external tooling.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10.1 Tier 2 Go | Tree-sitter heuristics for Go resolution — no external library needed |
| 10.1 Tier 2 General | Tier 2 resolvers run per-language, fill edges the tree-sitter layer cannot resolve |
| 10.1 Precision | Combined Tier 1 + Tier 2 target: ~85-92% for Go |

---

## Technical Specification

### Scope

**File types:** `.go`

**Resolution capabilities:**
- Package imports (`import "github.com/user/pkg"`)
- Standard library imports (`import "fmt"`, `import "net/http"`)
- Grouped imports (`import ( ... )`)
- Aliased imports (`import alias "github.com/user/pkg"`)
- Dot imports (`import . "pkg"`) — flagged as ambiguous
- Blank imports (`import _ "pkg"`) — tracked but no symbol edges
- Package-qualified function calls (`pkg.Function()`)
- Exported vs unexported visibility (capitalization convention)
- Struct embedding and promoted methods
- Interface method sets

**Out of scope (deferred to Tier 3 LSP):**
- Interface satisfaction checks (which types implement which interfaces)
- Generic type parameter resolution (Go 1.18+ generics)
- `reflect` package usage
- CGo interop
- Build tag / constraint resolution (`//go:build`)

### Why Heuristics Are Sufficient for Go

Go's design philosophy eliminates the ambiguity that plagues resolution in other languages:

1. **No barrel files** — Go has no index file pattern. Every import points to a specific package.
2. **No star imports** — `import . "pkg"` exists but is rare and discouraged by `goimports`.
3. **No path aliases** — Import paths are literal filesystem/module paths. No `tsconfig.json` equivalent.
4. **Explicit package qualifier** — Cross-package calls always use `pkg.Name()`. The call site tells you the package.
5. **Capitalization = visibility** — `Exported` (uppercase first letter) is public. `unexported` (lowercase) is private. No keywords needed.
6. **One package per directory** — No multiple packages in the same directory (except `_test` suffix).

These properties mean that matching `pkg.Function()` to the correct definition requires only:
1. Resolving `pkg` to its import path (from the file's import block)
2. Finding the file in that package that defines `Function`
3. Confirming `Function` is exported (starts with uppercase)

### Go Heuristic Resolver

```rust
pub struct GoHeuristicResolver {
    project_root: PathBuf,
    go_mod: Option<GoMod>,
    package_index: HashMap<String, PackageInfo>,
}

impl GoHeuristicResolver {
    /// Initialize resolver. Parses go.mod for module path.
    pub fn new(project_root: &Path) -> Result<Self, ResolveError> {
        let go_mod = Self::parse_go_mod(project_root)?;
        let package_index = Self::build_package_index(project_root, &go_mod)?;
        Ok(Self {
            project_root: project_root.to_path_buf(),
            go_mod,
            package_index,
        })
    }

    /// Resolve a package-qualified call (e.g., `http.ListenAndServe`).
    pub fn resolve_call(
        &self,
        source_file: &Path,
        package_alias: &str,
        symbol_name: &str,
    ) -> Result<GoResolution, ResolveError> {
        // 1. Find the import path for this alias in the source file's imports
        let import_path = self.find_import_for_alias(source_file, package_alias)?;

        // 2. Look up the package in our index
        let pkg = self.package_index.get(&import_path)
            .ok_or(ResolveError::PackageNotFound(import_path.clone()))?;

        // 3. Find the symbol in the package
        let symbol = pkg.find_exported_symbol(symbol_name)?;

        Ok(GoResolution {
            resolved_file: symbol.file_path.clone(),
            resolved_line: symbol.line,
            import_path,
            is_exported: symbol.is_exported,
            confidence: self.compute_confidence(&symbol),
        })
    }

    /// Build index of all packages and their exported symbols.
    fn build_package_index(
        root: &Path,
        go_mod: &Option<GoMod>,
    ) -> Result<HashMap<String, PackageInfo>, ResolveError> {
        let mut index = HashMap::new();
        // Walk project directories, parse each .go file's package clause
        // and exported symbols using tree-sitter
        // ...
        Ok(index)
    }

    /// Parse go.mod for module path and dependencies.
    fn parse_go_mod(root: &Path) -> Result<Option<GoMod>, ResolveError> {
        let go_mod_path = root.join("go.mod");
        if !go_mod_path.exists() {
            return Ok(None);
        }
        // Parse module path and require directives
        // ...
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct GoResolution {
    pub resolved_file: PathBuf,
    pub resolved_line: u32,
    pub import_path: String,
    pub is_exported: bool,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct GoMod {
    pub module_path: String,        // e.g., "github.com/user/project"
    pub go_version: String,         // e.g., "1.22"
    pub requires: Vec<GoRequire>,   // external dependencies
}

#[derive(Debug, Clone)]
pub struct GoRequire {
    pub path: String,     // e.g., "github.com/spf13/cobra"
    pub version: String,  // e.g., "v1.8.0"
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub import_path: String,
    pub directory: PathBuf,
    pub symbols: Vec<GoSymbol>,
}

#[derive(Debug, Clone)]
pub struct GoSymbol {
    pub name: String,
    pub kind: GoSymbolKind,
    pub file_path: PathBuf,
    pub line: u32,
    pub is_exported: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GoSymbolKind {
    Function,
    Method { receiver: String },
    Type,       // struct, interface, type alias
    Constant,
    Variable,
}
```

### Import Resolution

#### Standard Imports

```go
import (
    "fmt"
    "net/http"
    "github.com/user/project/internal/auth"
)
```

**Strategy:**
1. Parse import block using tree-sitter
2. For each import, determine if it is:
   - **Standard library** (`fmt`, `net/http`) — tracked as external, no source resolution
   - **Project-internal** (matches `go.mod` module path prefix) — resolve to local directory
   - **External dependency** (`github.com/...`) — tracked as external, no source resolution

#### Aliased Imports

```go
import (
    authpkg "github.com/user/project/auth"
    . "github.com/user/project/utils"  // dot import
    _ "github.com/lib/pq"             // blank import (side effects only)
)
```

**Strategy:**
- Aliased: use the alias as the package qualifier in call resolution
- Dot imports: symbols are accessible without qualifier — flag as ambiguous (confidence 0.55)
- Blank imports: create an `Imports` edge but no symbol edges

### Exported vs Unexported Resolution

Go's capitalization convention:

```go
// auth/auth.go
func Authenticate(token string) (*User, error) { ... }  // Exported (uppercase A)
func validateToken(token string) bool { ... }            // Unexported (lowercase v)
```

**Strategy:**
- `is_exported = name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)`
- Exported symbols: full confidence for cross-package resolution
- Unexported symbols: only accessible within the same package. Cross-package references are compile errors — keel flags these.

### Struct Embedding and Promoted Methods

```go
type Server struct {
    http.Handler          // embedded interface
    auth.Authenticator    // embedded struct
}

// Server now has all methods of Handler and Authenticator "promoted"
```

**Strategy:**
1. Detect embedded fields via tree-sitter (field with type but no name)
2. Look up the embedded type's method set
3. Create `Contains` edges from the embedding struct to promoted methods
4. Flag promoted method resolution as lower confidence (0.75) because the actual dispatch depends on method set composition

### Interface Method Resolution

```go
type Writer interface {
    Write(p []byte) (n int, err error)
}
```

**Strategy:**
- Extract interface method signatures via tree-sitter
- When a call site uses an interface type parameter, the concrete implementation is unknown at static analysis time
- **Flag interface method calls as low confidence (0.55)** with annotation: `WARNING: interface dispatch — concrete type unknown`
- Tier 3 (LSP via gopls) can promote these to higher confidence

### Integration with Graph Schema

Tier 2 Go resolution produces:
- `GraphEdge { kind: EdgeKind::Imports }` for package imports
- `GraphEdge { kind: EdgeKind::Calls }` for resolved cross-package function/method calls
- `GraphEdge { kind: EdgeKind::Inherits }` for struct embedding
- `GraphEdge { kind: EdgeKind::Contains }` for package-to-symbol relationships
- Resolution results cached in `resolution_cache` table with `resolution_tier: "tier2_go"`

### Confidence Scoring

| Resolution Path | Confidence |
|----------------|------------|
| Package-qualified call to exported function (`pkg.Func()`) | 0.92 |
| Package-qualified method call (`pkg.Type.Method()`) | 0.90 |
| Same-package function call (unqualified) | 0.88 |
| Aliased import, qualified call | 0.90 |
| Struct embedding, promoted method | 0.75 |
| Dot import symbol (`.` import) | 0.55 |
| Interface method call | 0.55 |
| Standard library call | 0.95 (external, no source resolution) |

---

## Acceptance Criteria

**GIVEN** a Go project with `import "github.com/user/project/auth"` and a call `auth.Login()`
**WHEN** resolution runs
**THEN** the `Calls` edge resolves to the `Login` function in the `auth` package with confidence >= 0.90

**GIVEN** a Go function `func Authenticate()` (uppercase A, exported) and `func validate()` (lowercase v, unexported)
**WHEN** resolution runs on a cross-package call to each
**THEN** `Authenticate` resolves with confidence >= 0.90; `validate` is flagged as a cross-package reference to an unexported symbol (ERROR)

**GIVEN** a struct embedding `http.Handler`
**WHEN** a method from `Handler` is called on the embedding struct
**THEN** the `Calls` edge traces through the embedding with confidence 0.75 and an embedding annotation

**GIVEN** a function parameter typed as an interface (`io.Writer`)
**WHEN** a method is called on that parameter (`w.Write()`)
**THEN** the edge is created with confidence 0.55 and `WARNING: interface dispatch` annotation

**GIVEN** an aliased import `import authpkg "github.com/user/project/auth"` and a call `authpkg.Login()`
**WHEN** resolution runs
**THEN** the alias is correctly mapped to the import path and `Login` resolves normally

**GIVEN** a dot import `import . "github.com/user/project/utils"` and an unqualified call `Hash()`
**WHEN** resolution runs
**THEN** the edge is created with confidence 0.55 and `WARNING: dot import` annotation

---

## Test Strategy

**Oracle:** LSP ground truth (Oracle 1 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])
- Run gopls on test repos
- Capture "Go to Definition" results for all imports and cross-package calls
- Compare keel Tier 2 resolution against gopls results
- Measure precision as: correct edges / total edges produced

**Test repositories:**
- **cobra** (spf13/cobra) — CLI framework, moderate package depth, struct embedding, interface usage
- **fiber** (gofiber/fiber) — HTTP framework, heavy method dispatch, middleware patterns

**Test files to create:**
- `tests/go_resolution/test_package_imports.rs` (~5 tests)
- `tests/go_resolution/test_exported_symbols.rs` (~5 tests)
- `tests/go_resolution/test_aliased_imports.rs` (~3 tests)
- `tests/go_resolution/test_struct_embedding.rs` (~4 tests)
- `tests/go_resolution/test_interface_methods.rs` (~4 tests)
- `tests/go_resolution/test_confidence_scoring.rs` (~4 tests)

**Estimated test count:** ~25

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| Interface dispatch resolution is fundamentally incomplete without type checking | Flag as low confidence (0.55). Tier 3 gopls can promote. Acceptable for keel's use case. |
| Generic type parameters (Go 1.18+) not resolved | Skip generic type resolution entirely. Flag generic calls with `WARNING: generic type parameter`. |
| Build tags (`//go:build linux`) cause conditional compilation | Ignore build tags — parse all files. May create edges to platform-specific code. Acceptable trade-off. |
| `go.mod` replace directives change import resolution | Parse `replace` directives and apply them. Test on repos using `replace`. |
| Internal packages (`internal/`) have restricted visibility | Enforce `internal/` import restrictions in heuristic resolver. Cross-module `internal/` imports flagged as ERROR. |

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> Agent B):

**`GoHeuristicResolver` struct:**
- `new(project_root: &Path) -> Result<Self, ResolveError>` — initialize resolver, parse `go.mod`
- `resolve_call(source: &Path, pkg_alias: &str, symbol: &str) -> Result<GoResolution, ResolveError>` — resolve a package-qualified call

**Package index:**
- `package_index: HashMap<String, PackageInfo>` — all project packages and their exported symbols
- Pre-built during `keel map`, reused across resolution passes

**Edge production:**
- All resolved edges conform to `GraphEdge` from [[keel-speckit/000-graph-schema/spec|Spec 000]]
- Resolution results written to `resolution_cache` table with `resolution_tier: "tier2_go"`
- Confidence scores attached to every produced edge

### Consumed from other specs:

**From [[keel-speckit/000-graph-schema/spec|Spec 000]]:**
- `GraphNode`, `GraphEdge`, `EdgeKind` types
- `GraphStore` trait for reading/writing graph
- `resolution_cache` table schema

**From [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]:**
- Tree-sitter Go grammar for parsing `.go` files
- AST queries for extracting imports, function declarations, method declarations, struct definitions
- Call-site extraction (function/method calls that need resolution)

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures this resolver populates
- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — provides Go AST parsing
- [[keel-speckit/002-typescript-resolution/spec|Spec 002: TypeScript Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/003-python-resolution/spec|Spec 003: Python Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/005-rust-resolution/spec|Spec 005: Rust Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — consumes edges produced here
