# Keel Agent Swarm Playbook

```yaml
tags: [keel, implementation, agent-swarm, automation, agent-teams]
status: ready
agents: 15 (1 orchestrator + 3 leads + 11 teammates)
budget_estimate: "Claude Max plan ($200/month) — expect 2-4 months based on Anthropic's C compiler data"
estimated_runtime: "Phase 0: ~12-24 hours (single agent). Phases 1-3: ~2-4 weeks autonomous. Remaining 30-50%: 2-3 weeks human polish."
inspired_by: "Anthropic C Compiler Swarm (16-agent adaptation) + Claude Code Agent Teams"
```

> **What this document is**: A runnable playbook for implementing keel using 15 Claude Code agents organized as 3 nested agent teams + 1 AI orchestrator. Pre-flight checklist, infrastructure setup, test harness, phase-by-phase agent assignments, coordination patterns, and verification checklists.
>
> **What this document is NOT**: A design document. All design decisions are finalized in [[constitution|Constitution]] and 13 specs in [[keel-speckit/]]. This document assumes specs are locked.

---

## Playbook Structure

This playbook is split into focused documents. **Read them in order for first-time setup.** For ongoing reference, jump to the relevant file.

| File | Contents | Read When |
|------|----------|-----------|
| [[agent-swarm/README\|README.md]] (this file) | Overview, philosophy, risks, pre-flight checklist | First |
| [[agent-swarm/scope-limits\|scope-limits.md]] | **Agent scope limits, context management rules, lessons learned** | **Before ANY agent work** |
| [[agent-swarm/infrastructure\|infrastructure.md]] | tmux setup, git worktrees, sandbox, agent teams config | Setting up infrastructure |
| [[agent-swarm/phases\|phases.md]] | Contracts, Phase 0-4 deliverables, execution model, gate criteria | During each phase |
| [[agent-swarm/spawn-prompts\|spawn-prompts.md]] | Agent assignments, team architecture, all spawn prompts | Spawning agents |
| [[agent-swarm/operations\|operations.md]] | Ralph loop, cross-team coordination, escalation, audit trail, verification | During autonomous runs |

> **CRITICAL: Read [[agent-swarm/scope-limits|scope-limits.md]] before spawning any agents.** It contains hard limits that prevent context exhaustion — the #1 failure mode for agent swarms.

---

## 1. Philosophy & Honest Expectations

### Why Agent Swarm Works for Keel

1. **13 self-contained specs with unambiguous GIVEN/WHEN/THEN acceptance criteria** — agents have clear pass/fail signals
2. **Natural three-way split** along the dependency DAG: Foundation (parsing + graph) -> Enforcement (validation + commands) -> Surface (integration + distribution)
3. **Typed contracts everywhere** — Rust traits and structs define interfaces between agents
4. **Resolution engine parallelizes internally** — 4 language resolvers can be developed independently by separate teammates within the Foundation team
5. **~667 pre-written tests** provide continuous feedback signal
6. **15-agent architecture** matches Anthropic's proven C compiler swarm scale (16 agents, 2,000 sessions)

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
| 6 | **Agent spinning** — same test failure loops indefinitely | Wasted budget, zero progress | Error fingerprinting via `TeammateIdle` hooks: 5=hint, 8=force-skip, 15=cooldown (see [[design-principles#Principle 6|Principle 6]]) |
| 7 | **Inter-agent contract drift** — Foundation's types != Enforcement's expectations | Integration failures at phase gates | Frozen contracts in Phase 0. Contract tests on every cycle. |
| 8 | **Performance benchmarks fail on first pass** — <200ms compile not trivially achievable | Blocks M2 gate | Profile early. Use criterion benchmarks. Optimize hot paths (tree-sitter incremental, SQLite queries). |
| 9 | **Context exhaustion from agent results** — Task subagents flood parent context | Session dies, all work lost | Hard limits in [[agent-swarm/scope-limits\|scope-limits.md]]. Max 15 files per session, max 30 tool calls per Task agent. |

---

## 2. Pre-Flight Checklist

Complete ALL items before launching agents.

### Specs & Documents

- [ ] All 13 specs hardened with unambiguous acceptance criteria
  - [ ] [[keel-speckit/000-graph-schema/spec|000 Graph Schema]]
  - [ ] [[keel-speckit/001-treesitter-foundation/spec|001 Tree-sitter Foundation]]
  - [ ] [[keel-speckit/002-typescript-resolution/spec|002 TypeScript Resolution]]
  - [ ] [[keel-speckit/003-python-resolution/spec|003 Python Resolution]]
  - [ ] [[keel-speckit/004-go-resolution/spec|004 Go Resolution]]
  - [ ] [[keel-speckit/005-rust-resolution/spec|005 Rust Resolution]]
  - [ ] [[keel-speckit/006-enforcement-engine/spec|006 Enforcement Engine]]
  - [ ] [[keel-speckit/007-cli-commands/spec|007 CLI Commands]]
  - [ ] [[keel-speckit/008-output-formats/spec|008 Output Formats]]
  - [ ] [[keel-speckit/009-tool-integration/spec|009 Tool Integration]]
  - [ ] [[keel-speckit/010-mcp-http-server/spec|010 MCP/HTTP Server]]
  - [ ] [[keel-speckit/011-vscode-extension/spec|011 VS Code Extension]]
  - [ ] [[keel-speckit/012-distribution/spec|012 Distribution]]
- [ ] [[constitution|Constitution]] reviewed — all 10 articles satisfied by spec coverage
- [ ] [[design-principles|Design Principles]] reviewed — all 10 principles understood
- [ ] [[keel-speckit/test-harness/strategy|Test Harness Strategy]] reviewed — 4 oracles defined, corpus listed
- [ ] **[[agent-swarm/scope-limits|scope-limits.md]] read and understood** — context management rules prevent session crashes

### Test Corpus

- [ ] Test corpus repos cloned and pinned to specific commits
- [ ] Purpose-built test repo (#11) created with known cross-file references
- [ ] LSP ground truth data generated for all repos (TypeScript/tsserver, Python/pyright, Go/gopls, Rust/rust-analyzer)

### External Dependencies Verified

- [ ] `tree-sitter` crate with 4 language grammars compiles
- [ ] `oxc_resolver` + `oxc_semantic` crate compiles (v0.111+)
- [ ] `ty` CLI installed and `ty --output-format json` works on Python test corpus
- [ ] `ra_ap_ide` crate compiles (note: 0.0.x unstable API)
- [ ] `rusqlite` with `bundled` feature compiles

### Agent Teams Prerequisites

- [ ] Claude Code installed with experimental agent teams enabled
- [ ] Environment variable set: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- [ ] Claude Code `settings.json` configured (see [[agent-swarm/infrastructure#Agent Teams Configuration|infrastructure.md]])
- [ ] tmux installed and available
- [ ] `/ralph-loop` and `/tmux-observe` skills installed in Claude Code
- [ ] `bubblewrap` and `socat` installed (Linux: `sudo apt install bubblewrap socat`)
- [ ] Sandbox verified working: `claude --sandbox --print "echo hello"`

---

## Related Documents

- [[design-principles|Design Principles]] — the "why" document
- [[constitution|Constitution]] — non-negotiable articles
- [[keel-speckit/test-harness/strategy|Test Harness Strategy]] — oracle definitions and corpus
- [[CLAUDE|CLAUDE.md]] — agent implementation guide
- [[PRD_1|PRD v2.1]] — master source document (agents should NOT read this — use specs instead)
