# Spec 002: TypeScript/JavaScript Resolution — Oxc-Powered Tier 2

```yaml
tags: [keel, spec, typescript, javascript, oxc, tier-2, resolution]
owner: Agent A (Foundation)
dependencies: [Spec 000 (graph schema), Spec 001 (tree-sitter foundation)]
prd_sections: [10.1]
priority: P1 — first language-specific resolver, validates Tier 2 architecture
```

## Summary

This spec defines Tier 2 resolution for TypeScript and JavaScript via the Oxc toolchain. `oxc_resolver` handles import/module resolution (barrel files, path aliases from tsconfig.json, re-exports, node_modules traversal), while `oxc_semantic` provides per-file symbol tables. Cross-file symbol stitching remains with tree-sitter queries from [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]. Oxc is production-ready (v0.111+), MIT-licensed, and benchmarked at 30x faster than webpack's enhanced-resolve. It is the resolver backing Rolldown and Biome. Combined with Tier 1, this targets ~85-93% resolution precision on real-world TypeScript/JavaScript codebases.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10.1 Tier 2 TS | Oxc integration for TS/JS resolution — `oxc_resolver` for imports, `oxc_semantic` for per-file symbols |
| 10.1 Tier 2 General | Tier 2 resolvers run per-language, fill edges the tree-sitter layer cannot resolve |
| 10.1 Precision | Combined Tier 1 + Tier 2 target: ~85-93% for TypeScript |

---

## Technical Specification

### Scope

**File types:** `.ts`, `.tsx`, `.js`, `.jsx`

**Resolution capabilities:**
- Static imports (`import { X } from './module'`)
- Dynamic imports (`import('./module')`) — resolved statically where path is a string literal
- Re-exports (`export { X } from './other'`, `export * from './barrel'`)
- Barrel files (`index.ts` / `index.js` re-exports)
- Path aliases from `tsconfig.json` (`paths`, `baseUrl`)
- `node_modules` resolution (standard Node algorithm)
- Type imports (`import type { X }`)
- Namespace imports (`import * as ns from './mod'`)

**Out of scope (deferred to Tier 3 LSP):**
- Generic type parameter resolution
- Complex conditional types
- Declaration merging across files
- Runtime-computed import paths

### Oxc Integration

#### `oxc_resolver` — Import Resolution

```rust
use oxc_resolver::{ResolveOptions, Resolver};

pub struct TsResolver {
    resolver: Resolver,
    tsconfig_paths: Option<TsconfigPaths>,
}

impl TsResolver {
    /// Create a resolver for the given project root.
    /// Reads tsconfig.json if present for path aliases and baseUrl.
    pub fn new(project_root: &Path) -> Result<Self, ResolveError> {
        let tsconfig = Self::read_tsconfig(project_root)?;
        let options = ResolveOptions {
            extensions: vec![
                ".ts".into(), ".tsx".into(),
                ".js".into(), ".jsx".into(),
                ".d.ts".into(),
            ],
            main_fields: vec!["module".into(), "main".into()],
            condition_names: vec!["import".into(), "require".into()],
            tsconfig: tsconfig.clone(),
            ..Default::default()
        };
        let resolver = Resolver::new(options);
        Ok(Self { resolver, tsconfig_paths: tsconfig })
    }

    /// Resolve an import specifier from a source file to an absolute path.
    pub fn resolve_import(
        &self,
        source_file: &Path,
        specifier: &str,
    ) -> Result<ResolvedImport, ResolveError> {
        let source_dir = source_file.parent().unwrap();
        match self.resolver.resolve(source_dir, specifier) {
            Ok(resolution) => Ok(ResolvedImport {
                resolved_path: resolution.path().to_path_buf(),
                is_barrel: Self::is_barrel_reexport(&resolution),
                is_type_only: false, // caller sets this from AST context
            }),
            Err(e) => Err(ResolveError::NotFound {
                specifier: specifier.to_string(),
                source: source_file.to_path_buf(),
                inner: e,
            }),
        }
    }

    /// Detect barrel file re-exports (index.ts/index.js).
    fn is_barrel_reexport(resolution: &oxc_resolver::Resolution) -> bool {
        resolution.path()
            .file_stem()
            .map(|s| s == "index")
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub resolved_path: PathBuf,
    pub is_barrel: bool,
    pub is_type_only: bool,
}
```

#### `oxc_semantic` — Per-File Symbol Tables

