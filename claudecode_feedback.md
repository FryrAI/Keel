# Claude Code Agent Feedback on Keel CLI

Real-world feedback from using keel to investigate a FastAPI route in a medium-sized project (1149 nodes, 1625 edges, 143 modules). This feedback is tool-general — not codebase-specific.

## Task performed

Explore an API endpoint — understand its structure, dependencies, callees, and health — using keel as the primary navigation tool.

## Current value by command

| Command | Value | Why |
|---------|-------|-----|
| `discover` | Medium | Caller/callee insight is genuinely useful, but hash lookup friction kills it |
| `compile` | Medium | Good backpressure, but empty hashes in errors make them non-actionable |
| `map` | Low-Medium | Hotspots are good; module listings are too sparse to act on |
| `where` | Low | Works fine, rarely needed independently |
| `name` | Zero | Returns empty suggestions for every query tested |

**Net result for this session:** Keel cost more tokens than it saved. The insight was real but the friction to get there was higher than `Grep` + `Read`.

---

## The core problem: hash-first design

Every useful keel command requires a hash. But finding that hash is the hardest part.

My actual workflow to discover a function:
```
keel map --llm                    # no hashes shown for non-hotspots
keel map --json | python3 -c "…" # parse JSON to find hotspot hashes
keel discover <hash>              # finally works
```

3 tool calls, ~1500 tokens consumed, just to get started. If the function wasn't a hotspot, I'd have had no path to its hash at all without parsing the full JSON map.

**What I needed:** `keel discover --file path/to/file.py` or `keel discover --name my_function`. One call. Done.

---

## What needs improvement (prioritized by token savings)

### 1. Accept file paths and function names everywhere (HIGH)

**Current:** Every command requires a hash.
**Needed:** Accept `file:line`, `file::function_name`, or just `function_name` as alternatives.

```bash
# These should all work:
keel discover path/to/file.py::my_function
keel discover --file path/to/file.py
keel discover --name my_function
keel discover <hash>  # still works
```

**Token savings:** ~800 tokens per investigation (eliminates JSON parsing step entirely).

### 2. Add `keel list <file>` command (HIGH)

I know the file, I need to see what's in it from keel's perspective.

```bash
$ keel list path/to/file.py
FILE path/to/file.py fns=4 cls=3 edges=26

  CLASSES
    abc123  MyModel       :22-28   callers=1 callees=0
    def456  MyResponse    :31-35   callers=1 callees=0

  FUNCTIONS
    ghi789  helper_fn     :44-68   callers=1 callees=0
    jkl012  main_handler  :78-200  callers=0 callees=5
```

**Why:** This is the single most common thing an agent needs — "what's in this file and how connected is it?" Currently requires `Read` + `keel map --json` + custom parsing.

**Token savings:** ~1200 tokens per file investigation.

### 3. Add `keel search <term>` command (HIGH)

I started looking for "export" functionality and keel had nothing. I had to grep keel's own output.

```bash
$ keel search export
No matches for "export" in code graph.

$ keel search handler
MATCHES for "handler"
  jkl012  main_handler    path/to/file.py:78     callers=0 callees=5
  mno345  error_handler   path/to/errors.py:12   callers=8 callees=0
```

**Token savings:** ~600 tokens (eliminates `keel map | grep` + JSON parsing).

### 4. Populate hashes in compile errors (MEDIUM)

Currently:
```
E002 missing_type_hints hash= FIX: Add type annotations...
```

The `hash=` is empty. If it said `hash=jkl012`, I could immediately `keel discover` for context. Right now compile errors are a dead end — I have to figure out which function they refer to from the text description alone.

```
E002 missing_type_hints hash=jkl012 FIX: Add type annotations to main_handler
```

**Token savings:** ~400 tokens per compile-fix cycle.

### 5. Show functions in `keel map` module listings (MEDIUM)

Currently:
```
MODULE path/to/file.py fns=4 cls=3 edges=26
```

With a `--depth` flag or by default in `--llm`:
```
MODULE path/to/file.py fns=4 cls=3 edges=26
  fn main_handler jkl012 ↑0 ↓5
  fn helper_fn ghi789 ↑1 ↓0
  cls MyModel abc123 ↑1
  cls MyResponse def456 ↑1
```

This makes `keel map` self-sufficient — no second command needed to find hashes.

### 6. Make `keel name` actually work (MEDIUM)

Tested 4 different queries, all returned empty suggestions — including describing a function that already exists in the codebase.

What it should return:

```bash
$ keel name "export data as CSV" --kind fn
SUGGESTIONS for "export data as CSV"

  1. export_data_csv
     Location: path/to/api/export.py (new file)
     Reason: Follows existing module pattern, separates concerns
     Near: get_data (jkl012) — likely reuses same query logic

  2. export_csv
     Location: path/to/file.py:201 (append)
     Reason: Same module as get_data, shares models
     Warning: File already at line limit, consider new file
```

