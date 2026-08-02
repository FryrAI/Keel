# keel Command Reference

All commands are run from a project directory that has been initialized with `keel init`.

## Global Flags

These flags apply to every command:

| Flag | Description |
|------|-------------|
| `--json` | Output as structured JSON |
| `--llm` | Output as token-optimized LLM format |
| `--verbose` | Include info block and diagnostic messages on stderr |
| `--max-tokens N` | Token budget for LLM output (default: 500). Only effective with `--llm`. |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success, no violations |
| `1` | Violations found |
| `2` | Internal error (not initialized, bad input, database failure) |

---

## keel init

Initialize keel in a repository.

```bash
keel init
```

**What it does:**
1. Creates `.keel/` directory with `keel.json`, `graph.db`, and `cache/`
2. Detects languages by scanning file extensions
3. Creates `.keelignore` with default patterns
4. Installs a `pre-commit` git hook (if `.git/hooks/` exists and no hook is present)
5. Detects AI tool integrations (Cursor, Windsurf, Aider, Continue)

**Performance:** <10s for 50k LOC.

**Fails if:** `.keel/` already exists. Run `keel deinit` first to re-initialize.

---

## keel map

Build or rebuild the full structural graph.

```bash
keel map [--depth 0-3] [--scope modules] [--strict] [--llm-verbose]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` | `1` | Output detail level. `0` = summary counts. `1` = modules + hotspots. `2` = functions with signatures. `3` = full graph. |
| `--scope <modules>` | (all) | Comma-separated module names to restrict the map output |
| `--strict` | off | Exit non-zero on any ERROR-level violations |
| `--llm-verbose` | off | Include full signatures in LLM format output |

**What it does:** Parses every source file with tree-sitter, applies per-language resolvers (Oxc for TS, ty for Python, heuristics for Go, rust-analyzer for Rust), builds call/import/contains edges, and stores everything in `.keel/graph.db`.

**Performance:** <5s for 100k LOC.

**Examples:**

```bash
# Quick summary for CI
keel map --depth 0 --json

# Full context for an LLM agent
keel map --depth 1 --llm

# Detailed map of a specific module
keel map --depth 2 --scope src/auth
```

---

## keel compile

Incrementally validate code after changes.

```bash
keel compile [file...] [flags]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` | `1` | Output detail. `0` = error/warning counts only. `1` = grouped by file. `2` = full detail with context. |
| `--batch-start` | off | Begin batch mode: defers type hints, docstrings, and placement checks. Structural errors (E001, E004, E005) still fire. |
| `--batch-end` | off | End batch mode: fires all deferred checks. Auto-expires after 60s of inactivity. |
| `--strict` | off | Treat warnings as errors (exit 1 on warnings) |
| `--suppress <code>` | (none) | Suppress a specific error/warning code for this run |
| `--format github` | (none) | Emit GitHub Actions annotations (`::error file=..,line=..,title=[CODE]::msg`) instead of a keel format. Replaces the JSON post-processing CI used to do in the workflow; keel ships no runtime dependency, so neither should its action. Combines with `--delta` (annotates only the new violations); overrides `--json`/`--llm`. |

If no files are specified, compiles all source files in the project.

**Clean compile:** Exit 0, empty stdout. This is intentional -- the LLM agent sees nothing and continues.

