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

If no files are specified, compiles all source files in the project.

**Clean compile:** Exit 0, empty stdout. This is intentional -- the LLM agent sees nothing and continues.

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
