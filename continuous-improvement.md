# Continuous Improvement Playbook

```yaml
status: active
purpose: "Repeatable swarm to find and fix all bugs until convergence"
usage: "Run anytime. Idempotent. Safe to re-execute."
```

> **Goal:** Zero ignored tests, zero failures, all 15 repos green, no known bugs.
> Only after convergence do we proceed to distribution and release.

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

## 11. Round 8 Plan: Distribution & Real-World Features

**Approach:** 3-agent swarm (same as Round 7) + manual infrastructure steps

### Blockers (must be done first, in order)

#### B1: End-to-end dogfood on a real project
Run `keel init && keel map && keel compile` on FryrAI's own repos (Fryr, SpexAI).
Find every rough edge in the actual workflow — not test fixtures. This is the single
highest-leverage thing before shipping.

#### B2: Release-mode benchmarks
Build `cargo build --release`, run the 15-repo benchmark, update PROGRESS.md and
README.md with real release-mode numbers. Debug-mode claims (200ms compile, 5s map)
are likely 3-10x better in release.

#### B3: Wire up `keel serve --mcp` map tool
MCP server is functional (compile, discover, where, explain all work) but `keel/map`
is stubbed. Wire up the real map implementation from `commands/map.rs`.
**File:** `crates/keel-server/src/mcp.rs:295`

### Tier 1: High-Value Features (this session)

#### T1.1: Diff-aware compile (`--changed`)
**Complexity: LOW.** Compile already accepts file paths. Just need:
1. Add `--changed` flag to `crates/keel-cli/src/cli_args.rs`
2. Add git diff helper (shell out to `git diff --name-only HEAD`, ~20 lines)
3. Modify `compile.rs` to populate files from git when `--changed` is set
4. Also add `--since <commit>` for `git diff --name-only <commit>..HEAD`
**Tests:** Add 2-3 tests in `tests/cli/test_compile.rs`

#### T1.2: GitHub Action on marketplace
**Complexity: MEDIUM.** No action.yml exists yet. Create:
1. `action.yml` — composite action (shell-based, no JS dependency)
2. Downloads keel binary from GitHub Releases (version input, default latest)
3. Runs `keel compile` on changed files (uses `--changed` from T1.1)
4. Outputs: violation count, exit code, summary for PR annotations
5. Caches keel binary by version
**Structure:**
```
.github/actions/keel/
  action.yml
  entrypoint.sh
```
Or separate repo `FryrAI/keel-action` for marketplace publishing.

#### T1.3: Homebrew tap
**Complexity: LOW.** Formula template already exists at `dist/homebrew/keel.rb`.
1. Create `FryrAI/homebrew-tap` GitHub repo (manual — needs org access)
2. Add `homebrew` job to `.github/workflows/release.yml`:
   - Download checksums from release artifacts
   - Replace `VERSION_PLACEHOLDER` and `SHA256_*` in `keel.rb`
   - Push updated formula to `FryrAI/homebrew-tap`
3. Needs: GitHub PAT with repo write scope stored as `HOMEBREW_TAP_TOKEN` secret

### Tier 2: Enterprise Features (next session)

#### T2.1: Monorepo support
**Complexity: MEDIUM-HIGH.** Biggest architectural change.
**Design decision needed:** centralized `.keel/` at workspace root with package scopes
in the DB, vs. per-package `.keel/` directories + aggregation.

**Recommended: centralized with package field.**
1. Add `package_id: Option<String>` to `GraphNode` in `types.rs`
2. Add `packages` table to SQLite schema
3. Workspace detection: walk up from CWD, detect `package.json` workspaces,
   `Cargo.toml` workspace members, `go.work`, `pyproject.toml` with `[tool.hatch]`
4. `keel init --workspace` scans and registers all packages
5. `keel compile --package <name>` or auto-detect from CWD
6. Cross-package edge resolution (imports between packages)
**Files affected:** config.rs, types.rs, sqlite.rs, init.rs, compile.rs, map.rs
**Tests:** ~10 new tests in `tests/integration/`

#### T2.2: `keel watch` (CLI file watcher)
Reuse watcher from keel-server. Pure CLI mode: watch for changes, auto-compile,
print violations as they appear. Debounce 200ms. Exit on Ctrl+C with summary.
**Complexity: LOW.** `crates/keel-server/src/watcher.rs` already exists.

### Tier 3: Polish

