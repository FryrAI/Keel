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

## 5. Monitoring (Orchestrator — Pane 0)

### Passive Monitoring

```bash
# Check what agents are doing
/tmux-observe

# Check worktree progress via git
git -C $HOME/keel-ci-test-infra log --oneline -5
git -C $HOME/keel-ci-enforcement log --oneline -5
git -C $HOME/keel-ci-bugs log --oneline -5
```

### Active Gate Checks

Run periodically (or let `/ralph-loop` do it):

```bash
# Gate 1: Do all stubs compile?
git -C $HOME/keel-ci-test-infra fetch origin
cd $HOME/keel-ci-test-infra && cargo test --workspace --no-run 2>&1 | tail -3

# Gate 2: Do enforcement tests pass?
cd $HOME/keel-ci-enforcement && cargo test --workspace 2>&1 | tail -5

# Gate 3: Zero failures across everything?
cd $HOME/keel-ci-bugs && cargo test --workspace 2>&1 | tail -5
```

### When to Intervene

- Agent stuck on same error for 15+ minutes → read error, give hint
- Merge conflict between branches → resolve manually
- Agent requests human judgment → check pane output
- Context exhaustion (agent stops responding) → restart pane

---

## 6. Merge & Verify

### Merge Order (Critical)

Merge in dependency order. Test-infra first (schema/helpers), then enforcement
(uses helpers), then bugs (uses everything).

```bash
cd $HOME/Curosor_Projects/Keel

# Step 1: Merge test infrastructure
git merge ci/test-infra --no-edit
cargo test --workspace 2>&1 | tail -10
# If failures: fix before proceeding

# Step 2: Merge enforcement tests
git merge ci/enforcement --no-edit
cargo test --workspace 2>&1 | tail -10
# If failures: fix before proceeding

# Step 3: Merge bug fixes
git merge ci/bugs --no-edit
cargo test --workspace 2>&1 | tail -10
# Must be zero failures
```

### Post-Merge Validation

```bash
# Full test suite (all tests, including previously-ignored)
cargo test --workspace -- --include-ignored 2>&1 | tail -20

# 15-repo corpus validation
./scripts/validate_corpus.sh 2>&1 | tail -30

# Check for files over 400 lines
find crates/ tests/ -name '*.rs' | xargs wc -l | sort -rn | head -20
# Any file > 400 lines must be decomposed
```

---

## 7. Convergence Criteria

The playbook is **done** when ALL of these are true:

| Criterion | How to verify | Target |
|-----------|--------------|--------|
| Zero test failures | `cargo test --workspace` | 0 failures |
| Zero ignored tests | `grep -r '#\[ignore' tests/ crates/ \| wc -l` | 0 (or only perf benchmarks) |
| All stubs implemented | No empty `fn test_foo() {}` bodies | 0 empty stubs |
| 15-repo validation | `./scripts/validate_corpus.sh` | All 15 green |
| Deterministic | Run validation 3x, same results | 3 consecutive identical |
| No files > 400 lines | `find . -name '*.rs' \| xargs wc -l \| awk '$1>400'` | 0 matches |
| Clippy clean | `cargo clippy --workspace -- -D warnings` | 0 warnings |
| LLM output depth | `keel compile --depth 0/1/2` on test fixtures | Correct output at each level |
| Backpressure signals | Verify `PRESSURE=` and `BUDGET=` in LLM output | Present when violations exist |

### Partial Convergence

If you can't reach full convergence in one session:
1. Merge what's passing
2. Update `PROGRESS.md` with new counts
3. Commit to main
4. Re-run this playbook — it's idempotent

---

## 8. Cleanup

After convergence (or when stopping for the day):

```bash
# Remove worktrees
git worktree remove $HOME/keel-ci-test-infra --force 2>/dev/null
git worktree remove $HOME/keel-ci-enforcement --force 2>/dev/null
git worktree remove $HOME/keel-ci-bugs --force 2>/dev/null

# Delete branches (only after merge to main)
git branch -d ci/test-infra ci/enforcement ci/bugs 2>/dev/null

# Kill tmux session
tmux kill-session -t keel-ci 2>/dev/null

# Prune worktree metadata
git worktree prune
```

---

## 9. Round 4 Results (2026-02-13) — COMPLETED

Features shipped in Round 4, from the candidates list:

| Priority | Feature | Description | Status |
|----------|---------|-------------|--------|
| **P0** | `keel fix --apply` | Auto-apply fix plans with file writes + re-compile verification | **DONE** |
| **P1** | Streaming compile | `--watch` mode for continuous agent loops | Deferred → Round 5 |
| **P2** | `--max-tokens N` | Configurable global token budget for LLM output (replaces hardcoded 500) | **DONE** |
| **P3** | `keel explain --depth 0-3` | Resolution chain truncation by depth level | **DONE** |
| **P4** | Map diff | `--since HASH` for structural delta (only show what changed) | Deferred → Round 5 |

- Tests: 926 → 931 (+5 new tests)
- 33 files changed, +2822/-499 lines
- Single-session approach (same as Round 3)

### Round 5 Candidates
| Priority | Feature | Description |
|----------|---------|-------------|
| **P1** | Streaming compile | `--watch` mode for continuous agent loops |
| **P2** | Map diff | `--since HASH` for structural delta (only show what changed) |
| **P3** | Backpressure threshold tuning | Calibrate PRESSURE/BUDGET based on real agent behavior |
| **P4** | Token budget calibration | Validate --max-tokens against actual LLM context windows |

---

## 10. Next Steps (Post-Convergence Only)

Once all convergence criteria are met:

1. **Tag release:** `git tag v0.1.0 && git push --tags`
2. **Build binaries:** `cargo build --release --target x86_64-unknown-linux-gnu` (+ macOS, Windows)
3. **Publish:** GitHub release with binaries + install script
4. **VS Code extension:** Package and publish to marketplace
5. **Documentation:** Update README with installation and usage
6. **Announce:** Post to relevant communities

**Do not start these until convergence = true.** Shipping with known test gaps
creates support burden that exceeds the cost of fixing them first.

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
