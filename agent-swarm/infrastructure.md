# Infrastructure Setup

```yaml
tags: [keel, agent-swarm, infrastructure, tmux, sandbox]
status: completed
note: "Some planned infrastructure (test-corpus, gate markers, hooks) was never deployed. See inline notes."
```

> **Prerequisites:** Complete the [pre-flight checklist](README.md) before running any setup.
> **Scope limits:** Read [scope-limits.md](scope-limits.md) before spawning any agents.

---

## 1. Architecture Overview

```
tmux session "keel-swarm" (4 panes)

Pane 0: ORCHESTRATOR (Claude Code session, root worktree)
  - AI-driven, runs /ralph-loop
  - Uses /tmux-observe to monitor panes 1-3
  - Checks test results via git across worktrees
  - Enforces phase gates (creates gate marker files)
  - No agent team — standalone session focused on coordination

Pane 1: FOUNDATION (Claude Code + agent team "keel-foundation")
  Lead A (delegate mode) — coordinates Foundation work
  +-- Teammate: ts-resolver     — Spec 002 (TypeScript/Oxc)
  +-- Teammate: py-resolver     — Spec 003 (Python/ty)
  +-- Teammate: go-resolver     — Spec 004 (Go/heuristics)
  +-- Teammate: rust-resolver   — Spec 005 (Rust/rust-analyzer)
  (Lead A handles Specs 000, 001 via subagents or directly)

Pane 2: ENFORCEMENT (Claude Code + agent team "keel-enforcement")
  Lead B (delegate mode) — coordinates Enforcement work
  +-- Teammate: enforcement-engine — Spec 006
  +-- Teammate: cli-commands      — Spec 007
  +-- Teammate: output-formats    — Spec 008

Pane 3: SURFACE (Claude Code + agent team "keel-surface")
  Lead C (delegate mode) — coordinates Surface work
  +-- Teammate: tool-integration — Spec 009
  +-- Teammate: mcp-server       — Spec 010
  +-- Teammate: vscode-ext       — Spec 011
  +-- Teammate: distribution     — Spec 012
```

**Planned: 1 orchestrator + 3 leads + 11 teammates = 15 agents.** Actual: 3 worktrees with single agents + 1 human-orchestrated session. See [Retrospective](README.md).

### Human Role: Manage Only the Orchestrator

The human interacts ONLY with the orchestrator (pane 0). Everything else is autonomous:
- Human launches Phase 0 (uses tmux panes, not agent teams — see Section 3 below)
- Human starts the tmux session and kicks off the orchestrator
- Orchestrator manages teams, enforces gates, handles escalation
- Human intervenes only when orchestrator flags 15-repeat escalation or a gate decision needs judgment

---

## 2. Git Worktrees Setup

Git worktrees provide file isolation between teams — each session has its own working directory while sharing a single git repo.

```bash
# Create the main repo
mkdir -p keel-swarm && cd keel-swarm
git init keel
cd keel

# Initial scaffold (done in Phase 0, before teams spawn)
echo "# keel" > README.md
git add README.md && git commit -m "Initial commit"

# Create worktrees for each team (after Phase 0 completes)
# Actual paths used: $HOME/keel-worktree-{a,b,c}
git worktree add $HOME/keel-worktree-a -b foundation
git worktree add $HOME/keel-worktree-b -b enforcement
git worktree add $HOME/keel-worktree-c -b surface

# NOTE: .keel-swarm/ and results/ directories were planned but never created.
# PROGRESS.md + git log served as the coordination mechanism instead.
```

**Actual folder structure (as used):**

```
$HOME/Curosor_Projects/Keel/   # Root worktree (orchestrator)
$HOME/keel-worktree-a/          # Foundation team
$HOME/keel-worktree-b/          # Enforcement team
$HOME/keel-worktree-c/          # Surface team
```

