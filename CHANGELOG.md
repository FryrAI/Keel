# Changelog

All notable changes to keel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **A multi-name import no longer collapses to its first name (T1.3).** One
  `import` statement produces one tree-sitter match per binding, and the
  dedup that merged them kept only the first: `import { a, b, c } from './m'`
  was recorded as `["a"]`, and `from mod import a, b, c` the same way. Nothing
  then knew `b` and `c` were in scope, so every cross-file reference to them
  went unresolved and their definitions read as uncalled. All bindings of a
  statement are now unioned (Go's `_`/`.` markers still replace rather than
  join), and `import * as ns` records its alias. This affects every language:
  expect more `calls` edges and *fewer* zero-caller functions after a re-map.
- **Svelte markup now resolves imported bindings (T1.3).** The template scan
  was seeded only with the component's own `<script>` definitions, so a helper
  imported from another module and used exclusively in markup —
  `{@const pct = completenessPct(v)}`, `<Panel onDone={refresh} />`, `{#each}`
  / `{#await}` / `{#snippet}` bodies — was invisible. It now emits a `uses`
  edge at confidence 0.70 under resolution tier `tier1_template`. Never a
  `calls` edge: a lexical match in unparsed markup carries no argument list, so
  it must not reach E001/E004/E005 or a fix plan. Measured on a 118-module
  SvelteKit app: the nine zero-caller exports of one `model.ts` all report
  callers ≥ 1, `uses` edges go 56 → 277, `calls` 771 → 830, `imports` edges are
  unchanged at 314, and `keel audit --dimension navigation` reports the same
  seven `high_coupling` and zero `bottleneck_module` findings as before.
  Component tags (`<FristenPanel />`) are still not edges, and imported
  constants and Svelte stores stay invisible by design — keel's graph holds
  functions and classes, so there is no node for a constant to point at.
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
- **`callers` / `callees` now count `uses` edges, not only `calls` (T1.3).**
  `keel search`, `keel discover` and `keel focus` answer "what depends on
  this?", and a function reached only through a callback, a handler table or a
  Svelte template expression is depended upon — reporting it at zero callers is
  the same false-dead-code signal W005 already refuses to send. Severity is
  untouched: E001/E004/E005 and the fix planner still filter `calls` edges
  themselves, because only a parsed call site carries an argument list.
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
- **A `.baml` function dispatched by string literal now has real callers
  (T1.4).** Code that drives a boundary function through a name string —
  `run_baml("PlanBerichtSection", input)`, a `match` arm on the same key, a
  handler-table entry — produced no edge at all, so every `baml_src/*.baml`
  function read as dead code with zero callers and zero callees. keel now
  captures string literals in exactly three positions (call argument,
  `match`/`switch`-case pattern, object/map key) for Rust, TypeScript and
  Python, keeps **only** those whose text exactly equals a name already in the
  boundary index, and emits a `uses` edge at the boundary provider's own
  confidence (0.75) under resolution tier `tier1_boundary_literal`. Never a
  `calls` edge: a string carries no argument list, so it must not reach
  E001/E004/E005 or a fix plan. `keel discover` on the dispatching function
  now lists the `.baml` node as a callee.
  Zero configuration, zero new error codes: a literal that matches nothing
  known is dropped inside the parser and never becomes a reference, so no
  free-text nodes or low-confidence guesses enter the graph, and a repo with
  no boundary surface produces no literal references at all. Go is not
  covered (no unambiguous cheap capture position).
  Cost, measured in-process against keel's own sources (identical trees, only
  the query source differing): +0.02–0.6 ms per file of query compilation and
  under 0.4 ms of matching for a 20-file batch, against a 10 ms budget — and
  ~0.04 ms/file even on a synthetic worst case of 240 captured literals per
  file. `keel compile` re-resolves these edges with the same ladder, so its
  prune-and-re-resolve does not delete them between maps.
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
