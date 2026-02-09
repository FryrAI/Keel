# Agent Scope Limits & Context Management Rules

```yaml
tags: [keel, agent-swarm, scope-limits, context-management]
status: enforced
applies_to: ALL PHASES (Phase 0, Phase 1, Phase 2, Phase 3, dogfooding, and any future phase)
```

> **Read this file BEFORE spawning any agents, creating any files, or starting any phase.**
> These rules exist because Phase 0 was destroyed by context exhaustion on 2026-02-09.
> They are non-negotiable. Violating them will crash your session.

---

## 1. Hard Limits (ALL PHASES)

These limits apply to every agent — orchestrator, lead, teammate, Task subagent — in every phase.

| Limit | Value | Rationale |
|-------|-------|-----------|
| Max files created per agent/session | 15 | Beyond this, context fills with write confirmations |
| Max tool calls per Task subagent | 30 | 93 tool calls = context exhaustion |
| Max expected tokens per Task subagent | 60k | Agents beyond 60k are losing coherence |
| Max result summary length | 200 words | Prevents context flooding in parent |
| Max duration per Task subagent | 5 minutes | 12+ minutes = scope too large, decompose further |

### If Any Limit Would Be Exceeded

**STOP. Decompose into smaller units.** Do not attempt to "push through" — context exhaustion is not recoverable. Once the context window fills, the session is dead and all in-flight work is lost.

---

## 2. Anti-Patterns (Explicit Violations)

These are real mistakes that have destroyed sessions. Do not repeat them:

