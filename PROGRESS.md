# keel — Implementation Progress

> Last updated: 2026-02-09

## Phase 0: Contracts & Scaffold — COMPLETE

- [x] Workspace `Cargo.toml` with all 6 crates
- [x] `keel-core` — graph types (`NodeKind`, `EdgeKind`, `StructuralNode`, `StructuralEdge`)
- [x] `keel-core` — hash computation (`base62(xxhash64(...))`, 11-char deterministic hashes)
- [x] `keel-core` — `GraphStore` trait + SQLite implementation
- [x] `keel-parsers` — `LanguageResolver` trait (frozen contract)
- [x] `keel-parsers` — language module stubs (TypeScript, Python, Go, Rust)
- [x] `keel-enforce` — `CompileResult`, `DiscoverResult`, `ExplainResult` structs
- [x] `keel-enforce` — error code types (E001-E005, W001-W002, S001)
- [x] `keel-output` — JSON, LLM, human formatter stubs
- [x] `keel-cli` — main entry point stub
- [x] `keel-server` — lib stub
- [x] `.keel/config.toml` — configuration template
- [x] `.keelignore` — default ignore patterns
- [x] `.github/workflows/ci.yml` — CI pipeline (check, test, clippy, fmt)
- [x] `extensions/vscode/` — VS Code extension manifest + entry stub
- [x] 13 unit tests passing (hash + SQLite store)

**Gate:** All frozen contracts defined. All crates compile. All tests pass.

## Phase 1: Tree-sitter Foundation — IN PROGRESS

### Spec 001: Tree-sitter Foundation — COMPLETE

- [x] `TreeSitterParser` — multi-language parser with query-based extraction
- [x] Language detection from file extension
- [x] Query patterns for TypeScript, Python, Go, Rust (`.scm` files)
- [x] Function, class, import, and call extraction via tree-sitter queries
- [x] `FileWalker` — parallel file discovery with `.keelignore` support
- [x] `streaming-iterator` dependency for tree-sitter cursor traversal
- [x] 11 parser unit tests passing (language detection, parsing, walker)

### Spec 002: TypeScript Resolution (Oxc) — PENDING

- [ ] `oxc_resolver` integration for module resolution
- [ ] `oxc_semantic` for type-aware call graph edges
- [ ] Import → export linking
- [ ] Re-export chain resolution

### Spec 003: Python Resolution (ty) — PENDING

- [ ] `ty --output-format json` subprocess integration
- [ ] Import resolution (absolute, relative, package)
- [ ] Type hint extraction and validation

### Spec 004: Go Resolution — PENDING

- [ ] Package-level resolution heuristics
- [ ] Interface method dispatch (low confidence edges)
- [ ] Import path resolution

### Spec 005: Rust Resolution — PENDING

- [ ] rust-analyzer lazy-load integration
- [ ] Trait dispatch resolution
- [ ] Module path resolution

## Phase 2: Enforcement Engine — PENDING

### Spec 006: Enforcement Engine

- [ ] `keel compile` validation pipeline
- [ ] Error code matching (E001-E005, W001-W002)
- [ ] `fix_hint` generation for every ERROR
- [ ] Confidence scoring (0.0-1.0)
- [ ] Circuit breaker (3 consecutive failures → auto-downgrade)
- [ ] Batch mode (`--batch-start` / `--batch-end`)

### Spec 007: CLI Commands

- [ ] `keel init` — repo initialization
- [ ] `keel map` — full structural re-map
- [ ] `keel compile [file...]` — incremental validation
- [ ] `keel discover <hash>` — adjacency lookup
- [ ] `keel where <hash>` — hash → file:line
- [ ] `keel explain <code> <hash>` — resolution chain
- [ ] `keel deinit` — clean removal
- [ ] `keel stats` — telemetry dashboard

### Spec 008: Output Formats

- [ ] JSON output with schema validation
- [ ] LLM-optimized compact output
- [ ] Human-readable terminal output
- [ ] `--format` flag support

## Phase 3: Server & Integrations — PENDING

### Spec 009: Tool Integration

- [ ] Claude Code pre-tool-use hook config
- [ ] Cursor rules file
- [ ] Windsurf, Cline, Copilot, Aider configs
- [ ] Generic `.keel/hooks/` system

### Spec 010: MCP + HTTP Server

- [ ] `keel serve --mcp` — MCP tool server
- [ ] `keel serve --http` — HTTP REST API
- [ ] File watcher integration (notify crate)
- [ ] WebSocket live updates

### Spec 011: VS Code Extension

- [ ] Inline diagnostics from keel violations
- [ ] Code actions for fix hints
- [ ] Hash decorations on symbols
- [ ] Status bar integration

## Phase 4: Distribution — PENDING

### Spec 012: Cross-platform Distribution

- [ ] Linux (x86_64, aarch64) binaries
- [ ] macOS (x86_64, aarch64) binaries
- [ ] Windows (x86_64) binaries
- [ ] `cargo install keel-cli`
- [ ] Homebrew formula
- [ ] npm wrapper package

## Test Summary

| Crate | Tests | Status |
|-------|-------|--------|
| keel-core | 13 | All passing |
| keel-parsers | 11 | All passing |
| keel-enforce | 0 | Stubs only |
| keel-output | 0 | Stubs only |
| keel-cli | 0 | Stub only |
| keel-server | 0 | Stub only |
| **Total** | **24** | **All passing** |

## Milestone Gates

| Gate | Criteria | Status |
|------|----------|--------|
| M1 | `keel init` + `keel map` work on 10k LOC TS repo | Not reached |
| M2 | `keel compile` catches broken caller in <200ms | Not reached |
| M3 | Full 4-oracle test suite passes | Not reached |
