# Agent Integration Guide

keel is designed to be called by LLM coding agents after every code modification. This guide covers how to wire keel into each supported AI coding tool.

## Overview

The integration pattern is the same across all tools:

1. **Session start** -- run `keel map --llm` to give the agent structural context about the codebase.
2. **After every file edit** -- run `keel compile <file> --llm` to validate the change and surface violations immediately.

`keel init` detects which AI tool directories are present (`.cursor/`, `.windsurf/`, `.aider/`, `.continue/`) and logs what it finds. The configs shown below can be generated manually or via post-init tooling.

## Claude Code

Claude Code supports lifecycle hooks via `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      { "command": "keel map --llm --depth 1" }
    ],
    "PostToolUse": [
      {
        "tools": ["Edit", "Write", "MultiEdit"],
        "command": "bash scripts/post-edit.sh $FILE"
      }
    ]
  }
}
```

The `post-edit.sh` script runs `keel compile` on the changed file and outputs violations in LLM format. See the [Hook Workflow](#hook-workflow) section below.

## Cursor

Cursor uses `.cursor/hooks.json` for lifecycle hooks and `.cursor/rules/` for instruction files:

**.cursor/hooks.json:**
```json
{
  "hooks": {
    "session_start": [
      { "command": "keel map --llm --depth 1" }
    ],
    "post_tool_use": [
      {
        "tools": ["edit_file", "create_file"],
        "command": "bash scripts/post-edit.sh $FILE"
      }
    ]
  }
}
```

**.cursor/rules/keel.mdc:**
```markdown
---
description: keel structural enforcement
globs: ["*.ts", "*.py", "*.go", "*.rs"]
---

After every file modification, keel compile runs automatically via hooks.
If violations appear, fix them before proceeding. Use `keel explain` for context.
Exit 0 + empty output = clean compile. Do not ignore violations.
```

## Gemini CLI

Gemini CLI uses `.gemini/settings.json` for hooks and `GEMINI.md` for instructions:

**.gemini/settings.json:**
```json
{
  "hooks": {
    "session_start": [
      { "command": "keel map --llm --depth 1" }
    ],
    "post_edit": [
      { "command": "bash scripts/post-edit.sh $FILE" }
    ]
  }
}
```

**GEMINI.md:**
```markdown
## Structural Enforcement

This project uses keel for structural code enforcement. After every file edit,
`keel compile` runs automatically. Fix all violations before continuing.

Commands: `keel compile`, `keel discover <hash>`, `keel explain <code> <hash>`
```

## Windsurf

Windsurf uses `.windsurf/hooks.json` for hooks and `.windsurfrules` for instructions:

**.windsurf/hooks.json:**
```json
{
  "hooks": {
    "session_start": [
      { "command": "keel map --llm --depth 1" }
    ],
    "post_edit": [
      { "command": "bash scripts/post-edit.sh $FILE" }
    ]
  }
}
```

**.windsurfrules:**
```
After every file change, keel compile runs automatically.
If keel reports violations, fix them before proceeding.
Use keel explain <error_code> <hash> to understand resolution chains.
Use keel fix <hash> to generate fix plans.
```

## GitHub Copilot

Copilot reads `.github/copilot-instructions.md`:

```markdown
## keel Structural Enforcement

This project uses keel for structural code enforcement.

After modifying any .ts, .py, .go, or .rs file, run:
  keel compile <file> --llm

If violations are reported, fix them before continuing.
Exit 0 + empty stdout = clean compile.

Useful commands:
- keel discover <hash> -- look up callers and callees
- keel explain <code> <hash> -- understand why a violation fires
- keel fix <hash> -- generate a fix plan
- keel name "<description>" -- find the right module for new code
```

## Aider

Aider uses `.aider.conf.yml` for configuration:

```yaml
lint-cmd: keel compile --llm
auto-lint: true
```

This runs `keel compile` after every edit and feeds violations back to the model.

## Letta Code

Letta Code uses `settings.json` and an instruction file:

```json
{
  "hooks": {
    "session_start": ["keel map --llm --depth 1"],
    "post_edit": ["bash scripts/post-edit.sh $FILE"]
  }
}
```

## Antigravity

Antigravity uses `.agent/rules/` for instruction files and `.agent/skills/` for tool definitions:

**.agent/rules/keel.md:**
```markdown
This project uses keel structural enforcement. After every file modification,
run `keel compile <file> --llm` and fix any violations before continuing.
```

**.agent/skills/keel/SKILL.md:**
```markdown
# keel Skill

Structural code enforcement for this codebase.

## Commands
- keel compile <file> -- validate after changes
- keel discover <hash> -- adjacency lookup
- keel explain <code> <hash> -- resolution chain
- keel fix <hash> [--apply] -- generate/apply fixes
```

## Universal Fallback: AGENTS.md

For any tool that reads a project-level instruction file, create `AGENTS.md` at the project root:

```markdown
## keel Structural Enforcement

This project uses keel. After every file edit:
1. Run: keel compile <changed-file> --llm
2. If exit 0 + empty stdout: clean, continue
3. If exit 1: violations found, fix them before proceeding

Key commands: keel discover, keel explain, keel fix, keel name
```

## Hook Workflow

The standard hook pattern uses two lifecycle events:

### SessionStart

Runs `keel map --llm --depth 1` once at the start of the session. This gives the agent a structural overview: modules, hotspots, and the graph shape. Costs roughly 200-500 tokens depending on project size.

### PostToolUse (post-edit)

Runs after every file edit. The `scripts/post-edit.sh` hook:

1. Receives the changed file path as `$FILE`
2. Runs `keel compile "$FILE" --llm --depth 1`
3. If exit 0 (clean compile): outputs nothing
4. If exit 1 (violations): outputs the violations in LLM format

This keeps the agent in a tight validate-fix loop without wasting tokens on clean compiles.

## Batch Mode

During rapid scaffolding (creating many files at once), use batch mode to defer non-critical checks:

```bash
keel compile --batch-start
# ... agent creates 10 files ...
keel compile --batch-end
```

Batch mode defers type hint (E002), docstring (E003), and placement (W001) checks. Structural errors (E001 broken caller, E004 function removed, E005 arity mismatch) still fire immediately. Batch mode auto-expires after 60 seconds of inactivity.

## MCP Server Mode

For tools that support MCP (Model Context Protocol), keel can run as a persistent tool server:

```bash
keel serve --mcp
```

This exposes keel's commands as MCP tools that the agent can call directly, without shell exec. The MCP server supports `compile`, `discover`, `where`, `explain`, `map`, and `fix` operations.

To also enable HTTP and file-watching:

```bash
keel serve --mcp --http --watch
```
