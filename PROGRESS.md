# keel — Implementation Progress

> Last updated: 2026-02-11

## Honest Status Summary

**Core implementation is functional.** All CLI commands work, 4 language resolvers pass,
15 real-world repos validate successfully. However, test coverage has significant gaps:
the 467 passing tests cover crate internals and resolution well, but higher-level behavioral
tests (enforcement rules, output formats, server endpoints, graph correctness) exist only
as uncompiled empty stubs.

## Test Status — Actual Numbers

### What `cargo test --workspace` Reports

| Category | Count | Notes |
|----------|-------|-------|
| **Passing** | 467 | Real assertions (459) + empty-body stubs (8) |
| **Ignored** | 58 | CLI integration stubs (53) + perf benchmarks (5) |
| **Failing** | 0 | — |

### Where the 467 Passing Tests Live

| Source | Tests | Real | Empty stubs | Notes |
|--------|-------|------|-------------|-------|
| crates/keel-core/ | 24 | 24 | 0 | SQLite, hash, config |
| crates/keel-parsers/ | 42 | 42 | 0 | tree-sitter, 4 resolvers, walker |
| crates/keel-enforce/ | 47 | 47 | 0 | Engine, violations, circuit breaker, batch |
| crates/keel-cli/ | 34 | 34 | 0 | CLI arg parsing, --json |
| crates/keel-server/ | 32 | 32 | 0 | MCP + HTTP + watcher |
| crates/keel-output/ | 16 | 16 | 0 | JSON, LLM, human formatters |
| tests/contracts/ | 66 | 66 | 0 | Frozen trait contracts |
| tests/fixtures/ | 10 | 10 | 0 | Mock graph + compile helpers |
| tests/integration/ | 31 | 31 | 0 | E2E workflows (real) |
| tests/resolution/ | 154 | 146 | 8 | 4 languages + barrel files |
| tests/cli/ | 2 | 2 | 0 | init keelignore + git hook |
| workspace root | 9 | 9 | 0 | — |
| **Total** | **467** | **459** | **8** | |

### 58 Ignored Tests

| Source | Count | Type |
|--------|-------|------|
| tests/cli/ (53) | 53 | Empty stubs with `#[ignore = "Not yet implemented"]` |
| tests/integration/test_large_codebase.rs | 5 | Real perf benchmarks (50-100k LOC generation) |

### 450 Orphaned Stubs (NEVER COMPILED)

These directories have `mod.rs` files but **no top-level `tests/*.rs` entry point**, so
`cargo test` never compiles them. All contain empty-body `#[test] #[ignore]` functions
with comment-only bodies (GIVEN/WHEN/THEN placeholders).

| Directory | Files | #[test] stubs | What they would test |
|-----------|-------|---------------|----------------------|
| tests/graph/ | 7 | 70 | Node/edge creation, hash, SQLite, schema migration |
| tests/enforcement/ | 13 | 112 | E001-E005, W001-W002, circuit breaker, batch, explain |
| tests/output/ | 8 | 50 | JSON schema compliance, LLM format, error codes |
| tests/parsing/ | 8 | 59 | Per-language parsing, incremental, parallel, keelignore |
| tests/server/ | 4 | 29 | HTTP endpoints, MCP server, watch mode, lifecycle |
| tests/tool_integration/ | 6 | 49 | Claude Code, Cursor, Gemini, git hooks, instruction files |
| tests/benchmarks/ | 7 | 31 | Parsing, hash, SQLite, compile, discover perf |
| tests/graph_correctness/ | 7 | 50 | Per-language correctness, cross-language, edge accuracy |
| **TOTAL** | **60** | **450** | — |

**Impact:** The spec-kit originally planned ~500 integration tests across these directories.
They were scaffolded but never wired up or implemented. This means enforcement rules (E001-E005),
output format compliance, server endpoints, and tool integration have **zero dedicated tests**
outside crate-level unit tests.

## Implementation Phase Status

### Phase 0: Contracts & Scaffold — DONE

All frozen contracts defined. All crates compile. Core types stable.

### Phase 1: Parsing & Resolution — DONE (well-tested)

All 4 language resolvers (TypeScript, Python, Go, Rust) implemented and passing.
154 resolver tests with real assertions. Tier 2 enhancers integrated.

| Language | Tier 2 Enhancer | Resolver Tests |
|----------|----------------|----------------|
| TypeScript | Oxc (`oxc_resolver` + `oxc_semantic`) | 42 |
| Python | ty (subprocess) + heuristics | 41 |
| Go | tree-sitter heuristics + cross-file | 26 |
| Rust | Heuristic resolver | 45 |

### Phase 2: Enforcement — DONE (under-tested)

Engine and violation checkers implemented. Circuit breaker, batch mode, suppression work.
**Gap:** 112 planned enforcement behavioral tests are uncompiled stubs. The engine_tests.rs
and violations_extended.rs in-crate tests cover basic paths, but no dedicated tests for:
- E001 broken callers across files
- E002/E003 type hint + docstring enforcement per language
- E005 arity mismatch detection
- W001 placement validation
- Progressive adoption (new vs pre-existing code)

