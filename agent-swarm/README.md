# Keel Agent Swarm Playbook

```yaml
tags: [keel, implementation, agent-swarm, automation, agent-teams]
status: completed
agents_planned: 15 (1 orchestrator + 3 leads + 11 teammates)
agents_actual: 3 worktrees with single agents + 1 orchestrator session (~2 days)
budget_estimate: "Claude Max plan ($200/month) — expect 2-4 months based on Anthropic's C compiler data"
actual_runtime: "~2 days (2026-02-09 to 2026-02-10). 442 tests, 0 failures."
inspired_by: "Anthropic C Compiler Swarm (16-agent adaptation) + Claude Code Agent Teams"
```

> **What this document is**: A runnable playbook for implementing keel using 15 Claude Code agents organized as 3 nested agent teams + 1 AI orchestrator. Pre-flight checklist, infrastructure setup, test harness, phase-by-phase agent assignments, coordination patterns, and verification checklists.
>
> **What this document is NOT**: A design document. All design decisions are finalized in [Constitution](../constitution.md) and 13 specs in `keel-speckit/`. This document assumes specs are locked.

---

## Playbook Structure

This playbook is split into focused documents. **Read them in order for first-time setup.** For ongoing reference, jump to the relevant file.

| File | Contents | Read When |
|------|----------|-----------|
| [README.md](README.md) (this file) | Overview, philosophy, risks, pre-flight checklist | First |
| [scope-limits.md](scope-limits.md) | **Agent scope limits, context management rules, lessons learned** | **Before ANY agent work** |
| [infrastructure.md](infrastructure.md) | tmux setup, git worktrees, sandbox, agent teams config | Setting up infrastructure |
| [phases.md](phases.md) | Contracts, Phase 0-4 deliverables, execution model, gate criteria | During each phase |
| [spawn-prompts.md](spawn-prompts.md) | Agent assignments, team architecture, all spawn prompts | Spawning agents |
| [operations.md](operations.md) | Ralph loop, cross-team coordination, escalation, audit trail, verification | During autonomous runs |

> **CRITICAL: Read [scope-limits.md](scope-limits.md) before spawning any agents.** It contains hard limits that prevent context exhaustion — the #1 failure mode for agent swarms.

---

## 1. Philosophy & Honest Expectations

### Why Agent Swarm Works for Keel

1. **13 self-contained specs with unambiguous GIVEN/WHEN/THEN acceptance criteria** — agents have clear pass/fail signals
2. **Natural three-way split** along the dependency DAG: Foundation (parsing + graph) -> Enforcement (validation + commands) -> Surface (integration + distribution)
3. **Typed contracts everywhere** — Rust traits and structs define interfaces between agents
4. **Resolution engine parallelizes internally** — 4 language resolvers can be developed independently by separate teammates within the Foundation team
5. **Pre-written tests with `#[ignore]`** provide continuous feedback signal (442 passing at completion)
6. **Worktree-based parallelism** with separate Claude sessions proved more effective than the planned 15-agent nested team architecture

### "One Shot" — What It Actually Means

**One shot does NOT mean perfection on first try.** It means agents handle the grind while you handle the judgment calls.

Based on Anthropic's C compiler data (16 agents, 2,000 sessions) and KolBaer's experience (3 agents, 50-70% first pass):

- **50-70% of acceptance criteria passing** after autonomous run. keel is compiler-adjacent — harder than CRUD apps, similar to Anthropic's C compiler challenge.
- Each agent runs 100-500 autonomous sessions (test -> fix -> test -> repeat via `/ralph-loop`)
- **The remaining 30-50% needs 2-3 weeks of focused human development**: resolution edge cases, false positive tuning, cross-tool integration quirks, performance optimization.

The goal: collapse 8-12 weeks of development into 2-4 weeks autonomous + 2-3 weeks human refinement.

### This Is Harder Than KolBaer

KolBaer was web app CRUD with frontend/backend split. Keel is compiler-adjacent infrastructure with a 4-language resolution engine. The C compiler comparison is apt.

**The resolution engine IS the risk.** 4 languages x different enhancers x 3 tiers of fallback. PRD estimates 10-14 days for M1 alone. Accept this — front-load the test harness so agents can iterate autonomously.

### Critical Risks & Mitigations

