# Changelog

All notable changes to keel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-16

### Added
- Core structural graph engine with tree-sitter parsing for TypeScript, Python, Go, and Rust
- 3-tier resolution: tree-sitter (universal) → per-language enhancer (Oxc, ty, heuristics, rust-analyzer) → LSP/SCIP (on-demand)
- `keel init` — initialize keel in a repo with auto-detection of languages and AI coding tools
- `keel map` — full structural map with depth-aware output (`--depth 0-3`)
- `keel compile` — incremental validation with backpressure signals and depth control
- `keel discover` — adjacency lookup (callers, callees, module context)
- `keel where` — hash-to-file:line resolution
- `keel explain` — resolution chain explanation with depth truncation and `--max-tokens`
- `keel fix` — diff-style fix plan generation with `--apply` for auto-repair
- `keel name` — location-aware naming suggestions with keyword overlap scoring
- `keel serve` — MCP + HTTP server with file watching
- `keel stats` — telemetry dashboard
- `keel deinit` — clean removal
- Tool integration configs for Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code, Aider, Copilot, Antigravity, GitHub Actions
- VS Code extension with compile-on-save, CodeLens, hover, diagnostics, server lifecycle
- Error codes E001-E005 (errors) and W001-W002 (warnings) with fix hints
- Circuit breaker: auto-downgrade after 3 consecutive failures
- Batch mode: `--batch-start` / `--batch-end` for rapid agent iteration
- O(n) compile performance (indexed SQL queries)
- 931 tests passing across 15 real-world repos

### Performance
- `keel compile` single file: <200ms
- `keel map` 100k LOC: <5s
- `keel discover` / `keel where`: <50ms

[0.1.0]: https://github.com/FryrAI/Keel/releases/tag/v0.1.0