#### T3.1: Un-ignore Cursor/Gemini hook tests (15 tests)
Implement hook generation for Cursor (`hooks.json` + `.mdc`) and Gemini
(`settings.json` + `GEMINI.md`) in `crates/keel-cli/src/init/generators.rs`.
Templates already exist in `init/templates.rs`.

#### T3.2: Release benchmarks update
After release-mode benchmarks, update all marketing: README, PROGRESS, landing page.

### Swarm Assignment (3 agents)

| Agent | Tasks | Files |
|-------|-------|-------|
| **features** | T1.1 (--changed), T2.2 (watch) | cli_args.rs, compile.rs, new watch.rs |
| **distribution** | T1.2 (GitHub Action), T1.3 (Homebrew CI job) | action.yml, release.yml |
| **integration** | B3 (MCP map), T3.1 (Cursor/Gemini hooks), tests | mcp.rs, generators.rs |

**Blockers B1 and B2** should be run manually before the swarm launches.

### Success Criteria

| Criterion | How to verify |
|-----------|--------------|
| `keel compile --changed` works | `git diff` + compile on real repo |
| GitHub Action runs in CI | Test workflow with `uses: FryrAI/keel-action@v1` |
| `brew install FryrAI/tap/keel` works | Test on macOS (or CI) |
| `keel serve --mcp` map tool works | JSON-RPC call returns real map data |
| 15 Cursor/Gemini tests pass | `cargo test --workspace` shows ≤78 ignored |
| Release benchmarks documented | PROGRESS.md has release-mode numbers |

---

## 12. Round 8 Results (2026-02-16) — COMPLETED

**Approach:** Single session, agent UX focus based on Claude Code dogfood feedback

### Key Deliverables
- `keel discover <path>` — list all symbols in a file, `--name` search by name
- `keel search <term>` — graph-wide name search with substring fallback
- `keel watch` — 200ms debounce auto-compile on file changes
- `keel compile --changed` / `--since <commit>` — git-diff-aware compilation
- Fixed empty hashes in E002/E003/W001/W002 violations
- Enriched `keel map --llm` with function names under MODULE lines
- GitHub Action (`.github/actions/keel/`) for CI marketplace
- Homebrew tap automation in `release.yml`
- Wired real MCP `keel/map` tool with `file_path` parameter
- Updated templates with new commands and common mistakes section

### Test Status
- **919 passed, 0 failed, 93 ignored, 0 clippy warnings**
- 53 files changed, +3304/-318 lines

---

## 13. Round 9 Results (2026-02-16) — COMPLETED

**Approach:** Single session, ~3.6x agent token cost reduction

### Key Deliverables
- `keel check` — pre-edit risk assessment (safe/caution/danger) with callers, callees, violations
- `keel analyze` — architectural observations (smells, refactoring) from graph data
- `keel compile --delta` — snapshot-based error diffing (only new/resolved violations)
- `keel discover --context [N]` — inline source code snippets eliminate separate file reads
- Fixed `keel name` — populate module_profiles during map, fallback scoring
- Split `types.rs` (335 lines) into `types/` directory (5 files)

### Test Status
- **927 passed, 0 failed, 93 ignored, 0 clippy warnings**
- +8 tests from Round 8

---

## 14. Round 10 Plan: Test Convergence Sprint

**Approach:** 3-agent team, target 93 → ~66 ignored (27 tests un-ignored)

### Analysis of 93 Ignored Tests

| Category | Count | Actionable? |
|----------|-------|-------------|
| Cursor hooks (hooks.json + MDC) | 8 | **YES** — generators exist, tests just need `.cursor/` dir |
| Gemini hooks (settings.json + GEMINI.md) | 8 | **YES** — generators exist, tests just need `.gemini/` dir |
| Hook execution (timeout + concurrent) | 2 | **YES** — add compile timeout + file locking |
| Schema v2 migration | 2 | **YES** — define v2 schema, implement migration |
| Module node per-file auto-creation | 4 | **YES** — parsers need Module node per source file |
| Dynamic dispatch confidence | 1 | **YES** — edge confidence classification |
| Large codebase perf | 5 | NO — debug mode too slow, needs release CI |
| Parser traits (method + corpus) | 3 | NO — needs API change + large corpus |
| Go advanced resolution | 15 | NO — cross-file, type inference |
| Python advanced resolution | 16 | NO — __all__, star imports, ty subprocess |
| Rust advanced resolution | 16 | NO — impl blocks, macros, trait resolution |
| TypeScript advanced resolution | 4 | NO — module augmentation, project refs |

### Feedback Bugs (from claudecode_feedback.md)

