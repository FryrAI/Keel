# keel — Implementation Progress

> Last updated: 2026-02-16

## Honest Status Summary

**Ready for v0.1.0 release.** All CLI commands work, 4 language resolvers pass, 15 real-world repos
validate successfully, tool config generation ships for 9+ AI coding tools. Round 5 delivered the
last-mile: `keel init` generates hook configs and instruction files, Cargo metadata and release
pipeline are ready, VS Code extension is packageable, docs are written. 953 tests passing.

## Test Status — Actual Numbers

### What `cargo test --workspace` Reports

| Category | Count | Notes |
|----------|-------|-------|
| **Passing** | 953 | Round 5: tool config gen, 18 tests un-ignored, 5 new merge tests |
| **Ignored** | 47 | Remaining stubs (cursor/gemini hooks, hook timeout) |
| **Failing** | 0 | — |

### Where the Passing Tests Live

| Source | Tests | Notes |
|--------|-------|-------|
| crates/keel-core/ | 24 | SQLite, hash, config |
| crates/keel-parsers/ | 42 | tree-sitter, 4 resolvers, walker |
| crates/keel-enforce/ | 61 | Engine, violations, circuit breaker, batch, discover BFS, fix generator, naming |
| crates/keel-cli/ | 54 | CLI args, init merge logic, map resolve, fix/name, explain --depth |
| crates/keel-server/ | 41 | MCP + HTTP + watcher |
| crates/keel-output/ | 35 | JSON, LLM, human formatters, depth/backpressure/fix/name, token budget |
| tests/contracts/ | 66 | Frozen trait contracts |
| tests/fixtures/ | 10 | Mock graph + compile helpers |
| tests/integration/ | 31 | E2E workflows (real) |
| tests/resolution/ | 154 | 4 languages + barrel files |
| tests/cli/ | 2 | init keelignore + git hook |
| tests/server/ | 29 | MCP + HTTP + watch + lifecycle |
| tests/benchmarks/ | 13 | Map, parsing, parallel parsing |
| tests/output/ | 56 | JSON schema, LLM format, discover schema |
| tests/enforcement/ | 44 | Violations, batch, circuit breaker |
| tests/tool_integration/ | 31 | Claude Code hooks, instruction files, git hooks, hook execution |
| other integration | ~160 | Graph, parsing, correctness |
| **Total** | **953** | |

## Round 5: Last Mile to v0.1.0 (2026-02-16) — COMPLETED

### Tool Config Generation (P0.1)

Refactored `keel init` from single file into modular architecture:
- `init.rs` — entry point, `DetectedTool` enum, `detect_tools()` scanner
- `init/templates.rs` — `include_str!()` for all 20 templates (single binary)
- `init/merge.rs` — JSON deep merge + markdown `<!-- keel:start/end -->` marker merge
- `init/generators.rs` — per-tool config generation (9 tools + AGENTS.md)
- `init/hook_script.rs` — post-edit.sh + git pre-commit hook install

**Tool detection and generation:**

| Tool | Detection | Generates |
|------|-----------|-----------|
| Claude Code | `.claude/` dir | `.claude/settings.json` + `CLAUDE.md` |
| Cursor | `.cursor/` dir | `.cursor/hooks.json` + `.cursor/rules/keel.mdc` |
| Gemini CLI | `.gemini/` or `GEMINI.md` | `.gemini/settings.json` + `GEMINI.md` |
| Windsurf | `.windsurf/` or `.windsurfrules` | `.windsurf/hooks.json` + `.windsurfrules` |
| Letta Code | `.letta/` dir | `.letta/settings.json` + `LETTA.md` |
| Antigravity | `.agent/` dir | `.agent/rules/keel.md` + `.agent/skills/keel/SKILL.md` |
| Aider | `.aider.conf.yml` or `.aider/` | `.aider.conf.yml` + `.aider/keel-instructions.md` |
| Copilot | `.github/` dir | `.github/copilot-instructions.md` |
| GitHub Actions | `.github/workflows/` | `.github/workflows/keel.yml` |
| *(always)* | — | `AGENTS.md` (universal fallback) |

### Release Prep (P0.2)

- Workspace Cargo.toml: `license-file = "LICENSE"`, description, homepage, repository, keywords
- All 6 crate Cargo.tomls: crate-specific descriptions + workspace inheritance
- Fixed publish order in release.yml (keel-enforce before keel-output)
- Unified all repo URLs to `FryrAI/Keel`
- Created CHANGELOG.md for v0.1.0
- Created per-crate README.md files for crates.io

### VS Code Extension (P0.3)

- Created tsconfig.json, .vscodeignore, marketplace README, CHANGELOG
- Added vsce packaging to package.json
- Ready for `npm install && npm run compile && npm run package`