```rust
use oxc_semantic::SemanticBuilder;

pub struct TsSymbolExtractor;

impl TsSymbolExtractor {
    /// Extract per-file symbol table.
    /// IMPORTANT: This is strictly per-file. Cross-file stitching
    /// uses tree-sitter queries from Spec 001.
    pub fn extract_symbols(
        source: &str,
        file_path: &Path,
    ) -> Result<FileSymbols, SymbolError> {
        let allocator = oxc_allocator::Allocator::default();
        let source_type = Self::detect_source_type(file_path);
        let parser_ret = oxc_parser::Parser::new(&allocator, source, source_type).parse();
        let semantic_ret = SemanticBuilder::new()
            .build(&parser_ret.program);

        let symbols = semantic_ret.semantic.symbols();
        let scopes = semantic_ret.semantic.scopes();

        let mut file_symbols = FileSymbols::new(file_path.to_path_buf());

        for (symbol_id, _) in symbols.iter() {
            let name = symbols.get_name(symbol_id);
            let scope_id = symbols.get_scope_id(symbol_id);
            let is_exported = Self::is_symbol_exported(symbols, symbol_id);

            file_symbols.add(PerFileSymbol {
                name: name.to_string(),
                scope_depth: scopes.get_depth(scope_id),
                is_exported,
                span: symbols.get_span(symbol_id),
            });
        }

        Ok(file_symbols)
    }
}

#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub file_path: PathBuf,
    pub symbols: Vec<PerFileSymbol>,
}

#[derive(Debug, Clone)]
pub struct PerFileSymbol {
    pub name: String,
    pub scope_depth: u32,
    pub is_exported: bool,
    pub span: oxc_span::Span,
}
```

### Barrel File Handling

Barrel files (`index.ts` / `index.js`) are a common TypeScript pattern where a directory re-exports symbols from internal modules:

```typescript
// src/components/index.ts (barrel file)
export { Button } from './Button';
export { Input } from './Input';
export * from './utils';
```

**Resolution strategy:**
1. `oxc_resolver` resolves `'./components'` to `./components/index.ts` natively
2. The barrel file's re-exports are parsed to trace to the actual definition file
3. Each re-exported symbol gets an `Imports` edge from the importing file to the defining file (not the barrel)
4. `export *` (star re-exports) are resolved by reading the target module's exports

### Path Alias Handling

Path aliases from `tsconfig.json`:

```json
{
  "compilerOptions": {
    "baseUrl": "./src",
    "paths": {
      "@components/*": ["components/*"],
      "@utils/*": ["utils/*"],
      "@/*": ["*"]
    }
  }
}
```

**Resolution strategy:**
1. On `TsResolver::new()`, parse `tsconfig.json` and extract `paths` + `baseUrl`
2. `oxc_resolver` handles alias expansion natively when configured with tsconfig
3. Extended tsconfig (`"extends": "./tsconfig.base.json"`) is followed by `oxc_resolver`

### Type Import Handling

```typescript
import type { User } from './models';  // type-only import
import { type Role, createUser } from './auth';  // inline type import
```

**Strategy:**
- Type-only imports (`import type`) generate `Imports` edges with a `type_only: true` annotation
- Inline type imports are split: value imports get normal edges, type imports get type-only edges
- Type-only edges are still tracked in the graph but can be filtered in output (e.g., for runtime-only dependency analysis)

### Namespace Import Handling

```typescript
import * as auth from './auth';
auth.login();  // resolved via namespace member access
```

**Strategy:**
1. Resolve `'./auth'` to the target file
2. Create an `Imports` edge from the importing file to the target module
3. Member accesses (`auth.login`) are resolved by matching the member name against the target module's exported symbols
4. Unresolvable members are flagged with low confidence and deferred to Tier 3

### Integration with Graph Schema

Tier 2 TS resolution produces:
- `GraphEdge { kind: EdgeKind::Imports }` for import statements
- `GraphEdge { kind: EdgeKind::Calls }` for resolved cross-file function calls
- `GraphEdge { kind: EdgeKind::Inherits }` for `extends`/`implements` across files
- Resolution results cached in `resolution_cache` table from [[keel-speckit/000-graph-schema/spec|Spec 000]]

### Confidence Scoring

| Resolution Path | Confidence |
|----------------|------------|
| Direct named import, resolved by `oxc_resolver` | 0.95 |
| Barrel file re-export, fully traced | 0.90 |
| Path alias, resolved via tsconfig | 0.93 |
| `export *` star re-export (single match) | 0.88 |
| `export *` star re-export (ambiguous — multiple sources) | 0.60 |
| Namespace member access | 0.80 |
| Dynamic import with string literal | 0.85 |
| Dynamic import with template literal / variable | 0.00 (skip) |

---

## Acceptance Criteria

**GIVEN** a TypeScript project with barrel files (`src/components/index.ts` re-exporting from `./Button`, `./Input`)
**WHEN** a file imports `{ Button }` from `'./components'`
**THEN** the `Imports` edge resolves to `src/components/Button.ts` (not `index.ts`) with confidence >= 0.90

**GIVEN** a `tsconfig.json` with path alias `"@utils/*": ["src/utils/*"]`
**WHEN** a file imports `{ hash }` from `'@utils/crypto'`
**THEN** the `Imports` edge resolves to `src/utils/crypto.ts` with confidence >= 0.93

