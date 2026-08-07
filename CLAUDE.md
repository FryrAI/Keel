# Keel — Development Guide

> Read this before making changes to the keel codebase.

## Project Overview

keel is a **pure Rust CLI tool** for structural code enforcement across LLM coding agents. It provides a fast, incrementally-updated structural graph of codebases and enforces architectural contracts at generation time — not at review time, not at build time.

- **Language:** Pure Rust (see [Constitution Article 1](constitution.md))
- **Scope:** TypeScript, Python, Go, Rust
- **License:** FSL-1.1-MIT (Functional Source License)
- **Website:** keel.engineer

## Architecture

```
keel CLI
  ├── keel-core       # Graph schema, GraphStore, SQLite storage
  ├── keel-parsers    # tree-sitter + per-language LanguageResolver
  ├── keel-enforce    # Compile validation, enforcement, circuit breaker
  ├── keel-cli        # clap CLI, command routing
  ├── keel-server     # MCP + HTTP server (keel serve)
  └── keel-output     # JSON, LLM, human output formatters

extensions/
  └── vscode/         # VS Code extension
```

### Resolution Engine (3-Tier Hybrid)

```
Tier 1: tree-sitter (universal, ~75-92%)
    ↓ ambiguous references
Tier 2: per-language enhancer
    - TypeScript: Oxc (oxc_resolver + oxc_semantic)
    - Python: ty subprocess (ty --output-format json)
    - Go: tree-sitter heuristics (sufficient for Go)
    - Rust: rust-analyzer lazy-load (ra_ap_ide)
    ↓ still ambiguous
Tier 3: LSP/SCIP (on-demand, optional, >95%)
```

## Tech Stack

| Component | Crate | Notes |
|-----------|-------|-------|
| Parsing | `tree-sitter` + 4 grammars | Compiled in, not runtime loaded |
| TS/JS Resolution | `oxc_resolver` + `oxc_semantic` | MIT, 30x faster than webpack |
| Python Resolution | `ty` (subprocess) | `ty --output-format json`. NOT a library. |
| Hashing | `xxhash-rust` | base62(xxhash64(...)), 11 chars |
| Database | `rusqlite` (bundled) | SQLite for graph storage, statically linked |
| CLI | `clap` | Argument parsing |
| Serialization | `serde` + `serde_json` | JSON output |

**Constraints:**
- No FFI in hot path
- `ty` is subprocess only (not library)
- `rust-analyzer` is lazy-loaded (60s+ startup)
- Single binary, zero runtime dependencies
- Cross-platform: Linux, macOS, Windows

## Key Commands

| Command | Purpose | Performance Target |
|---------|---------|-------------------|
| `keel init` | Initialize keel in a repo | <10s for 50k LOC |
| `keel map` | Full re-map | <5s for 100k LOC |
| `keel compile [file...]` | Incremental validate | <200ms single file |
| `keel discover <hash>` | Adjacency lookup | <50ms |
| `keel where <hash>` | Hash to file:line | <50ms |
| `keel explain <code> <hash>` | Resolution chain | <50ms |
| `keel skeleton <file>` | Signature-only view of a file | <100ms |
| `keel focus <hash\|file>` | Minimal context set for an edit | <50ms |
| `keel checkpoint` | Session-state summary (git + graph) | <1s |
| `keel validate-plan <file\|->` | Pre-execution plan risk check + P001/P002 | <100ms |
| `keel review --base <ref>` | Two-sided graph diff vs a base ref | <3s for a 54-file diff |
| `keel quality [--snapshot\|--trend]` | Countable maintainability metrics + their trend | <100ms |
| `keel serve` | MCP/HTTP/watch server | ~50-100MB memory |
| `keel login` | Authenticate with keel cloud | — |
| `keel logout` | Remove stored credentials | — |
| `keel push` | Upload graph to keel cloud | — |
| `keel upgrade` | Self-update from GitHub releases | — |
| `keel completion <shell>` | Generate shell completions | — |
| `keel deinit` | Clean removal | N/A |
| `keel stats` | Telemetry dashboard | N/A |

