# Continuous Improvement Playbook

```yaml
status: active
purpose: "Repeatable swarm to find and fix all bugs until convergence"
usage: "Run anytime. Idempotent. Safe to re-execute."
```

> **Goal:** Zero ignored tests, zero failures, all 15 repos green, no known bugs.
> Only after convergence do we proceed to distribution and release.
>
> **STATUS: CONVERGED (Round 14).** 1071 tests passing, 0 ignored, 0 failed, 0 clippy warnings.

---

## 1. Prerequisites

### Software

```bash
# Rust toolchain
rustup show           # Must show stable toolchain
cargo --version       # 1.75+

# Sandbox dependencies (Linux)
bwrap --version       # bubblewrap
socat -V | head -1    # socat (for tmux teammate mode)

# Claude Code
claude --version      # Must support agent teams
```

### Configuration

Verify `~/.claude/settings.json` has agent teams + sandbox enabled:

```json
{
  "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
  "teammateMode": "tmux",
  "sandbox": {
    "enabled": true,
    "autoAllowBashIfSandboxed": true,
    "allowUnsandboxedCommands": false
  }
}
```

### Repository State

```bash
cd $HOME/Curosor_Projects/Keel
git status            # Must be clean (commit or stash first)
cargo test --workspace 2>&1 | tail -5  # Baseline — note current pass/fail
```

---

## 2. Quick Start

```bash
# Dry run — checks prerequisites, shows what would happen
bash scripts/ci-swarm.sh --dry-run

# Full launch — creates worktrees, tmux session, 3 agent panes
bash scripts/ci-swarm.sh
```

The script:
1. Verifies prerequisites
2. Creates 3 git worktrees (or reuses existing ones)
3. Writes agent prompt files to `/tmp/claude/ci-prompts/`
4. Launches a 4-pane tmux session
5. Starts Claude Code in each pane with the appropriate prompt

**You only interact with Pane 0 (orchestrator).** Panes 1-3 are autonomous.

---

## 3. Architecture

```
tmux session "keel-ci" (4 panes)

Pane 0: ORCHESTRATOR (you — root repo)
  - Monitor panes 1-3 via /tmux-observe
  - Run /ralph-loop for cross-pane gate checks
  - Merge branches when panes complete
  - Final validation: cargo test --workspace + 15-repo corpus

Pane 1: TEST-INFRA (worktree — branch ci/test-infra)
  - Wire orphaned test directories into cargo test
  - Build shared test helpers (tests/common/)
  - Fix import paths in 450 uncompiled stubs
  - Goal: all 60 stub files compile (may still be #[ignore])

Pane 2: ENFORCEMENT (worktree — branch ci/enforcement)
  - Implement E001-E005, W001-W002 behavioral tests
  - Implement output format compliance tests
  - Implement server endpoint tests
  - Goal: all enforcement/output/server stubs pass

Pane 3: BUGS (worktree — branch ci/bugs)
  - Run full test suite, find failures, fix code
  - Run 15-repo validation, find regressions, fix code
  - Fix compile perf (O(n^2) violation checking)
  - Goal: zero failures, zero regressions
```

Each pane runs a 3-role agent team internally:
- **Coder** — writes and fixes code
- **Architect** — reviews approach, catches design issues
- **Devil's advocate** — challenges assumptions, finds edge cases

### When to Use Swarm vs Single Session

| Scenario | Approach | Why |
|----------|----------|-----|
| Independent crates (e.g., 4 language resolvers) | **Swarm** (3 worktrees) | No shared types, natural file isolation |
| Cross-crate dependency chain (e.g., types → formatters → CLI) | **Single session** | Shared types cause merge conflicts in parallel |
| Bug fixing across codebase | **Swarm** (3 worktrees by area) | Bugs are independent, fixes don't conflict |
| New feature with LLM output implications | **Single session** | Feature touches enforce → output → cli in sequence |

Round 3 used a single session because fix/name/depth/backpressure changes followed a tight dependency chain: `keel-enforce` types → `keel-output` formatters → `keel-cli` commands. Parallelizing would have caused merge conflicts on shared types like `PressureLevel` and `FixPlan`.

---

## 4. Launch Prompts

Prompt files live in `scripts/ci-prompts/`. Each is loaded by `ci-swarm.sh`.