| # | Risk | Impact | Mitigation |
|---|------|--------|------------|
| 1 | **Resolution engine complexity** — 4 languages, each with different Tier 2 enhancer | Long M1 phase, possible <85% precision | Per-language resolvers parallelize as separate teammates within Foundation team. Gate on precision before advancing. |
| 2 | **Oxc API surface** — `oxc_semantic` is per-file only | Cross-file stitching needed | Use tree-sitter queries (Spec 001) for cross-file, Oxc for per-file enhancement |
| 3 | **ty (Python) is beta** — v0.0.15, API not stable | Subprocess may change behavior | Use as subprocess only (not library). Fallback to tree-sitter heuristics + Pyright LSP. |
| 4 | **rust-analyzer lazy-load** — 60s+ startup | Performance impact on Rust projects | Lazy-loaded, not always-on. Only triggered when tree-sitter heuristics fail. |
| 5 | **tree-sitter grammar versions** — may differ from installed | Parse failures on edge cases | Pin grammar versions in Cargo.toml. Test against corpus. |
| 6 | **Agent spinning** — same test failure loops indefinitely | Wasted budget, zero progress | Error fingerprinting via `TeammateIdle` hooks: 5=hint, 8=force-skip, 15=cooldown (see [Design Principles](../design-principles.md)) |
| 7 | **Inter-agent contract drift** — Foundation's types != Enforcement's expectations | Integration failures at phase gates | Frozen contracts in Phase 0. Contract tests on every cycle. |
| 8 | **Performance benchmarks fail on first pass** — <200ms compile not trivially achievable | Blocks M2 gate | Profile early. Use criterion benchmarks. Optimize hot paths (tree-sitter incremental, SQLite queries). |
| 9 | **Context exhaustion from agent results** — Task subagents flood parent context | Session dies, all work lost | Hard limits in [scope-limits.md](scope-limits.md). Max 15 files per session, max 30 tool calls per Task agent. |

---

## 2. Pre-Flight Checklist

Complete ALL items before launching agents.

### Specs & Documents

- [x] All 13 specs hardened with unambiguous acceptance criteria
  - [x] [000 Graph Schema](../keel-speckit/000-graph-schema/spec.md)
  - [x] [001 Tree-sitter Foundation](../keel-speckit/001-treesitter-foundation/spec.md)
  - [x] [002 TypeScript Resolution](../keel-speckit/002-typescript-resolution/spec.md)
  - [x] [003 Python Resolution](../keel-speckit/003-python-resolution/spec.md)
  - [x] [004 Go Resolution](../keel-speckit/004-go-resolution/spec.md)
  - [x] [005 Rust Resolution](../keel-speckit/005-rust-resolution/spec.md)
  - [x] [006 Enforcement Engine](../keel-speckit/006-enforcement-engine/spec.md)
  - [x] [007 CLI Commands](../keel-speckit/007-cli-commands/spec.md)
  - [x] [008 Output Formats](../keel-speckit/008-output-formats/spec.md)
  - [x] [009 Tool Integration](../keel-speckit/009-tool-integration/spec.md)
  - [x] [010 MCP/HTTP Server](../keel-speckit/010-mcp-http-server/spec.md)
  - [x] [011 VS Code Extension](../keel-speckit/011-vscode-extension/spec.md)
  - [x] [012 Distribution](../keel-speckit/012-distribution/spec.md)
- [x] [Constitution](../constitution.md) reviewed — all 10 articles satisfied by spec coverage
- [x] [Design Principles](../design-principles.md) reviewed — all 10 principles understood
- [x] [Test Harness Strategy](../keel-speckit/test-harness/strategy.md) reviewed — 4 oracles defined, corpus listed
- [x] **[scope-limits.md](scope-limits.md) read and understood** — context management rules prevent session crashes

### Test Corpus

