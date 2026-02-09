# Spec 001: Tree-sitter Foundation — Universal Parsing Layer

```yaml
tags: [keel, spec, tree-sitter, parsing, tier-1, foundation]
owner: Agent A (Foundation)
dependencies: [keel-speckit/000-graph-schema]
prd_sections: [10.1]
priority: P0 — required before any language-specific resolution (Tier 2+)
```

## Summary

This spec defines keel's Tier 1 universal parsing layer: tree-sitter grammars for TypeScript, Python, Go, and Rust that extract function definitions, class definitions, method definitions, import statements, and call expressions from source files. The parser produces file-level indexes with incremental update support, feeds the graph schema defined in [[keel-speckit/000-graph-schema/spec|Spec 000]], and achieves ~75-92% cross-file resolution accuracy depending on language — sufficient for the structural map before Tier 2 (language-specific) and Tier 3 (LLM-assisted) resolution improve precision further.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10.1 | Tier 1 universal parsing — tree-sitter for all four languages, `tags.scm` query patterns, file-level index, incremental updates, expected precision ranges |

---

## Owner

**Agent A (Foundation)** — responsible for tree-sitter grammar integration, query pattern authoring, the `LanguageResolver` trait, parallel parsing orchestration, and `.keelignore` filtering.

---

## Dependencies

| Dependency | Why |
|-----------|-----|
| [[keel-speckit/000-graph-schema/spec\|Spec 000: Graph Schema]] | Parsed definitions, references, and imports must conform to `GraphNode`, `GraphEdge`, `Import`, and `ExternalEndpoint` structures defined there. The `LanguageResolver` trait produces data that feeds directly into the `GraphStore` trait. |

---

## Full Technical Specification

### Tree-sitter Grammars

Four grammars are required, pinned to specific versions in `Cargo.toml`:

| Crate | Language | File Extensions |
|-------|----------|----------------|
| `tree-sitter-typescript` | TypeScript / JavaScript | `.ts`, `.tsx`, `.js`, `.jsx` |
| `tree-sitter-python` | Python | `.py` |
| `tree-sitter-go` | Go | `.go` |
| `tree-sitter-rust` | Rust | `.rs` |

Each grammar is loaded once at startup and reused across all file parses for that language. Language detection is by file extension, not by content heuristics.

---

### Query Patterns (`tags.scm`)

Each language uses tree-sitter query files (`.scm`) to capture the following five categories:

| Pattern Category | Capture Name | Description |
|-----------------|--------------|-------------|
| Function definitions | `@definition.function` | Top-level and nested function/method declarations |
| Class definitions | `@definition.class` | Classes, structs, traits, interfaces |
| Method definitions | `@definition.method` | Methods bound to a class/struct/impl |
| Import statements | `@import` | All import/require/use statements |
| Call expressions | `@reference.call` | Function and method invocations |

Queries are authored per-language and stored in `queries/{language}/tags.scm` within the keel source tree. They follow tree-sitter's standard query syntax with `@capture` names.

---

### Per-Language Parsing Specifics

#### TypeScript (`.ts`, `.tsx`, `.js`, `.jsx`)

**File handling:**
- `.ts` and `.js` use the TypeScript grammar (superset of JavaScript)
- `.tsx` and `.jsx` use the TSX grammar variant from `tree-sitter-typescript`

**Barrel file detection:**
- An `index.ts` (or `index.js`) that consists primarily of re-export statements (`export { X } from './module'` or `export * from './module'`) is flagged as a barrel file
- Detection heuristic: if >80% of top-level statements are re-exports, mark the file as `is_barrel: true`
- Barrel files create `Imports` edges to every re-exported module, enabling keel to "see through" barrels when resolving call edges
- Barrel re-exports are expanded: a call to `import { foo } from './index'` resolves through the barrel to the original defining module

**Path aliases:**
- keel reads `tsconfig.json` (and `tsconfig.*.json` files) to resolve `compilerOptions.paths` aliases
- Example: `"@auth/*": ["src/auth/*"]` allows `import { verify } from '@auth/jwt'` to resolve to `src/auth/jwt.ts`
- If no `tsconfig.json` is found, path alias resolution is skipped (no error, just unresolved references flagged for Tier 2)
- `baseUrl` is also respected for non-relative imports

**Specifics:**
- Decorators (`@decorator`) on class methods are captured in the definition signature
- Default exports are normalized to the file name as the definition name
- Ambient declarations (`.d.ts` files) are parsed for type information but do not produce `Call` edges