### Phase 3: Server & Integrations — DONE (under-tested)

MCP + HTTP server, VS Code extension, tool hooks all implemented.
**Gap:** 29 server endpoint tests + 49 tool integration tests are uncompiled stubs.
HTTP endpoint testing relies on crate-level mcp_tests.rs (28 tests) only.

### Phase 4: Distribution — SCAFFOLD ONLY

CI pipeline and install script exist. No release has been published.

## Real-World Validation (2026-02-11) — STRONG

### 15-Repo Validation — ALL GREEN

| Repo | Lang | Nodes | Edges | X-file | Map(ms) | Compile(ms) |
|------|------|-------|-------|--------|---------|-------------|
| axum | rust | 3760 | 4028 | 52 | 3201 | 202870 |
| cobra | go | 637 | 1565 | 553 | 347 | 1880 |
| fastapi | python | 6617 | 6550 | 465 | 7379 | 259478 |
| fiber | go | 3954 | 7649 | 3167 | 1919 | 33036 |
| flask | python | 2116 | 2482 | 173 | 842 | 8993 |
| fzf | go | 892 | 1887 | 523 | 515 | 5819 |
| gin | go | 1268 | 2434 | 613 | 601 | 5304 |
| httpx | python | 1533 | 1965 | 177 | 663 | 5397 |
| ky | typescript | 150 | 158 | 33 | 865 | 1304 |
| pydantic | python | 11634 | 15960 | 1028 | 3393 | 118892 |
| ripgrep | rust | 4668 | 5754 | 581 | 2199 | 276985 |
| serde | rust | 3328 | 4424 | 256 | 2388 | 135561 |
| trpc | typescript | 2173 | 4218 | 742 | 12124 | 55411 |
| zod | typescript | 1039 | 1695 | 396 | 3262 | 9489 |
| zustand | typescript | 218 | 271 | 11 | 1017 | 1344 |
| **TOTAL** | | **43987** | **61040** | **8770** | | |

3 consecutive green rounds. Zero orphans. Deterministic across rounds.

## Infrastructure Gaps

### Missing Test Entry Points (P0)
8 directories in `tests/` have no top-level `.rs` file to wire them into `cargo test`.
Creating these entry points (like `tests/enforcement.rs`, `tests/server.rs`, etc.) is a
prerequisite before any stub can be implemented.

### No Shared Test Helpers (P1)
Each integration test file duplicates `keel_bin()`, `setup_ts_project()`, and similar
helpers. A `tests/common/` module would reduce duplication and make new tests faster to write.

### Empty-Body Stubs (P1)
8 resolution tests pass trivially with `fn test_foo() {}`. These should either be
implemented or removed to avoid inflating pass counts.

### Performance Testing (P2)
5 real perf benchmarks exist (test_large_codebase.rs) but are `#[ignore]`. No CI job
runs them. Compile time on large repos is O(n^2) — the biggest gap for production use.

### Tool Integration Testing (P2)
49 stubs describe how keel should integrate with Claude Code, Cursor, Gemini, git hooks,
and instruction files. None are implemented. These would catch regressions in the hook/config
generation paths.

## Remaining Work — Prioritized

### P0: Test Infrastructure
1. Create top-level `tests/*.rs` entry points for all 8 orphaned directories
2. Build shared test helpers (`tests/common/mod.rs`) for binary path, temp project setup
3. Fix import paths so stubs can compile (many reference old API signatures)

### P0: Implement Critical Behavioral Tests
4. Enforcement tests (E001-E005) — validate violation detection against real code changes
5. Output format tests — verify JSON schema compliance, LLM format structure
6. Server endpoint tests — HTTP and MCP tools return correct payloads

### P1: Fix Known Issues
7. Compile performance — O(n^2) violation checking (ripgrep: 4.6min, fastapi: 4.3min)
8. 8 empty-body resolution stubs — implement or delete

### P2: Hardening
9. Graph correctness tests — validate node/edge accuracy per language
10. Tool integration tests — hook generation, instruction file output
11. Benchmark CI — run perf tests on release builds

### P3: Distribution
12. First release build and publish
13. VS Code extension marketplace submission

## Historical Context

### Agent Swarm Results (2026-02-09 to 2026-02-10)
- **Enforcement Team:** 6 commits, +1983 -132 lines
- **Surface Team:** 4 commits, +1665 -189 lines
- **Foundation Team:** 1 commit, +2159 -312 lines

### Critical Gap Fixes Applied
1. Cross-file resolution (Go + Rust now produce edges)
2. Compile persistence (writes back to SQLite)
3. Watch mode debounce (300ms)
4. Config loading (TOML deserialization)
5. Init improvements (.keelignore + git hook)
6. Map persistence (writes to graph store)
7. Compile file filtering (specific files only)
8. JSON schema compliance (all required fields)
9. FK constraint fix (deferred foreign keys in transactions)
10. Data integrity (UPSERT, UNIQUE constraints, orphan cleanup)
11. Circuit breaker per-hash keying
12. W002 test file exclusion

### Test Count History
207 → 338 → 442 → 446 → 455 → 467 (current)
