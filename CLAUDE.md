# Keel — Agent Implementation Guide for Claude Code Sessions

```yaml
tags: [keel, implementation, claude-code, guide]
status: ready
purpose: "Read this FIRST before any implementation work"
```

> This file configures Claude Code for implementing the keel structural enforcement tool. Read this before touching any code.

## Project Overview

keel is a **pure Rust CLI tool** for structural code enforcement across LLM coding agents. It provides a fast, incrementally-updated structural graph of codebases and enforces architectural contracts at generation time — not at review time, not at build time.

- **Language:** Pure Rust (see [Constitution Article 1](constitution.md))
- **Scope:** TypeScript, Python, Go, Rust (Phase 1)
- **License:** FSL (Functional Source License)
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
  └── vscode/         # VS Code extension (~500 lines TypeScript)
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

## Tech Stack (Non-Negotiable)

| Component | Crate | Notes |
|-----------|-------|-------|
| Parsing | `tree-sitter` + 4 grammars | Compiled in, not runtime loaded |
| TS/JS Resolution | `oxc_resolver` + `oxc_semantic` | v0.111+, MIT, 30x faster than webpack |
| Python Resolution | `ty` (subprocess) | `ty --output-format json`. NOT a library. |
| Graph | `petgraph` | Function/class/module graph |
| Hashing | `xxhash-rust` | base62(xxhash64(...)), 11 chars |
| Database | `rusqlite` (bundled) | SQLite statically linked |
| CLI | `clap` | Argument parsing |
| Serialization | `serde` + `serde_json` | JSON output |
| Parallelism | `rayon` | Parallel file parsing |

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
| `keel serve` | MCP/HTTP/watch server | ~50-100MB memory |
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
| W001 | placement | WARNING |
| W002 | duplicate_name | WARNING |
| S001 | suppressed | INFO |

Every ERROR has `fix_hint`. Every violation has `confidence` (0.0-1.0) and `resolution_tier`.

## Agent Teams Setup

Keel uses Claude Code's native agent teams for parallel development. Enable before launching the swarm:

```json
// Claude Code settings.json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  },
  "teammateMode": "tmux"
}
```

**Key concepts:**
- 3 teams (Foundation, Enforcement, Surface), each in a separate git worktree
- Team leads run in **delegate mode** — coordinate only, don't edit code
- Teammates are spawned with detailed prompts referencing their spec files — they load this CLAUDE.md automatically
- Orchestrator is a standalone session (not part of any team) using `/tmux-observe` and `/ralph-loop`
- All sessions launch with `--sandbox --dangerously-skip-permissions` — sandbox (bubblewrap on Linux) restricts writes to CWD (worktree directory) and network to whitelisted domains. See [Sandbox Hardening](agent-swarm/infrastructure.md) for full config.
- See [Agent Swarm Playbook](agent-swarm/README.md) for full architecture and spawn prompts
- **CRITICAL:** Read [scope-limits.md](agent-swarm/scope-limits.md) before spawning any agents — hard limits on files, tool calls, and context

**Skills used:**
- `/ralph-loop` — autonomous test-fix-test cycles for every agent
- `/tmux-observe` — orchestrator monitors all 3 team panes

## Testing

**Run after EVERY change:**
```bash
cargo test                    # All unit tests
./scripts/test-fast.sh        # Quick integration suite
```

**Run for full validation:**
```bash
./scripts/test-full.sh        # All 4 oracles, all repos
```

**Test pattern:** Tests are pre-written with `#[ignore]`. Un-ignore as you implement features. Progress = passing tests / total tests.

## Common Gotchas

### Hash Computation
Hash = `base62(xxhash64(canonical_signature + body_normalized + docstring))`. Uses AST-based normalization, not raw text. Docstring is part of hash input.

### Clean Compile Output
When compile passes with zero errors AND zero warnings: **empty stdout, exit 0**. This is critical — the LLM should never see output unless something needs attention.

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

## Spec-Kit Structure

Each spec in `keel-speckit/` is **self-contained**. Read your assigned specs — they have everything you need. Do NOT read `PRD_1.md` (2000+ lines). Your specs extract all relevant content.

```
keel-speckit/
├── 000-graph-schema/spec.md        # Agent A — bedrock types
├── 001-treesitter-foundation/spec.md  # Agent A — Tier 1 parsing
├── 002-typescript-resolution/spec.md  # Agent A — Oxc Tier 2
├── 003-python-resolution/spec.md   # Agent A — ty Tier 2
├── 004-go-resolution/spec.md       # Agent A — heuristic Tier 2
├── 005-rust-resolution/spec.md     # Agent A — rust-analyzer Tier 2
├── 006-enforcement-engine/spec.md  # Agent B — compile + validation
├── 007-cli-commands/spec.md        # Agent B — all commands
├── 008-output-formats/spec.md      # Agent B — JSON, LLM, human
├── 009-tool-integration/spec.md    # Agent C — 9+ tool configs
├── 010-mcp-http-server/spec.md     # Agent C — serve modes
├── 011-vscode-extension/spec.md    # Agent C — VS Code display
├── 012-distribution/spec.md        # Agent C — cross-platform
└── test-harness/strategy.md        # All agents — oracles + corpus
```

## Frozen Contracts (Do NOT Modify)

These trait/struct signatures are frozen in Phase 0. Breaking a contract = immediate stop.

1. `LanguageResolver` trait — Agent A owns, Agent B consumes
2. `GraphStore` trait — Agent A owns, Agents B+C consume
3. `CompileResult` / `DiscoverResult` / `ExplainResult` structs — Agent B owns, Agent C consumes
4. JSON output schemas in `tests/schemas/` — Agents B+C own, external consumers depend on

## Related Documents

- [Design Principles](design-principles.md) — the "why" document (read before implementation)
- [Constitution](constitution.md) — non-negotiable articles
- [Agent Swarm Playbook](agent-swarm/README.md) — how agents coordinate (decomposed into 6 files)
- [Scope Limits](agent-swarm/scope-limits.md) — hard limits on agent scope and context management
- [Test Harness Strategy](keel-speckit/test-harness/strategy.md) — oracle definitions
- [PRD v2.1](docs/research/PRD_1.md) — master source (do NOT read directly — use specs)
