<p align="center">
  <h1 align="center">keel</h1>
  <p align="center">
    <strong>Structural code enforcement for LLM coding agents</strong>
  </p>
  <p align="center">
    <a href="https://github.com/FryrAI/Keel/actions/workflows/ci.yml"><img src="https://github.com/FryrAI/Keel/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <img src="https://img.shields.io/badge/rust-1.75%2B-orange" alt="Rust 1.75+">
    <img src="https://img.shields.io/badge/license-FSL--1.1--MIT-blue" alt="License: FSL-1.1-MIT">
    <img src="https://img.shields.io/badge/status-Phase%201-yellow" alt="Status: Phase 1">
  </p>
</p>

---

## What is keel?

keel is a **pure Rust CLI tool** that builds a fast, incrementally-updated structural graph of your codebase and enforces architectural contracts at generation time — not at review time, not at build time.

When an LLM coding agent modifies your code, keel immediately validates that the change doesn't break callers, violate type contracts, or introduce structural drift. Think of it as a structural linter purpose-built for the age of AI-generated code.

**Target:** TypeScript, Python, Go, and Rust codebases up to 500k LOC.

## Features

- **Structural graph** — maps every function, class, module, and their relationships using tree-sitter + per-language resolvers
- **Incremental validation** — `keel compile` re-checks only affected files in <200ms
- **Hash-based identity** — every symbol gets an 11-character `base62(xxhash64(...))` hash for fast lookup
- **3-tier resolution** — tree-sitter (universal) → per-language enhancer → LSP/SCIP (on-demand)
- **Error codes with fix hints** — every violation includes actionable remediation
- **Circuit breaker** — auto-downgrades repeated false positives to warnings
- **Batch mode** — defers non-critical checks during rapid agent iteration
- **MCP + HTTP server** — real-time enforcement via `keel serve`
- **Zero runtime dependencies** — single statically-linked binary

## Architecture

```
keel CLI
  ├── keel-core       Graph schema, GraphStore, SQLite storage
  ├── keel-parsers    tree-sitter + per-language LanguageResolver
  ├── keel-enforce    Compile validation, enforcement, circuit breaker
  ├── keel-cli        clap CLI, command routing
  ├── keel-server     MCP + HTTP server (keel serve)
  └── keel-output     JSON, LLM, human output formatters

extensions/
  └── vscode/         VS Code extension
```

### Resolution Engine (3-Tier Hybrid)

| Tier | Strategy | Coverage | Speed |
|------|----------|----------|-------|
| 1 | tree-sitter queries | 75-92% | <50ms / file |
| 2 | Per-language enhancer (Oxc, ty, heuristics, rust-analyzer) | 92-98% | <200ms / file |
| 3 | LSP/SCIP (on-demand) | >95% | seconds |

## Quick Start

```bash
# Install (from source)
cargo install --path crates/keel-cli

# Initialize in your repo
cd your-project
keel init

# Full structural map
keel map

# Validate changes
keel compile src/auth.ts

# Look up a symbol by hash
keel discover a7Bx3kM9f2Q

# Find where a hash lives
keel where a7Bx3kM9f2Q
```

## Commands

| Command | Purpose | Performance Target |
|---------|---------|-------------------|
| `keel init` | Initialize keel in a repo | <10s for 50k LOC |
| `keel map` | Full re-map of the codebase | <5s for 100k LOC |
| `keel compile [file...]` | Incremental validation | <200ms single file |
| `keel discover <hash>` | Adjacency lookup (callers, callees) | <50ms |
| `keel where <hash>` | Hash → file:line resolution | <50ms |
| `keel explain <code> <hash>` | Resolution chain explanation | <50ms |
| `keel serve` | MCP/HTTP/file-watch server | ~50-100MB memory |
| `keel deinit` | Clean removal of keel data | — |
| `keel stats` | Telemetry dashboard | — |

## Language Support

| Language | Tier 1 (tree-sitter) | Tier 2 (Enhancer) | Status |
|----------|---------------------|-------------------|--------|
| TypeScript/JavaScript | `tree-sitter-typescript` | Oxc (`oxc_resolver` + `oxc_semantic`) | In progress |
| Python | `tree-sitter-python` | ty (subprocess) | In progress |
| Go | `tree-sitter-go` | tree-sitter heuristics | In progress |
| Rust | `tree-sitter-rust` | rust-analyzer (lazy-load) | In progress |