| # | Bug | Severity | Root Cause | Fix |
|---|-----|----------|-----------|-----|
| F1 | `discover --name` broken | **HIGH** | `find_nodes_by_name(name, "", "")` — SQL requires `kind=""` matches nothing | Make empty kind/exclude_file into wildcards |
| F2 | `keel name` placement wrong | **MEDIUM** | Scoring doesn't weight by path relevance | Add path-keyword matching to scoring |
| F3 | Compile not showing deltas | **DONE** | `--delta` implemented in R9 | Wire `--delta` into post-edit hook templates |
| F4 | Duplicate hash entries (decorator vs fn) | **LOW** | Parser indexes decorator line AND function line | Deduplicate in parser by skipping decorator-only nodes |
| F5 | `check` RISK=DANGER for safe functions | **LOW** | Risk conflates health (violations) with structural risk (callers) | Separate risk dimensions |

### Swarm Assignment (3 agents)

| Agent | Tasks | Target Tests | Files |
|-------|-------|-------------|-------|
| **hooks** | Fix Cursor/Gemini test setup, adjust templates/expectations, wire `--delta` into hook templates, implement hook timeout + concurrent handling | 20 | test_cursor_hooks.rs, test_gemini_hooks.rs, test_hook_execution.rs, templates |
| **bugs** | F1: fix `find_nodes_by_name` wildcard, F2: fix `name` scoring, F4: deduplicate decorator entries, F5: fix risk scoring in `check` | 0 (bug fixes) | sqlite_queries.rs, name.rs, check.rs, parsers |
| **core** | Schema v2 migration, module node per-file creation, dynamic dispatch confidence, update PROGRESS.md | 7 | sqlite.rs, parsers, graph correctness tests, PROGRESS.md |

### Success Criteria

| Criterion | How to verify |
|-----------|--------------|
| Cursor hook tests pass (8) | `cargo test test_cursor -- --ignored` |
| Gemini hook tests pass (8) | `cargo test test_gemini -- --ignored` |
| Hook execution tests pass (2) | `cargo test test_hook -- --ignored` |
| `discover --name` works | `cargo test discover_name` |
| `check` risk scoring correct | `cargo test check_risk` |
| Schema migration tests pass (2) | `cargo test test_schema_migration -- --ignored` |
| Module node tests pass (4+1) | `cargo test graph_correctness -- --ignored` |
| **Total:** ≤66 ignored | `cargo test --workspace` |
| 0 clippy warnings | `cargo clippy --workspace` |
| PROGRESS.md accurate | Manual review |

---

## 15. Round 11 Plan: UX Polish + Resolution Depth

**Approach:** 3-agent swarm. Focus on feedback polish and achievable resolution tests.

### From Feedback (claudecode_feedback.md v2)

| # | Item | Severity | Fix |
|---|------|----------|-----|
| P1 | `name` unreliable placement | MEDIUM | Better path-keyword extraction, module convention detection, low-confidence abort |
| P2 | `check` high-caller summary | LOW | Summarize 20+ callers as "N callers across M files", --verbose for full list |
| P3 | `discover --context N` | LOW | Parameterize context lines (currently fixed) |
| P4 | Deprecate `where` | LOW | Print hint to use `discover --name` instead |

### Achievable Resolution Tests (~15 from 68 ignored)

| Language | Tests | Feature | Complexity |
|----------|-------|---------|-----------|
| Parser | 1 | `supports_extension()` method on LanguageResolver | Trivial |
| Go | 3 | blank/dot/module-relative imports | LOW-MED |
| Python | 5 | `__all__` basic list parsing | MEDIUM |
| Rust | 3 | use alias, use self, glob use | MEDIUM |
| TypeScript | 1 | package.json exports field | LOW-MED |

### Swarm Assignment

| Agent | Tasks | Files |
|-------|-------|-------|
| **polish** | P1-P4 feedback items, check caller summary, discover --context N, deprecate where | name.rs, naming.rs, check.rs, discover.rs, output formatters |
| **resolution-a** | Go imports (3), Python __all__ (5), parser supports_extension (1) | go/mod.rs, python/mod.rs, python.scm, resolver trait |
| **resolution-b** | Rust use statements (3), TS package.json exports (1), plus any quick wins | rust_lang/mod.rs, typescript/mod.rs |

### Success Criteria
- `keel name` returns "no confident match" when score < 0.3
- `check` summarizes 20+ callers
- `discover --context 5` works
- 68 → ~53 ignored tests
- 0 failures, 0 clippy warnings

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
