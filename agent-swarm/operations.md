# Operations: Ralph Loop, Coordination, Escalation & Audit

```yaml
tags: [keel, agent-swarm, operations, coordination, escalation, audit]
status: ready
```

> **All operations are governed by [[agent-swarm/scope-limits|scope-limits.md]].**
> Every session — orchestrator, lead, teammate — must respect scope limits.

---

## 1. Ralph Loop

Each participant in the swarm runs `/ralph-loop` — Claude Code's autonomous test-fix-test skill. No custom loop scripts needed.

### How `/ralph-loop` Works

`/ralph-loop` is a Claude Code skill that puts the agent into a continuous cycle:
1. Run tests for the agent's scope
2. Analyze failures
3. Fix the code
4. Run tests again
5. Repeat until tests pass or escalation triggers

**WARNING:** If `/ralph-loop` spawns Task subagents that create files, each subagent is subject to [[agent-swarm/scope-limits|scope limits]] (max 5 files, max 30 tool calls). If the fix requires creating many files, do it directly in the current session, not via Task subagent.

### Who Runs `/ralph-loop`

| Agent | Scope | Test Command |
|-------|-------|-------------|
| Orchestrator | Cross-team gate checks | `./scripts/test-full.sh` across worktrees |
| Lead A | Foundation team progress | `cargo test -p keel-core -p keel-parsers` |
| Lead B | Enforcement team progress | `cargo test -p keel-enforce -p keel-cli -p keel-output` |
| Lead C | Surface team progress | `cargo test -p keel-server && cd extensions/vscode && npm test` |
| ts-resolver | TypeScript resolver | `cargo test -p keel-parsers -- typescript` |
| py-resolver | Python resolver | `cargo test -p keel-parsers -- python` |
| go-resolver | Go resolver | `cargo test -p keel-parsers -- go` |
| rust-resolver | Rust resolver | `cargo test -p keel-parsers -- rust` |
| enforcement-engine | Enforcement logic | `cargo test -p keel-enforce` |
| cli-commands | CLI commands | `cargo test -p keel-cli` |
| output-formats | Output formatters | `cargo test -p keel-output` |
| tool-integration | Tool configs | `cargo test -- tool_integration` |
| mcp-server | MCP/HTTP server | `cargo test -p keel-server` |
| vscode-ext | VS Code extension | `cd extensions/vscode && npm test` |
| distribution | Build + distribution | `cargo build --release` |

### Leads vs Teammates

- **Teammates** run `/ralph-loop` to autonomously fix code within their crate scope
- **Leads** run `/ralph-loop` to monitor teammate progress, redistribute work, and escalate blockers
- **Orchestrator** runs `/ralph-loop` to monitor all 3 teams, enforce gates, and coordinate cross-team integration

---

## 2. Cross-Team Coordination

Since agent teams can't message across teams (one team per Claude Code session), coordination uses filesystem and git.

### Git Push/Pull

Each worktree pushes results to its branch. Other worktrees pull when needed.

```bash
# From orchestrator (root worktree), check all branches:
git fetch --all
git log --oneline foundation..HEAD
git log --oneline enforcement..HEAD
git log --oneline surface..HEAD
```

### Gate Marker Files

The orchestrator writes gate markers to the shared `.keel-swarm/` directory:

```
.keel-swarm/
+-- gate-m1-passed          # Written when M1 criteria verified
+-- gate-m2-passed          # Written when M2 criteria verified
+-- gate-m3-passed          # Written when M3 criteria verified
+-- status.md               # Current swarm status dashboard
```

**Marker file format:**
```
gate: M1
passed: 2026-03-15T14:30:00Z
criteria:
  ts_precision: 87%
  py_precision: 84%
  go_precision: 91%
  rust_precision: 78%
  no_panics: true
  init_time: 8.2s
  graph_delta: 6%
```

### Cross-Team Integration Merge

When a gate passes, the orchestrator merges the upstream branch into the downstream worktree:

```bash
# M1 gate passed — give Enforcement the real graph
cd worktree-b
git merge foundation --no-edit

# M2 gate passed — give Surface the real compile output
cd worktree-c
git merge enforcement --no-edit
```

