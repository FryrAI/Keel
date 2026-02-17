# Keel CLI — Agent Feedback

## Verdict: Keep it. The core 3 commands are genuinely useful.

Tested across 4 iterations by multiple agent teams. An honest-evaluator agent doing real tasks (not testing keel — doing actual work with tools available) reported: "If keel wasn't installed, I'd miss `check` and `discover --depth 2`. For investigation tasks, keel is my first instinct now. For simple edits, I forget it exists."

## The 3 commands that matter

### `discover --depth 2` — flow tracing
Traced Excel→HierarchyBuilder→graph node chain in ~1.5KB. Without keel: 4+ file Reads (~60KB). Agent said "keel was my first instinct because I wanted the call chain, not the code." With `--context N` flag, function body snippets are shown inline — enough to understand what each function does without a Read.

**When agents reach for it naturally:** Understanding unfamiliar code. "How does X get to Y?"

### `check` — blast radius before edits
CHANGE_RISK scoring now works correctly:
- 0 callers → CHANGE_RISK=LOW (get_graph_data)
- 5 callers, same file → CHANGE_RISK=MODERATE (parse_agtype)
- 24 callers, 8 files → CHANGE_RISK=HIGH (hash_password)

High-caller output now summarizes ("24 callers across 8 files") instead of listing every call site. Cross-checked with Grep — no missed callers.

**When agents reach for it naturally:** Before changing any function signature. "What will break?"

### `compile` delta — post-edit error catching
Shows +NEW / -FIXED / PRE-EXISTING. Clean files return "0 errors" cleanly. As a hook, it catches broken callers immediately after Edit/Write.

**When agents reach for it naturally:** They don't — it runs automatically as a hook. That's the point.

## Supporting commands worth keeping

| Command | Why |
|---------|-----|
| `search` | Cross-language function lookup. `--limit N` controls noise. |
| `discover --name` | Jump to function by name. Works. |
| `discover <file>` | File symbol table. Quick "what's in this file?" |
| `discover --context N` | Function body snippet. Avoids Read for quick checks. |
| `analyze` | File-level smells before refactoring. |

## Commands to consider removing

| Command | Why |
|---------|-----|
| `where` | Redundant with `discover --name` |
| `map` | Redundant with `search` + `discover <file>` for agents. Keep for CI only. |
| `name` | Improved (suggests right directory now, shows low-confidence warning) but still not reliable enough for agents to trust without verification. |

## Recommended CLAUDE.md instruction

```
Use `keel check <hash>` before editing functions. Use `keel discover <hash> --depth 2` to trace call chains. keel compile runs automatically as a post-edit hook. Skip keel for trivial edits (typos, docstrings, single-line fixes).
```

## Key finding from honest evaluation

The honest-evaluator agent was given 3 real tasks with both keel and Grep/Read available. Results:

1. **JWT auth flow tracing** — reached for keel first. "I wanted the call chain, not the code."
2. **Add parameter to endpoint** — used keel check first (0 callers = safe), then Read to edit. "Check was useful pre-flight, but I still needed Read."
3. **ExcelParser refactor blast radius** — keel check found 10+ callers across multiple files. "This is where keel shines — I would NOT have Grepped every caller manually."

**Pattern:** Agents naturally reach for keel when the question is "what connects to what?" They reach for Read when the question is "what does this code do?" Both questions come up in most tasks. Keel doesn't replace Read — it answers a different question that Read can't answer cheaply.

## Only remaining issue

`keel name` — directionally correct now (right directory, low-confidence warning) but naming conventions still don't match target module patterns. Not blocking — agents can use `search` + `discover` to verify placement. Fix when prioritized.