### Benchmarks (P0.4) — Post-O(n) Numbers

See real-world validation table below. Compile times dropped 2x-91x after O(n) fix.

### Documentation (P0.5)

Created 5 docs (all under 400 lines):
- `docs/getting-started.md` — install → init → map → compile in 5 minutes
- `docs/commands.md` — full command reference with examples
- `docs/agent-integration.md` — wiring keel into 9+ AI coding tools
- `docs/config.md` — keel.json reference, .keelignore
- `docs/faq.md` — troubleshooting and common questions

### Results
- 931 → 953 tests (+22: 18 un-ignored tool integration + 5 new merge.rs unit tests - 1 reclassified)
- 18 previously-ignored tool integration tests now pass
- All 15 real-world repos green with O(n) compile times

## Previous Rounds

### Round 4: Agent UX Polish (2026-02-13)

| Feature | What It Does |
|---------|-------------|
| `keel explain --depth 0-3` | Resolution chain truncation by depth level |
| `--max-tokens N` | Configurable global token budget for LLM output |
| `keel fix --apply` | Auto-apply fix plans with file writes + re-compile verification |

926 → 931 tests. 33 files changed, +2822/-499 lines.

### Round 3: LLM Experience (2026-02-13)

- `keel fix` — diff-style fix plans for E001-E005 violations
- `keel name` — module scoring by keyword overlap, convention detection
- `keel map --depth 0-3` — depth-aware output with hotspot detection
- `keel compile --depth 0-2` — backpressure signals (PRESSURE/BUDGET)

887 → 926 tests.

## Implementation Phase Status

### Phase 0: Contracts & Scaffold — DONE
### Phase 1: Parsing & Resolution — DONE (154 resolver tests)
### Phase 2: Enforcement — DONE (O(n) compile, circuit breaker, batch)
### Phase 3: Server & Integrations — DONE (MCP + HTTP + VS Code)
### Phase 4: Distribution — READY FOR RELEASE

## Real-World Validation — Round 5 (Post-O(n) Fix)

### 15-Repo Benchmark — ALL GREEN

| Repo | Lang | Nodes | Edges | X-file | Map(ms) | Compile(ms) | Old Compile(ms) | Speedup |
|------|------|-------|-------|--------|---------|-------------|-----------------|---------|
| axum | rust | 3621 | 3894 | 57 | 3048 | 4129 | 202870 | **49x** |
| cobra | go | 614 | 1509 | 536 | 241 | 327 | 1880 | **6x** |
| fastapi | python | 6617 | 6559 | 474 | 7164 | 15125 | 259478 | **17x** |
| fiber | go | 3657 | 9482 | 5344 | 1522 | 2384 | 33036 | **14x** |
| flask | python | 2116 | 2482 | 173 | 683 | 1049 | 8993 | **9x** |
| fzf | go | 892 | 1887 | 523 | 433 | 537 | 5819 | **11x** |
| gin | go | 1268 | 2435 | 613 | 415 | 661 | 5304 | **8x** |
| httpx | python | 1531 | 1957 | 177 | 561 | 833 | 5397 | **6x** |
| ky | typescript | 92 | 97 | 25 | 599 | 640 | 1304 | **2x** |
| pydantic | python | 11633 | 15920 | 1028 | 2985 | 7043 | 118892 | **17x** |
| ripgrep | rust | 4670 | 5756 | 581 | 1979 | 3029 | 276985 | **91x** |
| serde | rust | 3328 | 4424 | 256 | 2193 | 2664 | 135561 | **51x** |
| trpc | typescript | 2173 | 4218 | 742 | 12089 | 12390 | 55411 | **4x** |
| zod | typescript | 1039 | 1695 | 396 | 3387 | 3600 | 9489 | **3x** |
| zustand | typescript | 213 | 257 | 8 | 865 | 877 | 1344 | **2x** |
| **TOTAL** | | **43444** | **60572** | **10933** | | | | |

Zero orphans. Zero regressions. 4 consecutive green rounds.

**Highlights:**
- ripgrep compile: 277s → 3s (**91x** speedup)
- serde compile: 136s → 2.7s (**51x** speedup)
- fastapi compile: 259s → 15s (**17x** speedup)

## Remaining Work

### P1: Polish
- 47 ignored test stubs → real assertions (cursor/gemini hooks, hook timeout)
- Config format: TOML migration (keel.toml alongside keel.json)

### P2: Overdelivery
- Website (keel.engineer)
- Diff-aware compile (`--changed`, `--since HASH`)
- Streaming compile (`--watch` for CLI)
- Monorepo support

## Test Count History
207 → 338 → 442 → 446 → 455 → 467 → 478 → 874 → 887 → 926 → 931 → 953 (current)