- **"Create all ~98 test files" as 1-2 agents** — split into 6-8 groups of 10-15 files
- **Returning full file listings in agent results** — return only file count + any errors
- **Using Task tool for work that should use separate tmux panes** — see [[#5. Task Agent vs. tmux Pane Decision Matrix|Section 5]]
- **Agents that do both "read specs" and "create files" in one invocation** — separate into read-only research agent + file creation agent
- **Spawning 6+ Task agents in one session without tracking cumulative context** — after 3 agents, check if total received > 50k tokens

---

## 3. Context Management Rules (ENFORCEABLE)

### Rule 1: NEVER Use Task Tool Subagents for Bulk File Creation

Task tool subagents return their full results into the parent session's context window.
Creating N files = N write confirmations + result summary, all injected into the parent.

**HARD LIMIT:** Task agents may create at most **5 files**. If you need to create more,
use a separate sandboxed Claude session in a tmux pane.

**Violation consequence:** Parent session hits context limit, becomes unresponsive,
loses all work in progress.

### Rule 2: Separate tmux Panes for Parallel Work

Every independent workstream that creates files MUST run in its own tmux pane with
its own sandboxed Claude session (`claude --sandbox --dangerously-skip-permissions`).

Communication between panes uses **git commits only** — never Task tool results.

```bash
# WRONG: spawning 6 Task agents in one session
Task("Create 81 test files")  # → 99k tokens floods parent context

# RIGHT: 4 tmux panes, each creating ~15-20 files
# Pane 0: creates tests/graph/ + tests/parsing/ + tests/enforcement/
# Pane 1: creates tests/resolution/ + tests/cli/ + tests/output/
# Pane 2: creates tests/server/ + tests/benchmarks/ + tests/integration/
# Pane 3: creates schemas + fixtures + contracts + scripts
```

### Rule 3: Max Scope Per Agent Session

| Metric | Hard Limit | Kill Signal |
|--------|-----------|-------------|
| Files created | 15 per session | If you're planning to create 16+, split the work |
| Tool calls | 30 per Task subagent | Beyond 30 = scope is too large |
| Token consumption | 60k per Task subagent | Beyond 60k = agent is losing coherence |
| Wall clock time | 5 minutes per Task subagent | Beyond 5 min = use tmux pane instead |
| Result summary | 200 words max | Long results = context flood in parent |

**If any limit would be exceeded: STOP. Decompose into smaller units.**

### Rule 4: Result Brevity

When a Task subagent or teammate completes, its result MUST be terse:

```
# WRONG (what happened on 2026-02-09):
"Created 81 files: tests/graph/test_node_creation.rs (10 tests),
tests/graph/test_edge_creation.rs (10 tests), [... 3 pages of listings ...]"

# RIGHT:
"Created 81 files across tests/graph/, tests/parsing/, tests/enforcement/,
tests/resolution/, tests/cli/, tests/output/. 480 tests total, all #[ignore].
Zero compilation errors."
```

**Rule: File listings go in git commits. Agent results carry only counts and status.**

### Rule 5: Context Budget Tracking

The orchestrator (or parent session) MUST track context consumption:

- After receiving 3+ Task agent results: check if total received > 50k tokens
- If yes: STOP spawning Task agents. Switch to tmux panes or sequential work.
- If a single agent result exceeds 30k tokens: that agent's scope was too large.
  Note this and decompose further next time.

**Context exhaustion is a system failure, not a recoverable error.** Once the context
window fills, the session is dead. There is no "retry" — you lose all in-flight work.

### Rule 6: Git Commits as Coordination Boundaries

When multiple sessions work in parallel (whether tmux panes or agent teams):

1. Each session commits its work to git when done
2. Other sessions pull to get the results
3. NO session reads another session's work via Task tool results or shared context

This provides:
- Natural checkpointing (work survives session crashes)
- Zero context overhead (git log is cheap, Task results are expensive)
- Clean post-mortem (git blame shows exactly what each session produced)

---

## 4. Spawn Prompt Context Rules Appendix

**Every spawn prompt in [[agent-swarm/spawn-prompts|spawn-prompts.md]] MUST include this block:**

```
MANDATORY: Read scope-limits.md (Section 2.5 + Section 19) before starting.
Max 15 files per session. Max 30 tool calls per Task subagent.
Git commits for coordination. Terse results only.
If you're about to exceed any limit: STOP and decompose.
```

**Every phase in [[agent-swarm/phases|phases.md]] is governed by these rules.**
Every session — orchestrator, lead, teammate — must respect scope limits.

---

## 5. Task Agent vs. tmux Pane Decision Matrix

| Scenario | Use Task Agent | Use tmux Pane |
|----------|:-:|:-:|
| Quick read-only research (< 5 files) | Yes | No |
| Create 1-5 files with known content | Yes | No |
| Create 6-15 files | No | Yes |
| Create 16+ files | No | Yes (split across multiple panes) |
| Work lasting > 5 minutes | No | Yes |
| Work requiring > 30 tool calls | No | Yes |
| Work that needs to git commit results | No | Yes |
| Parallel independent workstreams | No | Yes (one pane per stream) |

**Rule: Task agents are for small, bounded, read-heavy work. File creation at scale uses separate sandboxed sessions.**

---

## 6. Ralph Loop & Task Subagent Warning

If `/ralph-loop` spawns Task subagents that create files, each subagent is subject to these scope limits (max 5 files, max 30 tool calls). If the fix requires creating many files, do it directly in the current session, not via Task subagent.

---

## 7. Incident Reference: 2026-02-09 Context Blowup

### What Happened

Phase 0 scaffolding used Task tool subagents in a single session instead of separate tmux panes. Six Task subagents created ~150 files. Three agents consumed 60-101k tokens each. All results flooded the parent context. Session died after 6 consecutive context compacts.

### Incident Data

| Agent | Tokens | Tool Calls | Files | Duration |
|-------|--------|-----------|-------|----------|
| Test files group 1 | 99k | 93 | 81 | 12+ min |
| Schemas/fixtures | 101k | 59 | ~20 | 8+ min |
| Test files group 2 | 61k | 46 | 35 | 5+ min |
| Parsers | 46k | 28 | 6 | 2.5 min |
| Enforce/cli/output/server | 45k | 20 | 8 | 1.7 min |
| Read specs | 54k | 3 | 0 | 0.5 min |

The bottom 3 agents were fine. The top 3 violated every rule above.

### Root Cause

No scope limits existed. The playbook said "one agent" but didn't define how to decompose 20 deliverables safely within context constraints.

### Fix

This document (scope-limits.md) — hard limits on files, tool calls, tokens, and duration per agent. Task agents for small work only; tmux panes for bulk creation. Git commits for coordination, not Task results.

---

## 8. Lessons Learned

### 2026-02-09: Phase 0 Context Blowup

Phase 0 scaffolding used Task tool subagents in a single session instead of separate tmux panes. Three agents exceeded 60k tokens each, flooding the parent context window. The session became unresponsive after 6 consecutive context compacts.

**Root cause:** No scope limits existed. The playbook said "one agent" but didn't define how to decompose 20 deliverables safely within context constraints.

**Fix:** Sections 1-6 of this document — hard limits on files, tool calls, tokens, and duration per agent. Task agents for small work only; tmux panes for bulk creation. Git commits for coordination, not Task results.

### Key Takeaways

1. **Context exhaustion is catastrophic, not graceful.** There is no warning, no retry, no recovery. The session simply dies.
2. **Task tool subagents are convenient but dangerous.** Their results inject directly into the parent's context. 3 large agents = dead session.
3. **Git is the right coordination mechanism.** Commits are free (context-wise), persistent (survive crashes), and auditable (blame shows everything).
4. **Monolithic playbooks are themselves a context risk.** This playbook was split into 6 files because a 1600+ line document consumes too much agent context.