## Configuration

keel stores its configuration in `.keel/config.toml`:

```toml
[keel]
version = "0.1.0"

[languages]
typescript = true
python = true
go = true
rust = true

[enforcement]
preexisting_severity = "warning"  # "error", "warning", "off"
type_hints = true
docstrings = true
placement = true

[batch]
auto_expire_seconds = 60

[circuit_breaker]
max_failures = 3

[output]
format = "json"  # "json", "llm", "human"
```

Additional ignore patterns go in `.keelignore` (gitignore syntax).

## Integration

### Claude Code / Cursor / AI Agents

keel is designed to be called by LLM coding agents after every code modification:

```bash
# Agent modifies src/auth.ts, then:
keel compile src/auth.ts

# Exit 0 + empty stdout = clean compile, carry on
# Exit 1 + JSON violations = fix the issues
```

The `--batch-start` / `--batch-end` flags let agents defer non-critical checks during rapid iteration.

### MCP Server

```bash
keel serve --mcp
# Exposes keel as an MCP tool server for Claude Code, Cursor, etc.
```

### VS Code Extension

The `extensions/vscode/` directory contains a VS Code extension that displays keel violations inline with diagnostics, code actions, and hash decorations.

## Development

### Building from Source

```bash
# Prerequisites: Rust 1.75+
cargo build --workspace

# Run tests
cargo test --workspace

# Run with optimizations
cargo build --release
```

### Project Structure

```
Cargo.toml              Workspace root
crates/
  keel-core/            Graph schema, hashing, SQLite store
  keel-parsers/         tree-sitter parsing, query patterns, file walker
  keel-enforce/         Compile validation, error codes, circuit breaker
  keel-cli/             CLI entry point, command routing
  keel-server/          MCP + HTTP server, file watcher
  keel-output/          JSON, LLM, and human formatters
extensions/
  vscode/               VS Code extension (TypeScript)
tests/
  fixtures/             Test fixture repos
  schemas/              JSON schema validation
scripts/
  test-fast.sh          Quick integration suite
  test-full.sh          Full oracle validation
```

### Error Codes

| Code | Category | Severity |
|------|----------|----------|
| E001 | Broken caller | ERROR |
| E002 | Missing type hints | ERROR |
| E003 | Missing docstring | ERROR |
| E004 | Function removed | ERROR |
| E005 | Arity mismatch | ERROR |
| W001 | Placement issue | WARNING |
| W002 | Duplicate name | WARNING |
| S001 | Suppressed | INFO |

### Exit Codes

- `0` — success, no violations (empty stdout)
- `1` — violations found
- `2` — keel internal error

## Phase Status

See [PROGRESS.md](PROGRESS.md) for detailed implementation status.

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 0 | Contracts, schemas, project scaffold | Complete |
| Phase 1 | Tree-sitter foundation + resolvers | **In progress** |
| Phase 2 | Enforcement engine + CLI commands | Pending |
| Phase 3 | Server, integrations, VS Code | Pending |
| Phase 4 | Polish, cross-platform, distribution | Pending |

**Current:** 24 tests passing (13 core + 11 parsers).

## Roadmap

- **Phase 1** — Tree-sitter parsing for all 4 languages, per-language Tier 2 resolvers, FileWalker, query patterns
- **Phase 2** — `keel compile`, `keel discover`, `keel where`, `keel explain`, error codes, circuit breaker, batch mode
- **Phase 3** — `keel serve` (MCP + HTTP), VS Code extension, tool integration configs (Claude Code, Cursor, Windsurf, etc.)
- **Phase 4** — Cross-platform binaries, `cargo install`, Homebrew, npm wrapper, performance benchmarks

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding standards, and PR guidelines.

## Security

See [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

## License

keel is licensed under the [Functional Source License, Version 1.1, MIT Future License](LICENSE) (FSL-1.1-MIT).

This means:
- **Free for non-competing use** — use keel in your projects, integrate it in your workflows
- **Source available** — read, modify, and contribute to the code
- **Converts to MIT** after 2 years — each release becomes fully open source 24 months after publication
- **No competing products** — you may not use keel's code to build a competing structural enforcement tool

---

<p align="center">
  <a href="https://keel.engineer">keel.engineer</a>
</p>