- N/A Test corpus repos cloned and pinned to specific commits — **not needed; unit/integration tests sufficed**
- N/A Purpose-built test repo (#11) — **not needed; inline test fixtures covered all cases**
- N/A LSP ground truth data — **not needed; 153 resolver tests validated precision directly**

### External Dependencies Verified

- [x] `tree-sitter` crate with 4 language grammars compiles
- [x] `oxc_resolver` + `oxc_semantic` crate compiles (v0.111+)
- N/A `ty` CLI — Python resolver used heuristic approach instead
- N/A `ra_ap_ide` crate — Rust resolver used heuristic approach instead
- [x] `rusqlite` with `bundled` feature compiles

### Agent Teams Prerequisites

- [x] Claude Code installed with experimental agent teams enabled
- [x] Environment variable set: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- [x] Claude Code `settings.json` configured (see [infrastructure.md](infrastructure.md))
- [x] tmux installed and available
- [x] `/ralph-loop` and `/tmux-observe` skills installed in Claude Code
- [x] `bubblewrap` and `socat` installed (Linux: `sudo apt install bubblewrap socat`)
- N/A Sandbox verified via `claude --sandbox` — **no `--sandbox` CLI flag; configured via settings.json**

---

## 3. Retrospective (2026-02-10)

All phases completed 2026-02-09 to 2026-02-10. Final result: **442 tests, 0 failures, 0 clippy warnings.**

### Plan vs Reality

| Aspect | Planned | Actual |
|--------|---------|--------|
| Agents | 15 (1 orchestrator + 3 leads + 11 teammates) | 3 worktrees with single agents + 1 human-orchestrated |
| Duration | 2-4 weeks autonomous + 2-3 weeks human polish | ~2 days |
| Resolver tests | ~71 of 104 enabled via heavy work | 101/104 already passed; 1-line fix enabled all 104 |
| Test count | ~667 pre-written | 442 passing (many were consolidated/merged) |
| Test corpus | 11 real repos with LSP ground truth | Unit/integration tests only — no external repos needed |
| Gate markers | `.keel-swarm/gate-m1-passed` files | Not created — progress tracked via PROGRESS.md + git log |
| Audit trail | JSONL logs, error fingerprinting | Not deployed — git history was sufficient |

### What Worked

1. **Git worktrees** — true isolation between parallel agents, commits for coordination
2. **Scope limits** (`scope-limits.md`) — prevented the context exhaustion that killed Phase 0's first attempt
3. **Pre-written tests with `#[ignore]`** — agents had clear pass/fail signals from day one
4. **Crate-based ownership** — natural file isolation prevented merge conflicts
5. **Frozen contracts (Phase 0)** — teams could work independently against stable interfaces

### What Was Overkill

1. **15-agent nested team architecture** — 3 single-agent worktrees were faster and simpler
2. **Gate marker files** — manual git log inspection was sufficient
3. **JSONL audit trail and error fingerprinting** — never deployed; git blame was enough
4. **Test corpus of 11 real repos** — inline test fixtures covered all validation needs
5. **Swarm status dashboard** — PROGRESS.md served the same purpose with less overhead

### Key Insight

> "Test your assumptions before building infrastructure. 101 of 104 resolver tests already passed before any agent touched resolver code. The 1-line fix that enabled the remaining 3 was `pub(` prefix detection in `rust_is_public()`."

---

## 4. CI Swarm — Round 2 (2026-02-12)

### What Changed Since Retrospective
- Entry points wired for all 8 orphaned test directories (Round 1: ci/test-infra)
- Shared test helpers created in `tests/common/mod.rs`
- 168 integration tests implemented with real assertions (Round 1: ci/enforcement)
- 7 bugs fixed (Round 1: ci/bugs)
- Test count: 467 → 478 passing, 0 → 318 wired-but-ignored stubs

### Round 2 Architecture
- Same 3-worktree model (ci/test-infra, ci/enforcement, ci/bugs)
- Prompts rewritten in `scripts/ci-prompts/` for remaining work
- Each agent runs `/ralph-loop` autonomously
- 15-repo corpus cloned at `/tmp/claude/test-corpus`
- Orchestrator monitors via git log + /tmux-observe from pane 0

### Targets

| Agent | Stubs | Priority |
|-------|-------|----------|
| test-infra | 179 (graph 70 + parsing 59 + graph_correctness 50) + 8 resolution | P0 |
| enforcement | 102 (cli 53 + tool_integration 49) + 31 benchmarks | P0 |
| bugs | O(n^2) perf (62s → <200ms) + corpus validation + 6 integration | P0 |

---

## Related Documents

- [Design Principles](../design-principles.md) — the "why" document
- [Constitution](../constitution.md) — non-negotiable articles
- [Test Harness Strategy](../keel-speckit/test-harness/strategy.md) — oracle definitions and corpus
- [CLAUDE.md](../CLAUDE.md) — agent implementation guide
- [PRD v2.1](../docs/research/PRD_1.md) — master source document (agents should NOT read this — use specs instead)