| File | Pane | Purpose |
|------|------|---------|
| `test-infra.md` | 1 | Wire stubs, build helpers, fix imports |
| `enforcement.md` | 2 | Implement behavioral tests |
| `bugs.md` | 3 | Find and fix bugs across codebase |

See each file for the full prompt. Key constraints in every prompt:
- Max 15 files per session
- Use `/ralph-loop` for autonomous test-fix cycles
- Create agent team with 3 roles (coder, architect, devil's advocate)
- Commit after each meaningful fix
- Never modify frozen contracts

---

## 5-8. Operations Reference (condensed)

**Monitoring:** Use `/tmux-observe` for passive monitoring. Run gate checks with
`cargo test --workspace` in each worktree. Intervene if agent stuck 15+ min.

**Merge order:** test-infra → enforcement → bugs (dependency order). Run
`cargo test --workspace` after each merge.

**Convergence criteria:** 0 failures, 0 clippy warnings, all stubs implemented,
15-repo validation green, no files >400 lines, deterministic results.

**Cleanup:** `git worktree remove`, `git branch -d`, `tmux kill-session -t keel-ci`,
`git worktree prune`.

---

## 9. Round 4 Results (2026-02-13) — COMPLETED

Features shipped in Round 4, from the candidates list:

| Priority | Feature | Description | Status |
|----------|---------|-------------|--------|
| **P0** | `keel fix --apply` | Auto-apply fix plans with file writes + re-compile verification | **DONE** |
| **P1** | Streaming compile | `--watch` mode for continuous agent loops | Deferred |
| **P2** | `--max-tokens N` | Configurable global token budget for LLM output (replaces hardcoded 500) | **DONE** |
| **P3** | `keel explain --depth 0-3` | Resolution chain truncation by depth level | **DONE** |
| **P4** | Map diff | `--since HASH` for structural delta (only show what changed) | Deferred |

- Tests: 926 → 931 (+5 new tests)
- 33 files changed, +2822/-499 lines
- Single-session approach (same as Round 3)

---

## 10. Round 7 Results (2026-02-16) — COMPLETED

**Approach:** Claude Code agent team (3 agents in parallel, ~30 min wall time)

| Agent | Focus | Commits |
|-------|-------|---------|
| test-infra | Resolution stubs, graph tests, file splits | 3 commits |
| enforcement | Benchmark timing fixes, CLI/tool verification | 1 commit |
| bugs | Feature implementations, perf, clippy, docs | 3 commits |

### Key Deliverables
- **+15 tests passing** (895 → 910), **-14 ignored** (107 → 93)
- `ModuleProfile.class_count` + `line_count` fields added to struct + SQLite
- `ResolvedEdge.resolution_tier` tracking across all 4 resolvers
- `get_node()` previous_hashes fallback for renamed functions
- SQLite WAL + performance pragmas (NORMAL sync, 8MB cache, 256MB mmap)
- Compile engine pre-fetches nodes once per file (was 3x redundant)
- Lazy resolver creation in CLI
- All clippy warnings fixed (5 → 0)
- Split oversized `test_sqlite_storage.rs` (475 → 210 + 297 lines)
- 18 resolution test files with real assertions across all 4 languages

### Convergence Status
- **910 passed, 0 failed, 93 ignored, 0 clippy warnings**
- All 93 ignored tests are feature-blocked (not missing test code)
- No files over 400 lines
- Convergence achieved for current feature set

### Next Round Candidates
See Round 8 plan below.

---

## 11-13. Round 8-9 Results — COMPLETED

- **Round 8** (2026-02-16): Agent UX — discover paths, search, watch, --changed, GitHub Action, MCP map. 919 passed, 93 ignored.
- **Round 9** (2026-02-16): check, analyze, compile --delta, discover --context. 927 passed, 93 ignored.

---

## 14. Round 10 Results (2026-02-17) — COMPLETED

**Approach:** 3-agent team (hooks, bugs, core). 93 → 68 ignored (-25).
Schema v2 migration, module node auto-creation, dynamic dispatch confidence,
Cursor/Gemini hook fixes, 5 feedback bugs fixed. 957 passed, 0 failed, 68 ignored.

---

## 15. Round 11 Results (2026-02-17) — COMPLETED

**Approach:** 3-agent swarm (polish, resolution-a, resolution-b). 68 → 55 ignored (-13).
UX polish (name reliability, check caller summary, --context N, deprecate where),
Go imports, Python __all__, Rust use statements, TS package.json exports.
972 passed, 0 failed, 55 ignored.

---

## 16. Round 12 Results (2026-02-17) — COMPLETED

**Approach:** 2-agent swarm (resolution-a: Go+Python, resolution-b: Rust+TS)

### Key Deliverables
- **47 tests un-ignored** across all 4 languages via enhanced Tier 2 heuristics
- Go: interface methods, receiver methods, visibility, package scoping (12 tests)
- Python: star imports, subprocess, package resolution (11 tests)
- Rust: impl blocks, mod declarations (6 tests)
- TypeScript: namespace resolution (4 tests)
- Parallel parsing, large codebase, integration tests (14 tests)

### Test Status
- **1038 passed, 0 failed, 8 ignored, 0 clippy warnings**
- 55 → 8 ignored (-47)

---

## 17. Round 13 Results (2026-02-17) — COMPLETED (CONVERGENCE)

**Approach:** 2-agent swarm (Rust tier3: 6 tests, TS tier3: 2 tests)

### Key Deliverables
- **All 8 remaining TIER3 tests implemented** — zero ignored tests achieved
- Rust trait bound resolution (`<T: Trait>` → method resolution at 0.65 confidence)
- Rust where clause resolution (`where T: Trait` with multi-line support)
- Rust supertrait method resolution (`trait A: B + C` hierarchy expansion)
- Rust associated type resolution (`type Output = String;` extraction)
- Rust derive macro resolution (`#[derive(Debug)]` → TypeRef references)
- Rust attribute macro resolution (`#[tokio::main]` → Call references)
- TS module augmentation (`declare module 'X' { ... }`)
- TS project reference resolution (tsconfig `"references"` array)
- New file: `trait_resolution.rs` (383 lines) with 6 unit tests
- Fixed 8 clippy warnings across Go and Rust modules

### Test Status
- **1052 passed, 0 failed, 0 ignored, 0 clippy warnings**
- Convergence criteria fully met

### Convergence Verification
| Criterion | Target | Actual |
|-----------|--------|--------|
| Tests passing | 1046+ | **1052** |
| Tests ignored | 0 | **0** |
| Tests failed | 0 | **0** |
| Clippy warnings | 0 | **0** |
| Files over 400 lines | 0 | **0** |

---

## 18. Round 14 Results (2026-02-17) — COMPLETED

**Approach:** Single session — telemetry engine, config command, install polish.

### Key Deliverables
- **Privacy-safe telemetry** (`telemetry.rs`): Separate `telemetry.db`, records command metrics only (no paths/code/git), aggregate/prune/recent queries
- **Config extension**: `Tier` (Free/Team/Enterprise), `TelemetryConfig` (opt-OUT remote), `NamingConventionsConfig` — all backward-compatible
- **`keel config` command**: Get/set with dot-notation (`keel config telemetry.enabled false`)
- **Telemetry recorder**: Wraps every command with timing, silently fails
- **`keel init` .gitignore**: Auto-adds `.keel/graph.db`, `telemetry.db`, `session.json`, `cache/`
- **`keel stats` upgrade**: Shows telemetry aggregate (invocations, avg times, top commands, languages)
- **README fix**: `config.toml` → `keel.json`, added Install section
- **install.sh**: `--version` flag, shell completion hints

### Test Status
- **1071 passed, 0 failed, 0 ignored, 0 clippy warnings**
- 14 files (5 new + 9 modified), all under 400 lines

---

## Appendix: Running a Single Pane Manually

If you don't want the full swarm, you can run any pane independently:

```bash
# Create one worktree
git worktree add $HOME/keel-ci-bugs -b ci/bugs

# Launch Claude Code with the prompt
cd $HOME/keel-ci-bugs
claude --dangerously-skip-permissions

# Then paste the contents of scripts/ci-prompts/bugs.md as your first message
```

This is useful for:
- Focusing on one area at a time
- Debugging a specific agent's work
- Running on a machine with limited resources
