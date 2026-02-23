# Changelog

All notable changes to keel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-02-22

### Added
- `keel login` — authenticate with keel cloud via Clerk OAuth device flow (browser-based)
- `keel logout` — remove stored credentials
- `keel push` — upload graph.db to keel cloud (full upload; incremental diffs planned)
- `keel context` — minimal structural context for safely editing a file
- Dual telemetry sending: anonymous aggregate + user-scoped when logged in
- Global credential storage at `~/.keel/credentials.json` with Unix permission hardening
- Agent identification via environment variable detection (`client_name` field)
- Real telemetry population from compile/map commands with error code breakdown
- MCP `context` tool for file-scoped structural context

### Changed
- `try_send_remote()` now dual-sends (anonymous + authenticated) when logged in
- Telemetry events include `error_codes` and `client_name` fields

### Dependencies
- Added `webbrowser = "1"` for browser-based OAuth flow

[0.3.0]: https://github.com/FryrAI/Keel/compare/v0.1.0...v0.3.0

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
- SQLite optimizations: WAL mode, 8MB cache, memory temp store, 256MB mmap
- `ModuleProfile` with `class_count` and `line_count` fields
- `ResolvedEdge.resolution_tier` tracking across all 4 language resolvers
- `get_node()` fallback to `previous_hashes` for renamed/updated functions
- Lazy resolver creation in compile CLI (only allocate resolver for target language)
- `keel upgrade` — self-update from GitHub releases (auto-detects Homebrew/cargo installs)
- `keel completion <shell>` — generate shell completions for bash, zsh, fish, elvish, powershell
- 762 tests passing, 0 ignored, 0 clippy warnings, 15 real-world repos validated

### Performance
- `keel compile` single file: <200ms
- `keel map` 100k LOC: <5s
- `keel discover` / `keel where`: <50ms
- Compile engine pre-fetches nodes once per file (was 3x redundant queries)

[0.1.0]: https://github.com/FryrAI/Keel/releases/tag/v0.1.0