**Value:** Saves 3-4 tool calls (Grep for naming conventions + Glob for file structure + Read for context).

---

## What would make keel truly HIGH value on every invocation

### A. Pre-edit backpressure: `keel check <hash|name|file>`

Before I edit a function, one command that tells me everything I need:

```bash
$ keel check main_handler
CHECK main_handler (path/to/file.py:78-200)

  SAFE TO EDIT
  ├── 0 upstream callers (HTTP route only)
  ├── 5 callees (all local to this file)
  └── No cross-module dependents

  CURRENT ISSUES
  ├── E002: Missing type hints on parameters
  └── W: File exceeds line limit

  SUGGESTIONS
  ├── Consider extracting repeated query patterns
  └── helper_fn is single-caller — inline or co-locate
```

This is the **"measure twice"** tool. One call before editing, and I know exactly what I'm working with: risk level, current health, and what to watch out for. No guessing.

### B. Post-edit validation: auto-compile with error deltas

After every edit, keel should auto-compile and show **what changed**, not just the total error count.

```
COMPILE DELTA after edit to path/to/file.py
  +1 error:  E001 broken_call — helper_fn signature changed but caller not updated
  -1 error:  E002 missing_type_hints — fixed by this edit
  NET: +0 errors (blocked: new error introduced, fix before continuing)
```

The key: **diff the errors**. Don't show all pre-existing errors. Show what I *introduced*. This is the difference between useful backpressure and noise.

### C. Context-aware discover: show code snippets

`keel discover` tells me signatures but not logic. For an agent, knowing that `helper_fn` returns `Dict[str, Any]` is less useful than knowing *what it actually does*.

Add a `--context` flag that includes the first 5-10 lines of the function body:

```bash
$ keel discover ghi789 --context
DISCOVER helper_fn (path/to/file.py:44-68)
  sig: helper_fn(value: Any) -> Dict[str, Any]
  callers: 1 (main_handler)

  BODY (first 10 lines):
    if value is None:
        return {}
    if isinstance(value, dict):
        if "properties" in value:
            return value["properties"]
        ...
```

This could eliminate ~50% of `Read` calls during investigation. The agent often just needs a sense of what the function does, not the full file.

### D. Graph-aware analysis: `keel analyze <file>`

Keel knows the structure. It should be able to surface architectural observations:

```bash
$ keel analyze path/to/file.py
ANALYSIS path/to/file.py

  STRUCTURE
  ├── 1 route handler (main_handler, 122 lines)
  ├── 2 helpers (helper_fn, setup_fn)
  └── 3 models (MyModel, MyLink, MyResponse)

  SMELLS
  ├── MONOLITH: main_handler has 5 repetitive blocks
  ├── OVERSIZED: File exceeds line limit
  └── ISOLATED: All callees are file-local, no shared utilities

  REFACTOR OPPORTUNITIES
  ├── Extract common pattern — deduplicates repeated blocks
  ├── Move models to schemas.py — enables reuse by sibling modules
  └── Split route logic from query logic into separate files
```

This is the dream — keel doesn't just enforce rules, it **guides architecture**.

---

## Token budget estimate

For a typical "investigate then edit" cycle:

| Step | Today (tokens) | With improvements (tokens) |
|------|---------------|---------------------------|
| Find the function | ~1500 (map + JSON parse + grep) | ~200 (search or list) |
| Understand dependencies | ~800 (discover + where) | ~400 (discover with context) |
| Pre-edit safety check | ~600 (manual grep for callers) | ~200 (keel check) |
| Post-edit validation | ~400 (manual compile + interpret) | ~100 (auto-compile delta) |
| **Total** | **~3300** | **~900** |

That's a **3.6x reduction** in tokens for the most common agent workflow.

---

## Summary

Keel's graph data is genuinely valuable — knowing caller/callee relationships, detecting isolated vs highly-connected code, and enforcing quality standards. The problem is **access friction**. Every useful insight currently requires 2-3 intermediate steps to extract.

The path from "sometimes useful" to "indispensable":

1. **Remove the hash barrier** — accept names, paths, and patterns everywhere
2. **Add `list` and `search`** — let agents navigate the graph without parsing JSON
3. **Make `name` work** — placement guidance saves real tool calls
4. **Add `check`** — one pre-edit command that answers "is it safe to change this?"
5. **Auto-compile deltas** — post-edit backpressure that shows what I broke, not everything that's broken
6. **Context in discover** — code snippets eliminate Read calls

**The golden rule: every keel command should replace 2+ tool calls, not require 2+ tool calls to set up.**
