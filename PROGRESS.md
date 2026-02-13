# keel — Implementation Progress

> Last updated: 2026-02-12

## Honest Status Summary

**Core implementation is functional.** All CLI commands work, 4 language resolvers pass,
15 real-world repos validate successfully. Test infrastructure is now wired — all 8 previously
orphaned test directories have entry points and shared helpers. 318 stubs are wired but need
real assertions (CI Swarm Round 2 in progress).

## Test Status — Actual Numbers

### What `cargo test --workspace` Reports

| Category | Count | Notes |
|----------|-------|-------|
| **Passing** | 874 | After merging Round 2 agent work |
| **Ignored** | 78 | Remaining stubs + 13 FK-blocked benchmarks |
| **Failing** | 0 | — |

### Where the 478 Passing Tests Live

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
| tests/ (new entry points) | 11 | 3 | 8 | Wired in Round 1 |
| workspace root | 9 | 9 | 0 | — |
| **Total** | **478** | **462** | **16** | |

### 318 Ignored Stubs (Wired, Empty Bodies)

All 8 previously orphaned directories now have top-level entry points. Stubs compile
and are visible to `cargo test` but skipped via `#[ignore]`.

| Directory | Stubs | What they test |
|-----------|-------|----------------|
| tests/graph/ | 70 | Node/edge creation, hash, SQLite, schema migration |
| tests/parsing/ | 59 | Per-language parsing, incremental, parallel, keelignore |
| tests/graph_correctness/ | 50 | Per-language correctness, cross-language, edge accuracy |
| tests/enforcement/ | 112 | E001-E005, W001-W002, circuit breaker, batch, explain |
| tests/output/ | 50 | JSON schema compliance, LLM format, error codes |
| tests/server/ | 29 | HTTP endpoints, MCP server, watch mode, lifecycle |
| tests/tool_integration/ | 49 | Claude Code, Cursor, Gemini, git hooks, instruction files |
| tests/benchmarks/ | 31 | Parsing, hash, SQLite, compile, discover perf |
| **TOTAL** | **318** | — |

## Implementation Phase Status

### Phase 0: Contracts & Scaffold — DONE
All frozen contracts defined. All crates compile. Core types stable.

### Phase 1: Parsing & Resolution — DONE (well-tested)
All 4 language resolvers implemented and passing. 154 resolver tests with real assertions.

| Language | Tier 2 Enhancer | Resolver Tests |
|----------|----------------|----------------|
| TypeScript | Oxc (`oxc_resolver` + `oxc_semantic`) | 42 |
| Python | ty (subprocess) + heuristics | 41 |
| Go | tree-sitter heuristics + cross-file | 26 |
| Rust | Heuristic resolver | 45 |

### Phase 2: Enforcement — DONE (under-tested)
Engine and violation checkers implemented. Circuit breaker, batch mode, suppression work.
**Gap:** 112 ignored enforcement stubs need real assertions.

### Phase 3: Server & Integrations — DONE (under-tested)
MCP + HTTP server, VS Code extension, tool hooks implemented.
**Gap:** 29 server + 49 tool integration stubs need real assertions.

### Phase 4: Distribution — SCAFFOLD ONLY
CI pipeline and install script exist. No release published.

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

## Infrastructure Status

| Item | Status | Notes |
|------|--------|-------|
| Test entry points | **DONE** | All 8 orphaned dirs wired (Round 1: ci/test-infra) |
| Shared test helpers | **DONE** | `tests/common/mod.rs`: `keel_bin()`, `setup_test_project()`, `create_mapped_project()`, `in_memory_store()`, generators |
| Empty-body stubs | **16 passing + 318 ignored** | Need real assertions |
| Compile performance | **O(n^2)** | 62s single-file compile in test; bugs agent targeting |
| Files > 400 lines | **2 remaining** | `test_json_schema_contract.rs` (467L), `test_multi_language.rs` (407L) |

## CI Swarm Round 2 (2026-02-12) — IN PROGRESS

3 agents running in `keel-ci` tmux session, each in a separate git worktree.

| Agent | Worktree | Branch | Target |
|-------|----------|--------|--------|
| test-infra | `$HOME/keel-ci-test-infra` | ci/test-infra | 179 stubs (graph + parsing + graph_correctness) + 8 resolution |
| enforcement | `$HOME/keel-ci-enforcement` | ci/enforcement | 102 stubs (cli + tool_integration) + 31 benchmarks |
| bugs | `$HOME/keel-ci-bugs` | ci/bugs | O(n^2) perf fix + corpus validation + 6 integration |

## Remaining Work — Prioritized

### P0: Implement Ignored Stubs (CI Swarm Round 2)
- 318 ignored stubs → real assertions across 8 test directories
- Each agent runs `/ralph-loop` autonomously

### P1: Fix Known Issues
- Compile performance — O(n^2) violation checking (ripgrep: 4.6min, fastapi: 4.3min)
- 2 files > 400 lines need decomposition

### P2: Hardening
- Graph correctness tests — validate node/edge accuracy per language
- Benchmark CI — run perf tests on release builds

### P3: Distribution
- First release build and publish
- VS Code extension marketplace submission

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
207 → 338 → 442 → 446 → 455 → 467 → 478 → 874 (current)