#### Python (`.py`)

**File handling:**
- All `.py` files, including `__init__.py`

**Package detection:**
- Directories containing `__init__.py` are recognized as Python packages
- `__init__.py` contents are parsed — re-exports defined there (e.g., `from .module import Class`) create `Imports` edges, analogous to TypeScript barrel files

**Import handling:**
- Absolute imports: `from package.module import name` — resolved against project root and any configured `src` directories
- Relative imports: `from .sibling import name` or `from ..parent import name` — resolved relative to the current package
- Star imports: `from module import *` — flagged with `imported_names: ["*"]` for Tier 2 resolution. Tier 1 cannot determine which names are actually imported. A warning is emitted: `WARN: Star import from 'module' in 'file.py' — resolution deferred to Tier 2`

**Specifics:**
- Decorators (`@decorator`) captured in the definition signature
- `__all__` list, if present, determines public API for a module (affects `is_public` on definitions)
- Nested functions and classes are captured with their enclosing scope in the signature
- Type hints (PEP 484) are extracted when present and included in the signature

#### Go (`.go`)

**File handling:**
- All `.go` files, excluding `_test.go` by default (test files are included only if explicitly configured)

**Package-level scoping:**
- Go files in the same directory share a package namespace — all top-level definitions in `package auth` are in scope for each other without explicit imports
- The module path is derived from `go.mod` at the project root

**Visibility:**
- Capitalized identifiers are exported (`is_public: true`): `func ProcessOrder()` is public, `func processOrder()` is not
- No visibility keyword needed — capitalization is the sole signal

**Import handling:**
- Go imports are explicit and unambiguous: `import "github.com/org/repo/pkg"` maps directly to a module path
- Aliased imports: `import auth "github.com/org/repo/auth"` — the alias is tracked
- Dot imports: `import . "pkg"` — treated like Python star imports, flagged for Tier 2
- Blank imports: `import _ "pkg"` — recorded as an import edge but no names are resolved

**Specifics:**
- Interface definitions are captured as `NodeKind::Class` with a distinguishing tag in the signature
- Struct methods (receiver functions) are linked to their struct definition via the receiver type
- `init()` functions are captured but flagged as non-callable (no direct call edges)

#### Rust (`.rs`)

**File handling:**
- All `.rs` files

**Module declarations:**
- `mod foo;` in `lib.rs` or `main.rs` declares a submodule — keel resolves this to either `foo.rs` or `foo/mod.rs`
- Inline modules (`mod foo { ... }`) are parsed as nested scopes
- The module tree is reconstructed from `mod` declarations starting at crate roots (`lib.rs`, `main.rs`)

**Use statements:**
- `use crate::module::Name` — resolved relative to the crate root
- `use super::Name` — resolved relative to the parent module
- `use self::Name` — resolved within the current module
- `use module::*` — star import, flagged for Tier 2
- Re-exports: `pub use crate::internal::Type` — creates an additional public definition at the re-export site

**Visibility:**
- `pub` — public to the crate and dependents (`is_public: true`)
- `pub(crate)` — public within the crate (`is_public: true`, but tagged as crate-scoped)
- `pub(super)` — public to the parent module
- No modifier — private to the current module (`is_public: false`)

**Specifics:**
- `impl` blocks link methods to their struct/enum: each method in `impl Foo { fn bar() {} }` produces a definition with `Foo::bar` as the qualified name
- Trait implementations (`impl Trait for Struct`) create `Inherits` edges
- Macro invocations (`macro!()`) are captured as call expressions but resolution is deferred to Tier 2 (macros can generate arbitrary code)
- `#[derive(...)]` attributes are noted but do not produce call edges at Tier 1

---

### File-Level Index with Incremental Updates

Each file produces a `FileIndex`:

```
pub struct FileIndex {
    pub file_path: String,
    pub content_hash: u64,         // xxHash64 of raw file content
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
    pub external_endpoints: Vec<ExternalEndpoint>,
    pub parse_duration_us: u64,    // microseconds, for perf monitoring
}
```

**Incremental update strategy:**
1. On `keel compile`, hash every file in the project using xxHash64
2. Compare against stored `content_hash` in the previous index
3. Only re-parse files whose hash has changed
4. For changed files, fully re-parse (tree-sitter incremental parsing is used internally but the output is a complete `FileIndex` replacement)
5. Target: <1ms per unchanged file (hash check only), <50ms per changed file (full re-parse)