> **Not created:** `test-corpus/` (unit tests sufficed), `specs/` symlink (agents used `keel-speckit/` directly), `.keel-swarm/` gate markers, `results/` directory.

---

## 3. Phase 0 tmux Setup (Before Agent Teams Exist)

Phase 0 does NOT use agent teams, but it DOES use multiple tmux panes for parallel sandboxed Claude sessions. Each session creates at most 15 files (see [scope-limits.md](scope-limits.md)).

```bash
SESSION="keel-phase0"
tmux new-session -d -s $SESSION -n "scaffold"

# Wave 1: Structural files (4 parallel panes)
# Pane 0: Cargo workspace + Cargo.toml files (Group A — 7 files)
# Pane 1: keel-core types + SQLite (Group B — 5 files) → depends on Group A
# Pane 2: keel-parsers stubs (Group C — 6 files) → depends on Group A
# Pane 3: keel-enforce/cli/output/server (Group D — 8 files) → depends on Group A

# Wave 2: Test files + support (4 parallel panes, after Wave 1 commits)
# Reuse panes 0-3 with new sessions for test file groups
# Pane 0: tests/graph/ + tests/parsing/ + tests/enforcement/ (Group E1)
# Pane 1: tests/resolution/ + tests/cli/ + tests/output/ (Group E2)
# Pane 2: tests/server/ + tests/benchmarks/ + tests/integration/ (Group E3)
# Pane 3: schemas + fixtures + contracts + scripts + config (Group F)
```

Each pane runs: `claude --dangerously-skip-permissions`
Sandbox is configured via `~/.claude/settings.json` (NOT a CLI flag — there is no `--sandbox` flag).
Coordination: git commit + git pull between panes. No Task tool cross-talk.

**Wave 1 must commit before Wave 2 starts** — Wave 2 sessions pull from git to get the Cargo workspace and type definitions created in Wave 1.

---

## 4. Phase 1-3 tmux Session Setup

```bash
#!/bin/bash
# Launch the 4-pane keel swarm session
SESSION="keel-swarm"

tmux new-session -d -s $SESSION -n "swarm"

# Pane 0 (top-left): Orchestrator — root worktree
tmux send-keys -t $SESSION "cd keel-swarm/keel && claude --dangerously-skip-permissions" C-m

# Pane 1 (top-right): Foundation team — worktree-a
tmux split-window -h -t $SESSION
tmux send-keys -t $SESSION "cd keel-swarm/worktree-a && claude --dangerously-skip-permissions" C-m

# Pane 2 (bottom-left): Enforcement team — worktree-b
tmux split-window -v -t $SESSION:0.0
tmux send-keys -t $SESSION "cd keel-swarm/worktree-b && claude --dangerously-skip-permissions" C-m

# Pane 3 (bottom-right): Surface team — worktree-c
tmux split-window -v -t $SESSION:0.1
tmux send-keys -t $SESSION "cd keel-swarm/worktree-c && claude --dangerously-skip-permissions" C-m

tmux attach -t $SESSION
```

Once each Claude Code session is running:
- **Pane 0 (Orchestrator):** Tell it to run `/ralph-loop` with the orchestrator CLAUDE.md instructions (see [operations.md](operations.md))
- **Panes 1-3 (Team Leads):** Each creates its agent team and spawns teammates (see [spawn-prompts.md](spawn-prompts.md))

---

## 5. Agent Teams Configuration

### Claude Code Settings

Add to Claude Code `settings.json`:

```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  },
  "teammateMode": "tmux",
  "sandbox": {
    "enabled": true,
    "autoAllowBashIfSandboxed": true,
    "allowUnsandboxedCommands": false,
    "excludedCommands": ["docker"],
    "network": {
      "allowedDomains": [
        "api.anthropic.com", "*.anthropic.com",
        "github.com", "*.github.com",
        "*.crates.io", "index.crates.io", "static.crates.io", "crates.io",
        "*.npmjs.org", "registry.npmjs.org", "registry.yarnpkg.com",
        "objects.githubusercontent.com"
      ],
      "allowLocalBinding": true
    }
  },
  "permissions": {
    "allow": [
      "Bash(cargo test*)",
      "Bash(cargo build*)",
      "Bash(cargo check*)",
      "Bash(cargo clippy*)",
      "Bash(cargo fmt*)",
      "Bash(cargo install*)",
      "Bash(cargo run*)",
      "Bash(./scripts/*)",
      "Bash(git *)",
      "Bash(gh *)",
      "Bash(npm *)",
      "Bash(npx *)",
      "Bash(ty *)",
      "Bash(rustup *)",
      "Bash(tmux *)",
      "Bash(curl *)",
      "Bash(wget *)",
      "Bash(mkdir *)",
      "Bash(cp *)",
      "Bash(mv *)",
      "Bash(rm *)",
      "Bash(ls *)",
      "Bash(cat *)",
      "Bash(cd *)",
      "Read",
      "Write",
      "Edit",
      "Glob",
      "Grep",
      "Skill",
      "Task",
      "SendMessage",
      "TaskCreate",
      "TaskUpdate",
      "TaskList",
      "TaskGet"
    ],
    "deny": [
      "Read(~/.ssh/**)",
      "Read(~/.aws/**)",
      "Read(~/.kube/**)",
      "Read(~/.gnupg/**)",
      "Read(/etc/shadow)"
    ]
  }
}
```

### Permission Pre-Approval

To reduce friction across agents, pre-approve common operations. With `autoAllowBashIfSandboxed: true`, all bash commands are auto-approved inside the sandbox anyway — these explicit entries serve as documentation and fallback if sandbox is ever disabled:

- **Cargo ecosystem** — `cargo test/build/check/clippy/fmt/install/run` + `rustup`
- **Node ecosystem** — `npm`, `npx` (for VS Code extension)
- **Git + GitHub** — `git`, `gh` (worktrees handle isolation)
- **Language tools** — `ty` (Python type checker subprocess)
- **Infrastructure** — `tmux` (orchestrator), `curl`/`wget`, basic file ops
- **Scripts** — `./scripts/*` (test harness, setup)
- **File tools** — `Read`, `Write`, `Edit`, `Glob`, `Grep` — always allowed (crate ownership prevents conflicts)
- **Agent teams plumbing** — `Skill`, `Task`, `SendMessage`, `TaskCreate`, `TaskUpdate`, `TaskList`, `TaskGet`

### `TeammateIdle` Hook Configuration (Planned — Not Deployed)

> **Note:** These hooks were designed but never deployed. Git history + `/tmux-observe` provided sufficient monitoring. Retained as a template for future swarm runs.

Create `.claude/hooks.json` in each worktree:

```json
{
  "hooks": {
    "TeammateIdle": [
      {
        "command": "echo 'TEAMMATE_IDLE: {{teammate_name}} has gone idle after {{idle_reason}}'",
        "description": "Log teammate idle events for escalation tracking"
      }
    ],
    "TaskCompleted": [
      {
        "command": "echo 'TASK_COMPLETED: {{task_id}} by {{teammate_name}}'",
        "description": "Log task completions for progress tracking"
      }
    ]
  }
}
```

### Delegate Mode for Leads

All 3 team leads run in delegate mode:

```
Task tool with mode: "delegate"
```

This means:
- Lead **cannot** edit files directly
- Lead coordinates via messages, task assignments, and plan approvals
- Lead reviews teammate plans before they implement (plan approval)

---

## 6. Sandbox Hardening

### Why Sandbox?

Agents run with `--dangerously-skip-permissions` for extended unsupervised periods. Without sandboxing, a confused agent could write outside its worktree, exfiltrate secrets, or reach arbitrary network endpoints. OS-level sandboxing makes `--dangerously-skip-permissions` safe.