### Shared Test Results

Each team writes oracle results to `results/` in the repo:

```
results/
+-- foundation/
|   +-- lsp-ground-truth-ts.json
|   +-- lsp-ground-truth-py.json
|   +-- lsp-ground-truth-go.json
|   +-- lsp-ground-truth-rust.json
+-- enforcement/
|   +-- mutation-test-results.json
|   +-- performance-benchmarks.json
+-- surface/
    +-- e2e-claude-code.json
    +-- e2e-cursor.json
```

---

## 3. Error Fingerprinting & Escalation

Error fingerprinting prevents agent spinning. Within each team, `TeammateIdle` hooks implement escalation natively. The orchestrator tracks cross-team patterns via `/tmux-observe`.

### Intra-Team Escalation (via `TeammateIdle` hooks)

**Escalation thresholds** (from [[design-principles#Principle 6|Principle 6]]):
- **5 consecutive failures:** Lead sends teammate a targeted hint
- **8 consecutive failures:** Lead reassigns the task or handles it via subagent
- **15 consecutive failures:** Lead flags to orchestrator via `ESCALATE:` commit message

### Cross-Team Escalation (via orchestrator)

The orchestrator uses `/tmux-observe` to detect:
- Teams stuck on the same error across multiple cycles
- Gate criteria not progressing
- Build failures affecting multiple teams

### Context Budget Tracking

The orchestrator (or parent session) MUST track context consumption:

- After receiving 3+ Task agent results: check if total received > 50k tokens
- If yes: STOP spawning Task agents. Switch to tmux panes or sequential work.
- If a single agent result exceeds 30k tokens: that agent's scope was too large.
- **Context exhaustion is a system failure, not a recoverable error.** See [[agent-swarm/scope-limits|scope-limits.md]].

### Error Fingerprint Format

```
Error fingerprint: hash(test_name + error_pattern + file_path)
- Groups identical failures
- Allows different manifestations of same root cause to escalate together
- Reset on new error: if the fix changes the error, counter resets
- Only identical consecutive failures escalate
```

---

## 4. Orchestrator Design

### Orchestrator CLAUDE.md

Place this in the root worktree's CLAUDE.md:

```markdown
# Keel Orchestrator — Cross-Team Coordinator

You are the orchestrator for the keel agent swarm. You are NOT part of any team.
You monitor 3 teams across 3 worktrees and enforce phase gates.

## Your Tools
- /tmux-observe — read output from panes 1-3
- /ralph-loop — continuous monitoring cycle
- git operations — check test results, merge branches at gates

## Your Responsibilities
1. Monitor all 3 teams via /tmux-observe
2. Check test results: pull each branch, run test scripts, compare against gate criteria
3. Write gate markers when criteria pass
4. Merge branches at gate transitions
5. Detect cross-team patterns
6. Write swarm-status.md
7. Flag human review when 15-repeat escalation fires

## Gate Check Procedure
1. git fetch --all
2. For each worktree: checkout branch, run test scripts, parse results
3. Compare results against gate criteria
4. If ALL criteria pass: write gate marker, perform cross-team merge
5. If criteria don't pass: log which criteria are failing

## You Do NOT
- Write Rust code
- Modify Cargo.toml
- Edit test files
- Push to any worktree branch
- Make architectural decisions

MANDATORY: Read scope-limits.md before starting.
Max 15 files per session. Max 30 tool calls per Task subagent.
Git commits for coordination. Terse results only.
```

### Swarm Status Dashboard

The orchestrator maintains `swarm-status.md` in the root worktree:

```markdown
# Keel Swarm Status
Updated: [timestamp]

## Current Phase: 1

## Teams
| Team | Lead | Teammates | Active Tasks | Completed Tasks |
|------|------|-----------|-------------|----------------|
| Foundation | Lead A | ts/py/go/rust-resolver | 4 | 2 |
| Enforcement | Lead B | engine/cli/output | 3 | 1 |
| Surface | Lead C | tools/mcp/vscode/dist | 2 | 2 |

## Gate Progress
| Gate | Status | Blocking Criteria |
|------|--------|-------------------|
| M1 | IN_PROGRESS | TS: 81% (need 85%), Rust: 72% (need 75%) |
| M2 | WAITING | Blocked by M1 |
| M3 | WAITING | Blocked by M2 |

## Escalations
- [none]
```

---

## 5. Agent Audit Trail

Every agent leaves a structured trace for post-mortem analysis.

### Log Directory Structure

```
.keel-swarm/logs/
+-- agents/                    # Per-agent JSONL audit logs
|   +-- ts-resolver.jsonl
|   +-- py-resolver.jsonl
|   +-- (etc.)
+-- escalations/               # Escalation events
|   +-- YYYY-MM-DD.jsonl
+-- gates/                     # Gate check attempts and results
|   +-- YYYY-MM-DD.jsonl
+-- aggregated/                # Orchestrator-produced summaries
    +-- daily-YYYY-MM-DD.json
```

### Hook-Driven Logging (Automatic)

**`.claude/hooks.json` — audit hooks (add to each worktree):**

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"pre_tool\",\"agent\":\"'$CLAUDE_AGENT_NAME'\",\"tool\":\"'$TOOL_NAME'\"}' >> .keel-swarm/logs/agents/${CLAUDE_AGENT_NAME:-unknown}.jsonl",
        "description": "Audit log: pre-tool breadcrumb"
      }
    ],
    "PostToolUse": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"post_tool\",\"agent\":\"'$CLAUDE_AGENT_NAME'\",\"tool\":\"'$TOOL_NAME'\",\"exit_code\":'${EXIT_CODE:-0}'}' >> .keel-swarm/logs/agents/${CLAUDE_AGENT_NAME:-unknown}.jsonl",
        "description": "Audit log: post-tool result"
      }
    ],
    "TeammateIdle": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"teammate_idle\",\"agent\":\"'$TEAMMATE_NAME'\",\"reason\":\"'$IDLE_REASON'\"}' >> .keel-swarm/logs/agents/${TEAMMATE_NAME:-unknown}.jsonl",
        "description": "Audit log: teammate idle event"
      }
    ]
  }
}
```

### Git-Driven Logging (Convention)

Commit message format:
```
[agent-name][spec-NNN] action: description