**Cache invalidation:**
- If `tsconfig.json` changes, all TypeScript files are re-parsed (path aliases may have changed)
- If `go.mod` changes, all Go files are re-parsed (module path may have changed)
- If `.keelignore` changes, the file list is recomputed before hashing

---

### Parallel Parsing with Rayon

File parsing is embarrassingly parallel — each file is independent at Tier 1:

```
use rayon::prelude::*;

files_to_parse
    .par_iter()
    .map(|file| {
        let content = fs::read_to_string(file)?;
        let resolver = get_resolver_for_extension(file);
        resolver.parse_file(file, &content)
    })
    .collect::<Vec<ParseResult>>()
```

**Parallelism constraints:**
- Tree-sitter parsers are not `Send` — each thread creates its own parser instance
- Grammar objects are `Send + Sync` and shared via `Arc`
- Thread count defaults to `num_cpus` but is configurable via `config.toml`
- Memory ceiling: each parser instance uses ~2MB; for 16 threads, ~32MB overhead

**Performance targets:**
- 10,000 files: <5 seconds on 8-core machine
- 1,000 files (incremental, 50 changed): <500ms

---

### Expected Precision

| Language | Tier 1 Resolution | Notes |
|----------|-------------------|-------|
| TypeScript | ~80-85% | Path aliases and barrel files reduce precision without tsconfig parsing |
| Python | ~75-80% | Star imports and dynamic imports are unresolvable at Tier 1 |
| Go | ~88-92% | Explicit imports and capitalization rules make Go the easiest language |
| Rust | ~82-87% | Macro-generated code and complex module trees reduce precision slightly |

These numbers represent the percentage of call sites that can be resolved to a specific definition using Tier 1 (tree-sitter + import resolution) alone. Unresolved call sites are flagged with `resolved_to: None` and deferred to Tier 2 / Tier 3.

---

### The `LanguageResolver` Trait

This is the core abstraction. Each language implements this trait. Agent B (Resolution) consumes it via the `GraphStore` interface defined in [[keel-speckit/000-graph-schema/spec|Spec 000]].

```
pub trait LanguageResolver {
    fn language(&self) -> &str;
    fn parse_file(&self, path: &Path, content: &str) -> ParseResult;
    fn resolve_definitions(&self, file: &Path) -> Vec<Definition>;
    fn resolve_references(&self, file: &Path) -> Vec<Reference>;
    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge>;
}

pub struct ParseResult {
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
    pub external_endpoints: Vec<ExternalEndpoint>,
}

pub struct Definition {
    pub name: String,
    pub kind: NodeKind,
    pub signature: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub docstring: Option<String>,
    pub is_public: bool,
    pub type_hints_present: bool,
    pub body_text: String,  // for hash computation
}

pub struct Reference {
    pub name: String,
    pub file_path: String,
    pub line: u32,
    pub kind: ReferenceKind,  // Call, Import, TypeRef
    pub resolved_to: Option<String>,  // resolved definition file_path if known
}

pub struct Import {
    pub source: String,          // module path
    pub imported_names: Vec<String>,  // specific names, or ["*"] for star
    pub file_path: String,
    pub line: u32,
    pub is_relative: bool,
}
```

**`ReferenceKind` enum:**
```
pub enum ReferenceKind {
    Call,       // function/method invocation
    Import,    // import statement reference
    TypeRef,   // type annotation reference
}
```

**`CallSite` and `ResolvedEdge`:**
```
pub struct CallSite {
    pub file_path: String,
    pub line: u32,
    pub callee_name: String,
    pub receiver: Option<String>,  // e.g., "self", "foo" for foo.bar()
}

pub struct ResolvedEdge {
    pub target_file: String,
    pub target_name: String,
    pub confidence: f64,  // 0.0-1.0, Tier 1 typically 0.7-0.9
}
```

---

### `.keelignore` Handling

keel respects a `.keelignore` file at the project root, using gitignore-style syntax (powered by the `ignore` crate, same as ripgrep).

**Default exclusions** (applied even without a `.keelignore` file):

```
# Generated code
generated/
**/generated/

# Dependency directories
vendor/
node_modules/

# Database migrations (generated SQL, not application logic)
**/migrations/

# Build output
dist/
build/
.next/
target/

# Python bytecode
__pycache__/
*.pyc

# Test fixtures (optional — can be overridden)
**/testdata/
**/fixtures/
```

