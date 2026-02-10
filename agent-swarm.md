# Keel Agent Swarm Playbook

```yaml
tags: [keel, implementation, agent-swarm, automation, agent-teams]
status: ready
agents: 15 (1 orchestrator + 3 leads + 11 teammates)
```

> This playbook has been decomposed into focused files for easier reading and reduced context pollution. **Read them in order for first-time setup.**

## Playbook Files

| # | File | Contents | Read When |
|---|------|----------|-----------|
| 1 | [README.md](agent-swarm/README.md) | Overview, philosophy, risks, pre-flight checklist | First |
| 2 | [scope-limits.md](agent-swarm/scope-limits.md) | **Agent scope limits, context management rules, lessons learned** | **Before ANY agent work** |
| 3 | [infrastructure.md](agent-swarm/infrastructure.md) | tmux setup, git worktrees, sandbox, agent teams config | Setting up infrastructure |
| 4 | [phases.md](agent-swarm/phases.md) | Contracts, Phase 0-4 deliverables, execution model, gate criteria | During each phase |
| 5 | [spawn-prompts.md](agent-swarm/spawn-prompts.md) | Agent assignments, team architecture, all spawn prompts | Spawning agents |
| 6 | [operations.md](agent-swarm/operations.md) | Ralph loop, cross-team coordination, escalation, audit trail, verification | During autonomous runs |

> **CRITICAL: Read [scope-limits.md](agent-swarm/scope-limits.md) before spawning any agents.** It contains hard limits that prevent context exhaustion — the #1 failure mode for agent swarms (see the 2026-02-09 incident).

## Quick Reference

- **Hard limits:** Max 15 files/session, max 30 tool calls/Task subagent, max 5 min/Task subagent
- **Phase 0:** Uses tmux panes, NOT Task subagents. See [phases.md](agent-swarm/phases.md)
- **Spawn prompts:** Every prompt includes scope limit rules. See [spawn-prompts.md](agent-swarm/spawn-prompts.md)
- **Decision matrix:** Task agent vs tmux pane — see [scope-limits.md](agent-swarm/scope-limits.md)
