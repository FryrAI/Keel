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
| 1 | [[agent-swarm/README\|README.md]] | Overview, philosophy, risks, pre-flight checklist | First |
| 2 | [[agent-swarm/scope-limits\|scope-limits.md]] | **Agent scope limits, context management rules, lessons learned** | **Before ANY agent work** |
| 3 | [[agent-swarm/infrastructure\|infrastructure.md]] | tmux setup, git worktrees, sandbox, agent teams config | Setting up infrastructure |
| 4 | [[agent-swarm/phases\|phases.md]] | Contracts, Phase 0-4 deliverables, execution model, gate criteria | During each phase |
| 5 | [[agent-swarm/spawn-prompts\|spawn-prompts.md]] | Agent assignments, team architecture, all spawn prompts | Spawning agents |
| 6 | [[agent-swarm/operations\|operations.md]] | Ralph loop, cross-team coordination, escalation, audit trail, verification | During autonomous runs |

> **CRITICAL: Read [[agent-swarm/scope-limits|scope-limits.md]] before spawning any agents.** It contains hard limits that prevent context exhaustion — the #1 failure mode for agent swarms (see the 2026-02-09 incident).

## Quick Reference

- **Hard limits:** Max 15 files/session, max 30 tool calls/Task subagent, max 5 min/Task subagent
- **Phase 0:** Uses tmux panes, NOT Task subagents. See [[agent-swarm/phases#2. Phase 0|phases.md]]
- **Spawn prompts:** Every prompt includes scope limit rules. See [[agent-swarm/spawn-prompts|spawn-prompts.md]]
- **Decision matrix:** Task agent vs tmux pane — see [[agent-swarm/scope-limits#5. Task Agent vs. tmux Pane Decision Matrix|scope-limits.md]]