**GIVEN** a module with `export { X } from './other'` re-export
**WHEN** a consumer imports `{ X }` from that module
**THEN** the resolution traces through the re-export to `./other.ts` as the definition source

**GIVEN** a file with `import type { User } from './models'`
**WHEN** resolution runs
**THEN** an `Imports` edge is created with `type_only: true` annotation

**GIVEN** a file with `import * as auth from './auth'` and a call `auth.login()`
**WHEN** resolution runs
**THEN** a `Calls` edge is created from the calling function to `auth.login` with confidence >= 0.80

**GIVEN** a barrel file with `export * from './a'` and `export * from './b'` where both export `helper`
**WHEN** a consumer imports `{ helper }`
**THEN** the edge is created with confidence 0.60 and a `WARNING: ambiguous star re-export` annotation

**GIVEN** a TypeScript project with `node_modules` dependency `lodash-es`
**WHEN** a file imports `{ debounce } from 'lodash-es'`
**THEN** the import resolves to the `node_modules/lodash-es` entry point (external dependency tracked as external node)

**GIVEN** a project with extended tsconfig (`"extends": "./tsconfig.base.json"`) defining `baseUrl`
**WHEN** resolution runs using a path relative to `baseUrl`
**THEN** the import resolves correctly using the inherited configuration

---

## Test Strategy

**Oracle:** LSP ground truth (Oracle 1 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])
- Run TypeScript Language Server on test repos
- Capture "Go to Definition" results for all imports and cross-file calls
- Compare keel Tier 2 resolution against LSP results
- Measure precision as: correct edges / total edges produced

**Test repositories:**
- **excalidraw** — heavy barrel file usage, path aliases, re-exports
- **cal.com** — monorepo with complex tsconfig paths, workspace packages
- **typescript-eslint** — deep re-export chains, namespace imports, type-only imports

**Test files to create:**
- `tests/ts_resolution/test_barrel_files.rs` (~8 tests)
- `tests/ts_resolution/test_path_aliases.rs` (~6 tests)
- `tests/ts_resolution/test_reexports.rs` (~6 tests)
- `tests/ts_resolution/test_type_imports.rs` (~5 tests)
- `tests/ts_resolution/test_namespace_imports.rs` (~5 tests)
- `tests/ts_resolution/test_node_modules.rs` (~4 tests)
- `tests/ts_resolution/test_confidence_scoring.rs` (~4 tests)
- `tests/ts_resolution/test_lsp_ground_truth.rs` (~6 tests — integration)

**Estimated test count:** ~44

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| Oxc API changes between v0.111+ releases | Pin exact version in Cargo.toml. Oxc follows semver for resolver crate. |
| Barrel file chains deeper than 3 levels | Cap traversal depth at 5. Log WARNING if exceeded. Rare in practice. |
| tsconfig `paths` with complex glob patterns | `oxc_resolver` handles standard patterns. Edge cases deferred to Tier 3 LSP. |
| `export *` from many modules causes combinatorial explosion | Limit star re-export tracing to 10 source modules per barrel. Flag as low confidence beyond that. |
| Monorepo workspace resolution (yarn/pnpm/npm workspaces) | `oxc_resolver` supports `node_modules` hoisting. Test explicitly on cal.com monorepo. |

---

## Inter-Agent Contracts

### Exposed by this spec (Agent A -> Agent B):

**`TsResolver` struct:**
- `new(project_root: &Path) -> Result<Self, ResolveError>` — initialize resolver with tsconfig
- `resolve_import(source_file: &Path, specifier: &str) -> Result<ResolvedImport, ResolveError>` — resolve a single import

**`TsSymbolExtractor` struct:**
- `extract_symbols(source: &str, file_path: &Path) -> Result<FileSymbols, SymbolError>` — per-file symbol extraction

**Edge production:**
- All resolved edges conform to `GraphEdge` from [[keel-speckit/000-graph-schema/spec|Spec 000]]
- Resolution results written to `resolution_cache` table with `resolution_tier: "tier2_ts"`
- Confidence scores attached to every produced edge

### Consumed from other specs:

**From [[keel-speckit/000-graph-schema/spec|Spec 000]]:**
- `GraphNode`, `GraphEdge`, `EdgeKind` types
- `GraphStore` trait for reading/writing graph
- `resolution_cache` table schema

**From [[keel-speckit/001-treesitter-foundation/spec|Spec 001]]:**
- Tree-sitter parsed ASTs for cross-file symbol stitching
- Call-site extraction (function calls that need cross-file resolution)
- File-level module node creation

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures this resolver populates
- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — provides ASTs and call-site extraction
- [[keel-speckit/003-python-resolution/spec|Spec 003: Python Resolution]] — sibling Tier 2 resolver (different toolchain)
- [[keel-speckit/004-go-resolution/spec|Spec 004: Go Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/005-rust-resolution/spec|Spec 005: Rust Resolution]] — sibling Tier 2 resolver
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — consumes edges produced here
