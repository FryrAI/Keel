# Changelog

All notable changes to keel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Rust macro invocations no longer resolve to same-named functions (T1.2).**
  `format!`, `vec!`, `write!`, `println!` and the rest of the Rust prelude are
  now recognised as external before any lookup happens, so a repo function
  named `format` stops collecting a `calls` edge from every file that formats
  a string. On one 25k-edge repo that single collision made a small currency
  formatter the graph's #1 hotspot at 1,385 phantom callers — edges that fed
  E001/E004/E005 and masked genuinely dead helpers. Macro definitions now
  carry an in-memory `is_macro` flag, so `name!(...)` resolves only to a
  `macro_rules! name`, never to a `fn name`. The same gate applies to
  attribute-macro paths.
- **Name-only cross-file matches spread across more than two files now emit no
  edge at all.** Picking the first cached hit at 0.50 confidence turned an
  ambiguity into a silent wrong answer; an honest absence is strictly better,
  and it stops the next instance of this bug class.
- **Total edge counts drop after these fixes — run `keel map` to re-map.** The
  removed edges were false, not data: they are phantom callers this release
  stops inventing. Measured on keel's own repo, `calls` edges go 4,097 →
  4,071 (total 10,129 → 10,103); every removed edge is a prelude-macro
  collision — `write!`/`writeln!` invocations that had been landing on a test
  helper named `write` (41 → 18 callers, the 18 real ones kept), and `json!`
  invocations landing on the `json` module. Repos with more such name
  collisions will see a proportionally larger drop. No schema change; the
  graph re-populates on the next map.

### Changed
- **The compile hot path no longer waits on the network (T1.1).** `compile`,
  `where`, `discover`, `focus`, `skeleton`, `explain`, `check`, `search`, and
  `context` are hard-coded as `hot_path_commands()` and never attempt a
  remote telemetry send — no config can put the network back on this path.
  `map`, `audit`, `stats`, and `push` are unaffected: they still send (and
  join) remote telemetry, opportunistically draining the same local
  `telemetry.db` these hot-path commands wrote to. Measured previously:
  network alone accounted for the entire gap between a reported
  `avg_compile_ms` and keel's <200ms constitutional target.
- `telemetry.remote` now defaults to `false`. Local writes to `telemetry.db`
  (`telemetry.enabled`) are unaffected — only remote reporting flips to
  opt-in. Enable it with `keel config telemetry.remote true`.
- A bare `keel compile` (no files, no `--changed`, no `--since`) now scopes
  to the git working-tree diff by default, matching the CLI help text's
  existing "empty = all changed" contract instead of silently walking and
  re-parsing every file in the repo (previously ~15s on a mid-size repo vs
  <100ms for a single scoped file — the "adjacent pathology" found during
  the T1.1 audit). Repos with no `.git` directory keep the old full-repo-scan
  default.
- `keel stats --llm` now reports `compile_p50_ms`/`compile_p95_ms` alongside
  the existing `avg_compile_ms` — the standing regression guard for T1.1.
  These also appear in `keel stats --json`'s `telemetry` block and in the
  human-readable output.

### Added
- `KEEL_NO_NETWORK=1` — a single escape hatch that disables remote telemetry
  for every command, without editing `keel.json`.

## [0.4.3] - 2026-07-21

First release since v0.4.2 picking up 75 commits of main (#48): Svelte/SvelteKit
parsing and resolution, test-context enforcement exemptions (#38/#39), parser
attribution fixes for same-named members (`de05056`), MCP protocol compliance,
economy enforcement (W005–W007), and the deep-clean backlog (#19–#46).

### Fixed
- Hash collisions at persist time are non-fatal (#48): a colliding node is
  re-salted with a file-path ordinal instead of aborting the whole compile
  with `failed to persist node updates`.

### Changed
- `keel --version` now embeds the git short SHA (e.g. `0.4.3 (abc123def)`), so
  an unreleased dev build can no longer masquerade as a released version (#48).

[0.4.3]: https://github.com/FryrAI/Keel/compare/v0.4.2...v0.4.3

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
