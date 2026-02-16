# Keel CLI — Agent Feedback

## Verdict: Keep it

The flow tracing alone justifies keel — 2KB vs 60KB to understand a call chain is a category difference that determines whether an agent completes a task within context or fails.

## Core value (3 commands that matter)

| Command | Why it matters | When to use |
|---------|---------------|-------------|
| `discover --depth 2` | Maps call chains in ~2KB vs ~60KB of Reads. Nothing else does this. | Before editing unfamiliar code. Trace how data flows. |
| `check` | Shows blast radius (callers + change risk) in 1 call. Prevents the most expensive agent mistake: breaking unknown callers (15+ calls to fix). | Before changing any function signature or return type. |
| `compile` (hook) | Catches new errors immediately post-edit via delta mode (+NEW / -FIXED / PRE-EXISTING). Agent never moves on with broken code. | Auto — fires after every Edit/Write. |

Supporting commands that earn their keep: `search` (cross-language function lookup), `discover --name` (jump to function by name), `discover --context` (see function body without Read), `analyze` (file-level smells before refactoring).

## Recommended CLAUDE.md instruction

```
Use `keel check <hash>` before editing functions. Use `keel discover <hash> --depth 2` to trace call chains. keel compile runs automatically as a post-edit hook. Skip keel for trivial edits (typos, docstrings, single-line fixes).
```

## Commands to consider removing

| Command | Why | Alternative |
|---------|-----|-------------|
| `where` | Redundant with `discover --name` | Remove or alias |
| `map` | Redundant with `search` + `discover <file>` | Keep for CI/tooling, remove from agent workflow |
| `name` | Placement still unreliable (low confidence scores, wrong naming conventions). Misleads more than it helps. | Fix or remove. Don't ship unreliable guidance. |

## Open issues

### 1. `keel name` unreliable (MEDIUM)
Low confidence scores (~0.16), wrong naming conventions (suggested `validate_` prefix for an export function). Either fix placement scoring (weight by path keyword overlap, extract conventions from target module) or remove `name` until it's reliable. Unreliable guidance is worse than no guidance.

### 2. `check` high-caller summary (LOW)
Functions with 20+ callers list every call site. Summarize as "24 callers across 8 files" by default, `--verbose` for full list.

### 3. `discover --context` line control (LOW)
Fixed snippet length. Allow `--context N` to control lines shown.