### 3-Layer Isolation Model

```
Layer 1: Sandbox (OS-level) — bubblewrap restricts filesystem writes to CWD
         (network unrestricted — agents can search docs freely)
         ┌─────────────────────────────────────────────────┐
         │                                                 │
Layer 2: │ Git worktrees (logical) — each team confined    │
         │ to its own worktree directory                   │
         │  ┌───────────┐ ┌───────────┐ ┌───────────┐     │
         │  │worktree-a │ │worktree-b │ │worktree-c │     │
         │  │Foundation │ │Enforcement│ │  Surface  │     │
         │  │           │ │           │ │           │     │
Layer 3: │  │ts/ py/ go/│ │enforce/   │ │tools/ mcp/│     │
         │  │rust/      │ │cli/ out/  │ │vscode/dist│     │
         │  │(crate own)│ │(crate own)│ │(crate own)│     │
         │  └───────────┘ └───────────┘ └───────────┘     │
         └─────────────────────────────────────────────────┘
```

### What Sandbox Prevents vs. Doesn't Prevent

| Threat | Mitigation |
|--------|-----------|
| Agent writes outside worktree | bubblewrap restricts writes to CWD |
| Agent reads SSH keys / AWS creds | `permissions.deny` blocks + bubblewrap |
| Agent runs Docker (hangs inside bubblewrap) | `excludedCommands: ["docker"]` |
| Agent escapes sandbox via unsandboxed fallback | `allowUnsandboxedCommands: false` |

| Risk | Why Sandbox Can't Help | Mitigation |
|------|----------------------|------------|
| Agent overwrites teammate's files | Sandbox CWD = worktree root | Crate ownership (Layer 3) |
| Agent makes bad git commits | Git operations allowed | Code review at gates |
| Agent pushes bad code | Network unrestricted | Branch protection |

### Pre-Flight Sandbox Verification

```bash
# Verify bubblewrap is installed
bwrap --version

# Verify socat is installed (used by tmux teammate mode)
socat -V | head -1

# NOTE: There is no --sandbox CLI flag. Sandbox is configured via settings.json.
# Enable sandbox via ~/.claude/settings.json "sandbox.enabled": true
# Or enable per-session via the /sandbox command (choose option 1: auto-allow)
```

### Crash Recovery

If bubblewrap kills an agent process (OOM, segfault, resource limit):

1. Team lead detects via `TeammateIdle` notification or missing progress
2. Lead spawns a replacement teammate with the same spawn prompt
3. New teammate picks up from the last committed state in git
4. Task is reassigned to the new teammate via `TaskUpdate`

---

## 7. Agent Teams Limitations

### No Cross-Team Messaging

Teams can't message other teams directly. Cross-team coordination uses:
- Git push/pull between worktree branches
- Gate marker files in `.keel-swarm/`
- Shared test results in `results/`
- Orchestrator reading all panes via `/tmux-observe`

### No Nested Teams

Teammates can't spawn their own teams. However:
- Leads CAN have teams (that's the whole architecture)
- Teammates CAN use the Task tool to spawn subagents for complex subtasks
- Subagents are not teammates — they're ephemeral helpers
- **Subagents are subject to scope limits** (max 5 files, max 30 tool calls — see [scope-limits.md](scope-limits.md))

### No Session Resumption

If a teammate crashes, the lead spawns a replacement:
1. Lead detects crash via `TeammateIdle` notification or missing progress
2. Lead creates a new teammate with the same spawn prompt
3. New teammate picks up from the last committed state in git
4. Task is reassigned to the new teammate

### One Team Per Session

Each Claude Code session (worktree) gets exactly one team. This is why the architecture uses 4 separate sessions in tmux panes.

### File Conflicts Within a Team

Teammates sharing a worktree must own non-overlapping files. Keel's crate structure provides natural isolation. If two teammates need to edit the same file, the lead must serialize the work.