**Behavior:**
- `.keelignore` is additive to defaults — it cannot un-ignore a default exclusion
- To override a default exclusion, use `!pattern` (negation), e.g., `!**/migrations/` to include migrations
- `.keelignore` is checked once at the start of `keel map` / `keel compile` and cached for the duration of the run
- Patterns are matched against paths relative to the project root
- If `.gitignore` exists, its patterns are also respected (union of `.gitignore` and `.keelignore`)

---

## Acceptance Criteria

**AC-1: Parse all four languages**
**GIVEN** a project containing `.ts`, `.py`, `.go`, and `.rs` files
**WHEN** `keel map` is run
**THEN** all four languages are detected, parsed, and produce non-empty `FileIndex` entries with definitions and references

**AC-2: Extract function definitions**
**GIVEN** a TypeScript file containing `export function processOrder(order: Order): Result { ... }`
**WHEN** the file is parsed
**THEN** a `Definition` is produced with `name: "processOrder"`, `kind: NodeKind::Function`, `is_public: true`, `type_hints_present: true`, and a signature containing the parameter and return types

**AC-3: Extract class and method definitions**
**GIVEN** a Python file containing `class UserService:` with methods `def create_user(self, data: dict) -> User:`
**WHEN** the file is parsed
**THEN** a `Definition` with `kind: NodeKind::Class` is produced for `UserService`, and a `Definition` with `kind: NodeKind::Function` is produced for `create_user` with the enclosing class in its qualified name

**AC-4: Extract call sites**
**GIVEN** a Go file containing `result := auth.ValidateToken(token)`
**WHEN** the file is parsed
**THEN** a `Reference` is produced with `name: "ValidateToken"`, `kind: ReferenceKind::Call`, and the receiver `auth` is recorded for resolution

**AC-5: Detect and resolve imports**
**GIVEN** a Rust file containing `use crate::auth::validate_token;`
**WHEN** the file is parsed
**THEN** an `Import` is produced with `source: "crate::auth"`, `imported_names: ["validate_token"]`, `is_relative: false`

**AC-6: Handle incremental updates**
**GIVEN** a project with 1,000 files previously parsed, where 10 files have changed
**WHEN** `keel compile` is run
**THEN** only the 10 changed files are re-parsed, unchanged files retain their cached `FileIndex`, and total compile time is under 1 second

**AC-7: Parallel parsing performance**
**GIVEN** a project with 5,000 source files across all four languages
**WHEN** `keel map` is run on a machine with 8+ cores
**THEN** parsing completes in under 3 seconds, utilizing multiple CPU cores (observable via thread count in debug logs)

**AC-8: `.keelignore` respect**
**GIVEN** a project with a `.keelignore` containing `internal/legacy/`
**WHEN** `keel map` is run
**THEN** no files under `internal/legacy/` appear in the graph, and default exclusions (`node_modules/`, `vendor/`, etc.) are also excluded

**AC-9: Barrel file detection (TypeScript)**
**GIVEN** a TypeScript project with `src/utils/index.ts` containing only re-export statements
**WHEN** the file is parsed
**THEN** the file is flagged as a barrel file, and imports from `'./utils'` are resolved through the barrel to the original defining modules

**AC-10: Star import flagging**
**GIVEN** a Python file containing `from utils import *`
**WHEN** the file is parsed
**THEN** an `Import` is produced with `imported_names: ["*"]`, and a warning is emitted indicating resolution is deferred to Tier 2

---

## Test Strategy

**Oracle:** LSP ground truth (Oracle 1 from [[keel-speckit/test-harness/README|Test Harness]])
- For each test project, run the language's LSP (tsserver, pyright, gopls, rust-analyzer) to produce a ground truth set of definitions and references
- Compare keel Tier 1 output against LSP output: measure precision and recall
- Acceptable thresholds: precision > 95% (what we report is correct), recall > 75% (we find most things — Tier 2/3 fill the gap)

**Test files to create:**

