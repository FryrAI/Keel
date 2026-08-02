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
- **`keel audit` is now a list a human will read to the end (T1.5).** Five
  changes, all noise removal:
  1. Test files are exempt from `function_size`, `god_file`, `public_ratio`
     and `cryptic_name` (via the shared `violations_util::is_test_file`) — a
     253-line integration test is not a maintainability defect.
  2. Findings are deduplicated on rule+file+symbol **before** scoring, so a
     file visited twice by a dimension's walk no longer double-reports (and no
     longer double-depresses its dimension score).
  3. `circular_dep` is disabled for Rust: intra-crate module cycles are legal,
     idiomatic Rust and cargo already forbids the illegal inter-crate ones, so
     the check could only ever report the legal kind. It still applies to
     TS/Python/Go, now capped at cycles of 8 modules or fewer. The new
     `keel audit --strict-cycles` restores the old behavior.
  4. A new `FileClass` (`Source | Boundary | Generated | Data | Test`), derived
     from the canonical `detect_language` table rather than a parallel
     extension list, exempts `.baml`/`.proto`/`.graphql` (Boundary),
     `baml_client`/`baml_sdk` (Generated) and `.sql` (Data) from those same
     four checks. `orphan_file` deliberately still applies to `.baml` — T1.4
     gives those files real edges, and real edges beat exemptions.
  5. Output is ranked by (severity, agent-config over per-file smells,
     dimension-score impact, count) instead of file-scan order. `--llm` prints
     the top 20 and states how many findings it omitted; `--top 0` lifts the
     cap. Measured on this repo: the top-of-list finding is now the one whose
     dimension score actually moves.
- **`keel compile` no longer reports a silent all-clear for files it cannot
  parse (T1.5).** Compiling a `.sql`, `.baml`, `.proto` or `.graphql` file —
  named explicitly or matched by `--changed` — prints one stderr line,
  `keel: .sql is not a tracked language — no checks ran`. Exit stays 0 and
  stdout stays empty (the clean-compile contract is intact), but a hook that
  reads exit 0 as verification is no longer told an unchecked migration
  passed. Never fires for `.md`, `.json`, `.lock` or any other extension.
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
- **Version-stamped, drift-detected agent docs (T1.6).** Every generated
  `<!-- keel:start -->`/`<!-- keel:end -->` block (`AGENTS.md`, `CLAUDE.md`,
  `GEMINI.md`, Copilot, Aider, Letta) now opens with a
  `<!-- keel:version X.Y.Z -->` stamp matching the binary that generated it,
  and the shared `Error codes` table (previously `AGENTS.md`-only) covers
  W005-W007 in every one of them, alongside the full v0.5 command set
  (`skeleton`/`focus`/`checkpoint`/`validate-plan`/`--semantic`) and all
  currently-registered MCP tools. `keel map`/`keel compile` detect a stale
  stamp or a `.keel/keel.json`-vs-binary version mismatch and print exactly
  one stderr line naming the fix; they never rewrite the file themselves.
  `keel init --update-docs` is the human-authorized fix: it rewrites the
  keel-managed block in every doc file already present (never creates a new
  integration), regenerates `.keel/hooks/post-edit.sh` — now passing
  `--client` explicitly (env-var client detection does not reliably survive
  into the hook's subprocess) and using the T1.1 5-second timeout instead of
  the old 15-second one — and syncs `.keel/keel.json`'s pinned version.
  `keel upgrade` syncs that same pinned version automatically after a
  successful binary swap.
- **`W009 new_cross_boundary_dep` — architectural erosion caught at the edit
  (T1.7).** A file that starts calling into a package it did not depend on
  before now produces one WARNING (confidence 0.9) whose `fix_hint` names that
  package's most-called public symbol as the façade to go through instead. A
  cross-package edge is a design decision that today gets zero review because
  it looks like one added `use` line in a diff of 400, and it is cheapest to
  reverse the moment it is made.
  Self-baselining and zero-config: every boundary the file's module already
  reaches in the graph is grandfathered, so only new erosion fires, and once
  the compile syncs the new edge the dependency stops being new — there is no
  baseline file. One warning per boundary, not per call site. Boundaries come
  from the monorepo package a node belongs to, falling back to the first path
  segment for unpackaged files (`frontend/` vs `crates/`); a repo that declares
  no packages stays completely silent, because a guessed boundary produces
  confident wrong warnings. Nothing fires before the first `keel map`, or in a
  directory whose stored nodes have no call edges yet — that guard is per
  *module*, not per file, so a brand-new file in a mapped module does fire.
  The detector is deliberately kept no wider than the graph it is diffed
  against, since anything it can see but `keel map` cannot resolve would be
  reported on an unchanged tree forever: only `calls` count (type-only
  references are a dependency on an abstraction; enable
  `architecture.count_type_deps` to include them), only names the file
  imported explicitly count, and the name must resolve to exactly one
  boundary's public, non-associated function — method dispatch (`from`,
  `collect`, `get_edges`) never does. Validated against keel's own 6-crate
  workspace: 113 files compile with zero W009 on an unchanged tree, while a
  synthetic `keel-output` → `keel-core` call fires exactly one with
  `compute_hash` named as the façade. Measured cost: ~0.8 ms per file in an
  unoptimized build (0.23 ms release) against a 2 ms budget.
- **`E006 layer_violation` (opt-in).** `"architecture": {"deny": [["harness",
  "core"]]}` in `.keel/keel.json` escalates a denied ordered pair from W009 to
  an ERROR that gates exit 1. Empty by default — keel never decides on its own
  which layers a repo has.

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