## Exit Codes

- `0` — success, no violations
- `1` — violations found
- `2` — keel internal error

**Clean compile:** Zero errors + zero warnings = exit 0, **empty stdout**. No info block unless `--verbose`.

## Error Codes

| Code | Category | Severity |
|------|----------|----------|
| E001 | broken_caller | ERROR |
| E002 | missing_type_hints | ERROR |
| E003 | missing_docstring | ERROR |
| E004 | function_removed | ERROR |
| E005 | arity_mismatch | ERROR |
| E006 | layer_violation | ERROR (opt-in) |
| W001 | placement | WARNING |
| W002 | duplicate_name | WARNING |
| W005 | dead_code | WARNING |
| W006 | duplicate_implementation | WARNING |
| W007 | oversized_file | WARNING |
| W009 | new_cross_boundary_dep | WARNING |
| S001 | suppressed | INFO |
| P001 | unknown_symbol | WARNING (plan-time only) |
| P002 | signature_mismatch | WARNING (plan-time only) |

W005-W007 are the v0.5 "economy" additions — additive only; existing codes/severities/exit codes unchanged.

W009 fires when a file starts depending on a package it did not depend on before. It is self-baselining
(everything in the graph is grandfathered), needs no config, and is silent in repos that declare no
packages. E006 is its escalation for ordered pairs listed in `architecture.deny` in keel.json — strictly
opt-in, and the only new ERROR.

P001/P002 are the plan-time namespace, emitted only by `keel validate-plan` — never by `keel compile`, never
in the compile exit code. `keel validate-plan` still exits 0 by default; `--strict` is the opt-in gate.

Every ERROR has `fix_hint`. Every violation has `confidence` (0.0-1.0) and `resolution_tier`. The plan-time
findings are the one exception to the second half: P001/P002 carry `confidence` and `fix_hint` but no
`resolution_tier`, because no resolver runs at plan time — there is no code yet to resolve.

## Build

```bash
cargo build --workspace                                    # Full build
cargo clippy --workspace --all-targets -- -D warnings      # Lint (must pass clean)
cargo fmt --check                                          # Format check
```

## Testing

```bash
cargo test                    # All unit tests
./scripts/test-fast.sh        # Quick integration suite
./scripts/test-full.sh        # All 4 oracles, all repos
```

## Common Gotchas

