<p align="center">
  <h1 align="center">keel</h1>
  <p align="center">
    <strong>Structural code enforcement for LLM coding agents</strong>
  </p>
  <p align="center">
    <a href="https://github.com/FryrAI/Keel/actions/workflows/ci.yml"><img src="https://github.com/FryrAI/Keel/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <img src="https://img.shields.io/badge/rust-1.75%2B-orange" alt="Rust 1.75+">
    <img src="https://img.shields.io/badge/license-FSL--1.1--MIT-blue" alt="License: FSL-1.1-MIT">
    <img src="https://img.shields.io/badge/status-Phase%204-green" alt="Status: Phase 4">
    <a href="https://github.com/FryrAI/Keel"><img src="https://img.shields.io/github/stars/FryrAI/Keel?style=social" alt="GitHub Stars"></a>
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
- **Backpressure signals** — `PRESSURE=LOW/MED/HIGH` with `BUDGET=` directives for token-aware agents
- **Cloud sync** — `keel login` + `keel push` uploads graph to keel cloud for team dashboards and cross-repo linking
- **Fix generation** — `keel fix` produces diff-style fix plans for E001-E005 violations
- **Naming suggestions** — `keel name` scores modules by keyword overlap and detects naming conventions
- **MCP + HTTP server** — real-time enforcement via `keel serve`
- **Tool config generation** — `keel init` auto-detects 11 AI coding tools and generates hook configs
- **Zero runtime dependencies** — single statically-linked 12MB binary

## Performance

Validated against 15 real-world repos (43k nodes, 60k edges). Post-O(n) fix numbers:

| Repo | Language | Compile Time | Nodes | Cross-file Edges |
|------|----------|-------------|-------|-----------------|
| ripgrep | Rust | 3.0s (was 277s, **91x faster**) | 4670 | 581 |
| fastapi | Python | 15.1s (was 259s, **17x faster**) | 6617 | 474 |
| pydantic | Python | 7.0s (was 119s, **17x faster**) | 11633 | 1028 |
| fiber | Go | 2.4s (was 33s, **14x faster**) | 3657 | 5344 |
| axum | Rust | 4.1s (was 203s, **49x faster**) | 3621 | 57 |
| cobra | Go | 0.3s | 614 | 536 |

See [PROGRESS.md](PROGRESS.md) for full 15-repo benchmark table.

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

## Install

```bash
# macOS (Homebrew)
brew tap FryrAI/keel && brew install keel

# Linux / macOS (script)
curl -fsSL https://raw.githubusercontent.com/FryrAI/Keel/main/scripts/install.sh | bash

# From source
cargo install --path crates/keel-cli

# CI (GitHub Actions)
# uses: FryrAI/Keel/.github/actions/keel@v0.1.0
```

### Updating

```bash
# Self-update (direct installs)
keel upgrade

# Homebrew
brew upgrade keel

# Cargo
cargo install keel-cli
```

### Shell Completions

```bash
# Bash
keel completion bash > /etc/bash_completion.d/keel

# Zsh
keel completion zsh > ~/.zfunc/_keel

# Fish
keel completion fish > ~/.config/fish/completions/keel.fish
```

## Quick Start

```bash
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
| `keel map [--depth 0-3]` | Depth-aware structural map | <5s for 100k LOC |
| `keel compile [--depth 0-2] [file...]` | Validation with backpressure | <200ms single file |
| `keel discover <hash>` | Adjacency lookup (callers, callees) | <50ms |
| `keel where <hash>` | Hash → file:line resolution | <50ms |
| `keel explain <code> <hash>` | Resolution chain explanation | <50ms |
| `keel serve` | MCP/HTTP/file-watch server | ~50-100MB memory |
| `keel fix [hash...]` | Generate fix plans from violations | <200ms |
| `keel name <desc>` | Location-aware naming suggestions | <100ms |
| `keel login` | Authenticate with keel cloud | — |
| `keel logout` | Remove stored credentials | — |
| `keel push [--yes]` | Upload graph to keel cloud | — |
| `keel upgrade` | Self-update to latest version | — |
| `keel completion <shell>` | Generate shell completions | — |
| `keel deinit` | Clean removal of keel data | — |
| `keel stats` | Telemetry dashboard | — |

## Language Support

| Language | Tier 1 (tree-sitter) | Tier 2 (Enhancer) | Status |
|----------|---------------------|-------------------|--------|
| TypeScript/JavaScript | `tree-sitter-typescript` | Oxc (`oxc_resolver` + `oxc_semantic`) | Tier 1+2 Complete |
| Python | `tree-sitter-python` | ty (subprocess) | Tier 1+2 Complete |
| Go | `tree-sitter-go` | tree-sitter heuristics | Tier 1+2 Complete |
| Rust | `tree-sitter-rust` | rust-analyzer (lazy-load) | Tier 1+2 Complete |

## Configuration

keel stores its configuration in `.keel/keel.json`:

```json
{
  "version": "0.1.0",
  "languages": ["typescript", "python", "go", "rust"],
  "enforce": {
    "type_hints": true,
    "docstrings": true,
    "placement": true
  },
  "circuit_breaker": { "max_failures": 3 },
  "batch": { "timeout_seconds": 60 },
  "tier": "free",
  "telemetry": {
    "enabled": true,
    "detailed": false,
    "remote": true
  },
  "naming_conventions": {
    "style": null,
    "prefixes": []
  }
}
```

Use `keel config` to read/write values:

```bash
keel config                          # dump full config
keel config tier                     # get current tier
keel config tier team                # set tier
keel config telemetry.enabled false  # disable telemetry
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

### Backpressure & Fix Planning

```bash
# Depth-0 compile: summary only, minimal tokens
keel compile --depth 0 src/auth.ts
# Output: PRESSURE=LOW BUDGET=expand | PRESSURE=HIGH BUDGET=contract

# Generate fix plans for violations
keel fix a7Bx3kM9f2Q
# Output: diff-style fix plan with context lines

# Find the best module for a new function
keel name "validate user authentication"
# Output: scored modules with keyword overlap and convention hints

# Depth-aware map for context budgeting
keel map --depth 1
# Output: modules + direct children, hotspot detection
```

### MCP Server

```bash
keel serve --mcp
# Exposes keel as an MCP tool server for Claude Code, Cursor, etc.
```

### VS Code Extension

The `extensions/vscode/` directory contains a VS Code extension that displays keel violations inline with diagnostics, code actions, and hash decorations.

## Documentation

- [Getting Started](docs/getting-started.md) — install, init, map, compile in 5 minutes
- [Command Reference](docs/commands.md) — full command reference with examples
- [Agent Integration](docs/agent-integration.md) — wiring keel into Claude Code, Cursor, etc.
- [Configuration](docs/config.md) — keel.json reference, .keelignore
- [FAQ](docs/faq.md) — troubleshooting and common questions

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
| Phase 1 | Tree-sitter foundation + resolvers | Complete |
| Phase 2 | Enforcement engine + CLI commands | Complete |
| Phase 3 | Server, integrations, VS Code | Complete |
| Phase 4 | Polish, cross-platform, distribution | **Ready for release** |

**Current:** 1236 tests passing, 0 failures, 0 ignored, 0 clippy warnings. 15 real-world repos validated.

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