Examples:
[ts-resolver][spec-002] feat: implement Oxc barrel file resolution
[orchestrator][gate] pass: M1 all criteria met
[py-resolver][spec-003] ESCALATE: ty subprocess returns invalid JSON
```

### Post-Mortem Analysis

```bash
# What was agent X doing in the last hour?
jq 'select(.ts > "2026-03-15T17:00:00Z")' .keel-swarm/logs/agents/ts-resolver.jsonl

# Find all escalation events
grep '"ESCALATE"' .keel-swarm/logs/agents/*.jsonl

# Git log filtered by agent
git log --oneline --grep='\[ts-resolver\]'

# Count tool calls per agent
wc -l .keel-swarm/logs/agents/*.jsonl | sort -rn
```

### Log Rotation

At each phase gate:
1. Orchestrator compresses current logs: `gzip .keel-swarm/logs/agents/*.jsonl`
2. Archives to `.keel-swarm/logs/archive/phase-N/`
3. Fresh JSONL files start for next phase

---

## 6. Verification Checklist (Post-Build)

After all phases complete, verify:

- [ ] Every spec's acceptance criteria passes
- [ ] Resolution precision >85% per language (Gate M1)
- [ ] Mutation testing >95% true positive, <5% false positive (Gate M2)
- [ ] All performance benchmarks pass (Gate M2)
- [ ] JSON schemas validate on all outputs (Gate M2)
- [ ] E2E with Claude Code + Cursor works (Gate M3)
- [ ] Cross-platform builds succeed (Gate M3)
- [ ] `keel init` on real repos generates correct multi-tool configs
- [ ] `keel deinit` cleanly removes all generated files
- [ ] Binary size <40MB
- [ ] No runtime dependencies (single binary works on fresh system)