### Compile Sync Never Deletes on Failed Re-Resolution
`sync_compiled_files` (keel-cli `compile_sync.rs`) deletes a stored `calls`/`uses` edge only on positive
evidence: this pass re-added the same `(source_id, target_id, kind)`, or the target lives inside the
compiled file. A cross-file reference that fails to re-resolve at compile time proves nothing — compile-time
tier-2 resolvers have parsed only the compiled files — so its edge is KEPT, and a deleted cross-file call
site keeps its edge until the next `keel map` (bounded staleness, accepted in PR #56). A zero-definition
parse touches nothing. Do not "fix" the staleness by re-adding a wholesale prune: that was the edge-erosion
bug where one compile permanently deleted a file's cross-crate edges, starving W005/E004/E001/`keel
discover` and W009's stored-boundary baseline.

### Hash Computation
Hash = `base62(xxhash64(canonical_signature + body_normalized + docstring))`. Uses AST-based normalization, not raw text. Docstring is part of hash input.

### W006 Has Two Tiers, and an Empty Fingerprint Is Not a Match
Type-1 is `hash::normalize_body` (whitespace only, confidence 0.85). Type-2 is `hash_t2` — a lexical
tokenizer that renames non-keyword identifiers to `v0, v1, …` and collapses literals to
`<int>`/`<str>`/`<bool>` (confidence 0.6, fires only when Type-1 found nothing). It reuses
`hash::strip_comments`, so the two normalizers can never disagree about where a comment or string ends.
`body_index.t2_hash` is written **only by `keel map`** (compile reads it), stores `''` for anything under
`hash_t2::MIN_T2_NORMALIZED_LEN` **and** for every row written before the column existed — so `''` must
never match `''`; `body_index_find_t2` refuses an empty argument. The floor (120) was calibrated on keel's
own tree: at 80, `match self { .. => <str> }` string tables and `&[<str>, <str>]` constant lists start
colliding. Raise it before lowering it.

### Clean Compile Output
When compile passes with zero errors AND zero warnings: **empty stdout, exit 0**. This is critical — the LLM should never see output unless something needs attention.

### Quality Snapshots Survive `keel map`
`clear_all()` in `crates/keel-core/src/sqlite.rs` enumerates its tables explicitly and **`quality_snapshots` is deliberately not in the list** — that omission is the entire mechanism giving keel a memory across re-maps. Adding it would silently turn `keel quality --trend` into a report on the current map.

`quality_snapshots` (schema v7) stores a versioned JSON blob keyed on `commit_sha` (UNIQUE — a second capture at the same commit *updates* in place). Bump `keel_enforce::quality::METRICS_VERSION` whenever a metric's *definition* changes: `--trend` then refuses to compare across versions rather than silently re-baselining. `dead_private_fns` is computed from the graph alone, so it over-counts relative to W005 (no decorator/trait-context/`keel:keep` awareness) — it is a trend line, not a violation count.

A metric *added* without a version bump (its field carries `#[serde(default)]`) must also be named in
`quality::trend::METRICS_ADDED_AFTER_V1`. Otherwise a blob written before the field existed deserializes as
`0.0` and `--trend` draws a step from "not measured" to a real reading — the same silent re-baselining
`refused` exists to prevent. Named there, `build_trend` records the gap per point and omits the metric
(reported in `QualityTrend::omitted`, rendered as `—` in the per-commit table) instead of trending it.
Append to that list, never remove from it.

The one exception is **honesty about files keel cannot parse**: compiling a `.sql`, `.baml`, `.proto` or `.graphql` file (named explicitly or matched by `--changed`) prints one stderr line — `keel: .sql is not a tracked language — no checks ran`. Exit stays 0, stdout stays empty. Never fires for `.md`/`.json`/`.lock`. Owned by `keel_enforce::file_class`.

### What the Audit Grades
`keel audit` grades size and naming smells (`function_size`, `god_file`, `public_ratio`, `cryptic_name`) on **hand-written source only**. `FileClass` (keel-enforce `file_class.rs`) classifies every path as Source / Boundary (`.baml`, `.proto`, `.graphql`) / Generated (`baml_client`, `baml_sdk`) / Data (`.sql`) / Test (`violations_util::is_test_file`), deriving Source from the canonical `detect_language` table so the two can't drift. Only Source is graded; `orphan_file` deliberately still applies to `.baml` (T1.4 gives those files real edges — real edges beat exemptions).

`circular_dep` is **disabled for Rust**: intra-crate module cycles are legal, idiomatic Rust and cargo already forbids the illegal inter-crate ones. It still applies to TS/Python/Go, capped at cycles of ≤8 modules. `--strict-cycles` restores the old behavior.

Findings are deduplicated on rule+file+symbol before scoring, then ranked by (severity, agent-config over per-file smells, dimension-score impact, count). `--llm` prints the top 20 and states how many it omitted; `--top 0` lifts the cap.

### Dynamic Dispatch
Low-confidence call edges (trait dispatch, interface methods) produce **WARNING not ERROR**. Prevents false positives on ambiguous resolution.

### Batch Mode
`--batch-start` defers type hints, docstrings, placement. Structural errors (broken callers, removed functions, arity) still fire immediately. `--batch-end` fires all deferred. Auto-expires after 60s inactivity.

### Circuit Breaker
3 consecutive failures on same error-code + hash pair: attempt 1 = fix_hint, attempt 2 = wider discover, attempt 3 = auto-downgrade to WARNING. Counter resets on success or different error.

### Type Hint Enforcement Per Language
- TypeScript, Go, Rust: already typed. Validate signatures against callers.
- Python: requires explicit type annotations. `def f(x)` = ERROR. `def f(x: int) -> str` = passes.
- JavaScript: requires JSDoc `@param` and `@returns`.

### Progressive Adoption
- New/modified code: ERROR
- Pre-existing code: WARNING (configurable escalation)

## Related Documents

- [Design Principles](design-principles.md) — the "why" document
- [Constitution](constitution.md) — non-negotiable articles

<!-- keel:start -->
<!-- keel:version 0.5.0 -->
## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.

### Session continuity (before compaction)
Context windows compact and reset. Before you run low on context, capture a compact,
re-injectable summary of the session's structural changes:

```
keel checkpoint --llm --since <session-base-commit> -o .keel/checkpoint.md
```

It records changed/added/removed symbols, callers now at risk, outstanding violations,
and recent commit subjects — all derived from git + the graph, no stored state. After a
reset, re-read `.keel/checkpoint.md` to recover where you were. (Keeping the file in a
project vault/notes directory is optional and entirely user-side.)

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it. The hash is shown in the keel map (injected at session start or embedded below).
- For **body-only changes** (bug fixes, refactoring internals, improving logging), skip discover — compile will catch any issues.
- If the function has upstream callers (↑ > 0), you MUST understand them before changing its interface.

### After every edit:
- `keel compile` runs automatically via hooks after every Edit/Write/MultiEdit.
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions.
- Docstrings are mandatory on all public functions.
- If a warning has `confidence` < 0.7, attempt one fix. If it doesn't resolve, move on.

### Error codes:
| Code | Meaning |
|------|---------|
| E001 | broken_caller — a caller references a changed/removed function |
| E002 | missing_type_hints — function parameters or return type lack annotations |
| E003 | missing_docstring — public function lacks documentation |
| E004 | function_removed — a function was deleted but callers remain |
| E005 | arity_mismatch — caller passes wrong number of arguments |
| E006 | layer_violation — dependency denied by `architecture.deny` in keel.json (opt-in) |
| W001 | placement — function is in a non-ideal module |
| W002 | duplicate_name — another function with the same name exists |
| W005 | dead_code — private function has no callers in the graph |
| W006 | duplicate_implementation — function body is identical to one elsewhere, or identical apart from renamed identifiers/literals (lower confidence) |
| W007 | oversized_file — file exceeds the configured line budget and grew |
| W009 | new_cross_boundary_dep — this file now depends on a package it did not before |
| S001 | suppressed — violation suppressed via `--suppress` or circuit breaker |
| P001 | unknown_symbol — plan-time only: the plan calls a symbol the graph does not have |
| P002 | signature_mismatch — plan-time only: the plan's call does not match the stored signature |

### If compile keeps failing (circuit breaker):
1. **First failure:** Fix using the `fix_hint` provided
2. **Second failure (same error):** Run `keel discover <hash> --depth 2` — the issue may be upstream
3. **Third failure (same error):** keel auto-downgrades to WARNING. Run `keel explain <error-code> <hash>` to inspect the resolution chain.

### Before creating a new function:
1. Check the keel map to see if a similar function already exists
2. Place the function in the module where it logically belongs
3. If keel warns about placement, move the function to the suggested module

### When scaffolding (creating multiple new files at once):
1. Run `keel compile --batch-start` before creating files
2. Create files normally — structural errors still fire immediately
3. Run `keel compile --batch-end` when scaffolding is complete

### Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel discover <file-path>` — list all symbols in a file with hashes
- `keel discover --name <function-name>` — find a function by name
- `keel search <term>` — search the graph by name (substring match)
- `keel compile <file>` — validate changes
- `keel compile --changed` — validate only git-changed files
- `keel compile --since <commit>` — validate files changed since a commit
- `keel compile --batch-start` / `--batch-end` — batch mode for scaffolding
- `keel explain <error-code> <hash>` — inspect resolution reasoning
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — regenerate the LLM-optimized map (includes function names)
- `keel map --semantic` — per-file summaries, public API, and when-to-use guidance
- `keel watch` — auto-compile on file changes
- `keel check <hash>` — pre-edit risk assessment (callers, risk level)
- `keel fix [--apply]` — generate and optionally apply fix plans
- `keel name <description>` — suggest names for new code
- `keel analyze <file>` — architectural analysis of a file
- `keel audit [--dimension <name>] [--top N] [--strict-cycles]` — AI-readiness scorecard (structure, discoverability, navigation, config); ranked worst-first, top 20 by default, `--top 0` for all
- `keel context <file>` — minimal structural context for safely editing a file
- `keel skeleton <file>` — compressed signature-only view (`--docs`, `--private`, `--budget <tokens>`)
- `keel focus <hash|file>` — minimal context set to safely modify a target (`--depth N`, `--budget <tokens>`)
- `keel checkpoint [--since <commit>] [--staged] [-o <file>]` — compact session-state summary (changed symbols, affected callers, violations, recent commits) for re-injection after context loss
- `keel validate-plan <file|-> [--strict]` — validate a plan against the graph before execution (callers at risk, risk level, callers-first order) plus P001/P002 plan findings; always exits 0 unless `--strict` is passed
- `keel review --base <ref>` — two-sided graph diff vs a base ref: which contracts moved, which callers were left outside the diff, and which violations the diff *introduced* (`--format github` for CI annotations, `--gate` to fail on the codes listed in `review.gate`)
- `keel quality [--trend]` — countable maintainability metrics from the stored graph (`files_over_budget`, `cycle_count`, `dead_private_fns`, `cross_module_edge_ratio`, `high_cc_mass_share`, `propagation_cost`); `--snapshot` records one point per commit, `--trend [--since <sha>|--last N]` reports the direction. Never gates (always exits 0)

**Tip:** When running keel commands manually, always use the `--llm` flag for token-efficient output.

### MCP Tools (available via `keel serve --mcp`):
The keel MCP server exposes these tools directly to your IDE:
- `keel/compile` — compile files and check for violations
- `keel/discover` — find callers and callees of a function by hash
- `keel/where` — resolve a hash to file:line
- `keel/explain` — explain a violation with resolution chain
- `keel/map` — full or scoped graph map
- `keel/check` — pre-edit risk assessment
- `keel/fix` — generate fix plans for violations
- `keel/search` — search the graph by name
- `keel/name` — suggest names for new code
- `keel/analyze` — architectural analysis of a file
- `keel/audit` — AI-readiness scorecard
- `keel/context` — minimal structural context for a file
- `keel/skeleton` — compressed signature-only view of a file
- `keel/focus` — minimal context set to safely modify a target
- `keel/checkpoint` — compact session-state summary for re-injection after context loss
- `keel/validate-plan` — validate a plan against the graph before execution (`strict: true` adds a `strict_failed` boolean)
- `keel/review` — two-sided graph diff vs a base ref (contracts moved, callers left behind, violations the diff introduced)

### Common Mistakes:
- **Don't guess hashes.** Use `keel discover path/to/file.py` to see all symbols and their hashes first.
- **Don't pass file paths as hashes.** If discover says "hash not found", check if you passed a file path — use path mode instead.
- **Recommended workflow:** `keel discover path/to/file.py` → see all symbols → `keel discover <hash> --depth 2` for deep exploration.
- **Use `keel search`** to find functions by name across the entire graph.
- **Use `--changed` in CI** to only check modified files: `keel compile --changed`. Add `--format github` there to get inline PR annotations straight from the binary.
<!-- keel:end -->