**Stale graph:** exit 2. `keel map` records the commit it mapped; if that commit is not
an ancestor of `HEAD` (a rebase, an amend, a branch switch, a poisoned CI cache) the
graph describes code this checkout does not contain, and compiling against it would
report phantom callers and removals. Run `keel map`. Graphs with no recorded commit
(mapped by an older keel, or outside a git repo) are never treated as stale. See
[CI](ci.md#the-staleness-guard).

**Performance:** <200ms for a single file.

**Examples:**

```bash
# Validate a single file
keel compile src/auth.ts

# Validate multiple files
keel compile src/auth.ts src/users.ts

# Batch mode for rapid scaffolding
keel compile --batch-start
# ... agent creates multiple files ...
keel compile --batch-end

# Depth-0 for minimal token output
keel compile --depth 0 src/auth.ts --llm
# Output: PRESSURE=LOW BUDGET=expand

# Suppress a specific check
keel compile --suppress E002 src/legacy.py
```

---

## keel discover

Look up a symbol's callers, callees, and graph context.

```bash
keel discover <hash> [--depth N] [--suggest-placement]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` | `1` | Number of hops to traverse from the target node |
| `--suggest-placement` | off | Return top 3 placement suggestions for where new related code should go |

**Performance:** <50ms.

**Examples:**

```bash
# Basic adjacency lookup
keel discover a7Bx3kM9f2Q

# Two-hop traversal
keel discover a7Bx3kM9f2Q --depth 2

# Get placement suggestions
keel discover a7Bx3kM9f2Q --suggest-placement --json
```

---

## keel where

Resolve a hash to its file and line number.

```bash
keel where <hash>
```

Returns the file path and line number where the symbol identified by `<hash>` is defined.

**Performance:** <50ms.

**Example:**

```bash
keel where a7Bx3kM9f2Q
# Output: src/auth.ts:42

keel where a7Bx3kM9f2Q --json
# Output: {"file": "src/auth.ts", "line": 42, "name": "authenticate"}
```

---

## keel explain

Show the resolution reasoning chain for an error on a specific symbol.

```bash
keel explain <error_code> <hash> [--depth 0-3] [--tree]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--depth N` | `1` | Resolution chain depth. `0` = summary only. `1` = first hop. `2` = two hops. `3` = full chain. |
| `--tree` | off | Human-readable tree output instead of flat list |

**Performance:** <50ms.

**Examples:**

```bash
# Why is E001 firing on this hash?
keel explain E001 a7Bx3kM9f2Q

# Full resolution chain
keel explain E001 a7Bx3kM9f2Q --depth 3

# Summary only for LLM
keel explain E001 a7Bx3kM9f2Q --depth 0 --llm
```

---

## keel fix

Generate fix plans for violations, optionally applying them.

```bash
keel fix [hash...] [--file <path>] [--apply]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--file <path>` | (none) | Restrict to violations in this file |
| `--apply` | off | Write fixes to disk and re-compile to verify. Without this flag, only outputs the plan. |

If no hashes are specified, generates plans for all current violations.

**With `--apply`:** Writes changes to files, then re-compiles to verify the fix resolved the violation. Reports applied/failed actions and whether the recompile is clean.

**Examples:**

```bash
# Plan-only (safe, read-only)
keel fix a7Bx3kM9f2Q --json

# Fix all violations in a file
keel fix --file src/auth.ts

# Apply fixes to disk
keel fix a7Bx3kM9f2Q --apply

# Fix everything and apply
keel fix --apply
```

---

## keel name

Suggest names and file locations for new code based on graph analysis.

```bash
keel name "<description>" [--module <path>] [--kind <type>]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--module <path>` | (none) | Constrain suggestions to a specific module or file |
| `--kind <type>` | (none) | Kind of entity: `fn`, `class`, `method` |

Analyzes the existing codebase graph to suggest names that match conventions, and modules where the new code should live based on keyword overlap and structural affinity.

**Performance:** <100ms.

**Examples:**

```bash
# Where should I put a new user authentication function?
keel name "validate user authentication"

# Suggest a class name in a specific module
keel name "database connection pool" --module src/db --kind class

# JSON output for programmatic use
keel name "parse configuration file" --json
```

---

## keel review

Two-sided graph diff against a base ref — the PR cover letter GitHub cannot write.

```bash
keel review --base main
keel review --base origin/main --json
keel review --base origin/main --format github --gate
```

| Flag | Default | Description |
|------|---------|-------------|
| `--base <ref>` | (required) | Ref to diff the working tree against |
| `--format github` | (none) | Emit the new violations as GitHub Actions annotations instead of the cover letter |
| `--gate` | off | Exit 1 when the diff introduced a violation whose code is listed in `review.gate` in `keel.json` (empty by default, so `--gate` alone gates nothing) |

Parses the base side straight out of git (`git show <base>:<path>`) in memory — no
checkout, no worktree — and diffs it against the working tree. For every symbol it
reports whether the **contract** moved (signature changed, added, removed, moved) or
only the body or docstring did, so the report can say "12 functions changed, only 3
changed their contract".

Each contract change carries the stored callers that live in files this diff does
**not** touch — literally the call sites the change did not update. Changed files keel
has no grammar for (`.sql`, `.baml`, fixtures) are listed under `UNANALYZED` rather
than silently omitted.

Both sides are parsed with tree-sitter only (`resolution: tier1`) so the two sides are
symmetric. Review-time only: nothing here runs in the compile hot path.

### Baseline-relative violations

`keel review` also compiles both sides and reports **only the violations the diff
introduced**, under `NEW VIOLATIONS` (`new_violations` in JSON, with a
`pre_existing_violations` count beside it). In a repo carrying tens of thousands of
findings, a PR comment listing current violations is unreadable; listing the ones this
diff added is not.

- Findings match across the two revisions on `(code, file, symbol)` — never a line —
  so reformatting a file with existing violations introduces **zero** new findings, and
  renaming a file does not resurrect the findings inside it.
- Diffed codes: **E002, E003, W005, W006, W007**. These are the ones both sides can
  compute at the same tier. E001/E004/E005 need cross-file reference resolution the
  base blobs never got, so they stay on the `keel compile` surface, head-only —
  diffing them across asymmetric tiers would manufacture phantom findings. The
  contract-change section already answers the same question structurally.
- W007 is evaluated per-PR here: base under `enforce.max_file_lines`, head over it,
  reported once. A file that was already over budget is inherited, not reported.
- Exit code is 0 whatever it finds, unless `--gate` is set and `review.gate` in
  `keel.json` names one of the codes it found.

Honors the clean-output contract — a diff that moved no contract, introduced no
violation, and touched no unparsed file prints nothing and exits 0 (use `--verbose`
for the counts anyway). Requires full git history in CI (`fetch-depth: 0`).

---

## keel validate-plan

Check a plan against the graph **before** any code exists. Resteering a plan is cheap;
resteering 2,000 lines is not.

```bash
keel validate-plan plan.md
cat plan.md | keel validate-plan --llm -
keel validate-plan plan.md --strict
```

| Flag | Default | Description |
|------|---------|-------------|
| `<file>` or `-` | (required) | Plan file (markdown/plain), or `-` to read stdin |
| `--strict` | off | Exit 1 when a live P001/P002 finding is present |

Two outputs from one free-text scan:

1. **The risk report** (unchanged): symbols the plan names that exist in the graph, the
   action detected near them (`remove`, `rename`, `change_signature`, `add_param`), the
   callers at risk, a risk level, and a callers-first suggested order.
2. **Plan findings**: [`P001 unknown_symbol`](error-codes.md#p001--unknown-symbol) — the
   plan calls something the graph does not have — and
   [`P002 signature_mismatch`](error-codes.md#p002--signature-mismatch) — the plan's call
   does not match the stored signature. Each carries the real hash, `file:line`, the
   stored signature, and a `fix_hint`.

**The report never fails.** Exit is 0 whatever it finds, unless `--strict` is passed.
That contract is what lets the [`ExitPlanMode` hook](agent-integration.md#pretooluse-exitplanmode-plan-check)
run on every plan without ever blocking a session.

---

## keel serve

Run a persistent server for real-time enforcement.

```bash
keel serve [--mcp] [--http] [--watch]
```

| Flag | Description |
|------|-------------|
| `--mcp` | Expose keel as an MCP tool server over stdio (for Claude Code, Cursor, etc.) |
| `--http` | Start an HTTP API on `localhost:4815` |
| `--watch` | Watch the file system for changes and auto-recompile |

Flags can be combined. The HTTP server exposes endpoints for `/compile`, `/discover/{hash}`, `/where/{hash}`, `/map`, and `/health`.

**Memory usage:** ~50-100MB.

**Examples:**

```bash
# MCP server for Claude Code
keel serve --mcp

# HTTP + file watching for VS Code extension
keel serve --http --watch

# All modes
keel serve --mcp --http --watch
```

---

## keel stats

Display a telemetry dashboard with graph statistics.

```bash
keel stats [--json]
```

Shows node counts, edge counts, language breakdown, parse timing, and compilation history.

---

## keel deinit

Remove all keel-generated files and configuration.

```bash
keel deinit
```

Deletes the `.keel/` directory, the `.keelignore` file, and the pre-commit hook (if installed by keel). Does not modify your source code.