| Test File | Focus | Est. Tests |
|-----------|-------|-----------|
| `tests/parsing/test_typescript_parser.rs` | TS/TSX/JS/JSX parsing, barrel files, path aliases | ~10 |
| `tests/parsing/test_python_parser.rs` | Python parsing, `__init__.py`, relative imports, star imports | ~10 |
| `tests/parsing/test_go_parser.rs` | Go parsing, package scoping, capitalization visibility, receiver methods | ~8 |
| `tests/parsing/test_rust_parser.rs` | Rust parsing, mod declarations, use statements, pub visibility, impl blocks | ~8 |
| `tests/parsing/test_incremental_update.rs` | File change detection, cache invalidation, config change triggers | ~6 |
| `tests/parsing/test_parallel_parsing.rs` | Rayon parallelism, thread safety, performance benchmarks | ~5 |
| `tests/parsing/test_keelignore.rs` | Pattern matching, default exclusions, negation, gitignore union | ~6 |
| `tests/parsing/test_language_resolver_trait.rs` | Trait contract compliance for all four implementations | ~7 |

**Estimated total:** ~60 tests

**Test fixture projects:**
- `fixtures/ts-project/` — TypeScript project with barrel files, path aliases, decorators
- `fixtures/py-project/` — Python project with packages, relative imports, star imports
- `fixtures/go-project/` — Go project with multiple packages, interfaces, receiver methods
- `fixtures/rs-project/` — Rust project with module tree, traits, impl blocks, macros
- `fixtures/mixed-project/` — Project with all four languages for integration testing

---

## Known Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Tree-sitter grammar version compatibility | Query patterns may break across grammar updates | Pin grammar versions in `Cargo.toml`. CI test against pinned versions. Update grammars deliberately with query pattern review. |
| Query pattern coverage gaps | Some language constructs not captured by `tags.scm` patterns | Start with well-tested upstream `tags.scm` files. Add custom patterns incrementally. Track "uncaptured construct" reports during dogfooding. |
| Barrel file complexity (TypeScript) | Deeply nested or circular barrel re-exports may cause infinite loops or missed resolutions | Depth limit of 5 for barrel traversal. Cycle detection with visited set. Unresolved barrels flagged for Tier 2. |
| Star import ambiguity (Python, Rust) | `from module import *` / `use module::*` cannot be resolved at Tier 1 | Flag explicitly, defer to Tier 2. Emit warnings so users know resolution is incomplete. Count star imports in module profile metrics. |
| Macro-generated code (Rust) | `macro_rules!` and proc macros can generate arbitrary definitions and calls invisible to tree-sitter | Capture macro invocations as call expressions. Defer macro expansion to Tier 2 / Tier 3 (LLM). Document known blind spots. |
| `tsconfig.json` variants | Projects may use `tsconfig.build.json`, `tsconfig.app.json`, monorepo configs | Read `tsconfig.json` first, then `extends` chains. Support explicit config path in `keel.toml`. |
| Large file performance | Files >10,000 lines may slow parsing | Set a configurable size limit (default 100,000 lines). Warn on files exceeding threshold. Parse them anyway but track duration. |

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> Agent B):

**`LanguageResolver` trait** — the primary interface consumed by the resolution engine (Tier 2, Tier 3) and the graph builder.

Agent B (Resolution) calls `LanguageResolver::parse_file()` to get raw `ParseResult` data, then feeds it into the `GraphStore` (from [[keel-speckit/000-graph-schema/spec|Spec 000]]) to populate nodes and edges.

The contract:
- `parse_file()` is **pure**: given the same file content, it always returns the same `ParseResult`
- `parse_file()` is **file-scoped**: it does not read other files or perform cross-file resolution
- `resolve_call_edge()` performs **single-hop** resolution using import tables built from the current file's imports and the project's file index. It does not recurse into transitive dependencies.
- `resolve_definitions()` and `resolve_references()` are convenience methods that extract subsets of `parse_file()` output

### Dependencies consumed:

| Source Spec | What We Consume |
|------------|----------------|
| [[keel-speckit/000-graph-schema/spec\|Spec 000]] | `GraphNode`, `GraphEdge`, `NodeKind`, `EdgeKind`, `ExternalEndpoint`, `ModuleProfile` — all parsed data is shaped to fit these structures |

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — the data structures this parser populates
- [[keel-speckit/002-typescript-resolution/spec|Spec 002: TypeScript Resolution]] — Tier 2 resolution for TypeScript, consumes Tier 1 output
- [[keel-speckit/003-python-resolution/spec|Spec 003: Python Resolution]] — Tier 2 resolution for Python
- [[keel-speckit/004-go-resolution/spec|Spec 004: Go Resolution]] — Tier 2 resolution for Go
- [[keel-speckit/005-rust-resolution/spec|Spec 005: Rust Resolution]] — Tier 2 resolution for Rust
- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — `keel map` and `keel compile` invoke this parsing layer
