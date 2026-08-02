# Configuration Reference

keel stores all configuration in the `.keel/` directory at your project root. This directory is created by `keel init`.

## .keel/keel.json

The main configuration file. All fields have sensible defaults -- you only need to modify values you want to change.

```json
{
  "version": "0.1.0",
  "languages": ["typescript", "python", "go", "rust"],
  "enforce": {
    "type_hints": true,
    "docstrings": true,
    "placement": true
  },
  "circuit_breaker": {
    "max_failures": 3
  },
  "batch": {
    "timeout_seconds": 60
  },
  "ignore_patterns": []
}
```

### Field Reference

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | `string` | `"0.1.0"` | Config schema version. Do not modify. |
| `languages` | `string[]` | `[]` | Languages detected in the project. Set automatically by `keel init`. Valid values: `"typescript"`, `"python"`, `"go"`, `"rust"`. |
| `enforce.type_hints` | `bool` | `true` | Enforce type annotations. When true, functions without type hints produce E002 errors. Applies primarily to Python (which requires explicit annotations) and JavaScript (which requires JSDoc `@param`/`@returns`). TypeScript, Go, and Rust are already statically typed. |
| `enforce.docstrings` | `bool` | `true` | Enforce documentation. When true, public functions without docstrings produce E003 errors. |
| `enforce.placement` | `bool` | `true` | Enforce structural placement. When true, functions placed in modules where they don't belong produce W001 warnings. |
| `enforce.progressive` | `bool` | `true` | Progressive adoption. When true, E002/E003 on pre-existing functions the current change didn't touch are downgraded to WARNING instead of ERROR, so adopting keel on a legacy repo doesn't flood errors. |
| `enforce.dead_code` | `bool` | `true` | Enforce liveness. When true, private functions with no callers anywhere in the graph produce W005 warnings. |
| `enforce.duplication` | `bool` | `true` | Enforce non-duplication. When true, function bodies identical (whitespace-normalized) to a function elsewhere in the graph produce W006 warnings. |
| `enforce.oversized_files` | `bool` | `true` | Enforce file size budgets. When true, files that exceed `enforce.max_file_lines` and grew since the last `keel map` produce W007 warnings. |
| `enforce.max_file_lines` | `u32` | `400` | Line budget used by the W007 oversized-file check. |
| `circuit_breaker.max_failures` | `u32` | `3` | Maximum consecutive failures on the same error-code + hash pair before auto-downgrade. After N failures: attempt 1 = fix_hint, attempt 2 = wider discover context, attempt N = auto-downgrade to WARNING. Resets on success or a different error. |
| `batch.timeout_seconds` | `u64` | `60` | Seconds of inactivity before batch mode auto-expires. Batch mode defers E002, E003, and W001 checks during rapid iteration. |
| `ignore_patterns` | `string[]` | `[]` | Additional glob patterns for files to ignore (beyond `.keelignore`). Uses gitignore syntax. |
| `tier3.enabled` | `bool` | `false` | Enable Tier 3 (LSP/SCIP) resolution for references tree-sitter and per-language enhancers can't resolve. Higher precision, slower — on-demand only. |
| `architecture.count_type_deps` | `bool` | `false` | Count type-only references as cross-boundary dependencies for W009. Off by default: depending on another package's *types* is the behaviour you want, and in a workspace sharing a canonical types crate that pattern dominates. Only `calls` count unless this is enabled. |
| `architecture.deny` | `[string, string][]` | `[]` | Ordered boundary pairs that must never depend on each other, e.g. `[["harness", "core"]]`. A dependency matching a pair is reported as E006 `layer_violation` (ERROR, exit 1) instead of W009. Empty by default — keel stays non-opinionated about which layers a repo has. |

W009 itself has no toggle: it is self-baselining (everything already in the graph is grandfathered) and silent
in repos that declare no packages, so there is nothing to turn off. Use `keel compile --suppress W009` for a
one-off run.

### Enforcement per Language

| Language | Type hints | Docstrings | Placement |
|----------|-----------|------------|-----------|
| TypeScript | Validates signatures match callers (already typed) | Public exports | Module boundaries |
| Python | Requires explicit `def f(x: int) -> str` annotations | Public functions | Module boundaries |
| Go | Validates signatures match callers (already typed) | Exported functions | Package boundaries |
| Rust | Validates signatures match callers (already typed) | Public items | Module boundaries |
| JavaScript | Requires JSDoc `@param` and `@returns` | Public exports | Module boundaries |

## .keelignore

A gitignore-syntax file that specifies which files and directories keel should skip when scanning. Created automatically by `keel init` with these defaults:

```
node_modules/
__pycache__/
target/
dist/
build/
.next/
vendor/
.venv/
```

Add your own patterns to skip generated code, vendored dependencies, or large binary directories:

```
# Generated protobuf code
src/generated/

# Test fixtures with intentional violations
tests/fixtures/bad-code/

# Large asset directories
assets/

# Specific files
config/legacy-router.ts
```

## .keel/ Directory Structure

After initialization, the `.keel/` directory contains:

| File | Purpose |
|------|---------|
| `keel.json` | Main configuration (described above) |
| `graph.db` | SQLite database storing the structural graph |
| `cache/` | Incremental parsing cache |
| `telemetry.db` | Compilation history and statistics (used by `keel stats`) |
| `session.json` | Temporary session state (batch mode, circuit breaker state) |

The `graph.db`, `telemetry.db`, and `session.json` files should be added to `.gitignore` (they are environment-specific). The `keel.json` file should be committed to version control so all team members share the same enforcement settings.

## Example Configurations

### Strict mode (new project, zero tolerance)

```json
{
  "version": "0.1.0",
  "languages": ["typescript"],
  "enforce": {
    "type_hints": true,
    "docstrings": true,
    "placement": true
  },
  "circuit_breaker": {
    "max_failures": 1
  },
  "batch": {
    "timeout_seconds": 30
  }
}
```

### Relaxed mode (legacy codebase migration)

```json
{
  "version": "0.1.0",
  "languages": ["python", "typescript"],
  "enforce": {
    "type_hints": false,
    "docstrings": false,
    "placement": true
  },
  "circuit_breaker": {
    "max_failures": 5
  },
  "batch": {
    "timeout_seconds": 120
  },
  "ignore_patterns": [
    "src/legacy/**",
    "*.generated.ts"
  ]
}
```

### Minimal (structural errors only)

```json
{
  "version": "0.1.0",
  "languages": ["go", "rust"],
  "enforce": {
    "type_hints": false,
    "docstrings": false,
    "placement": false
  }
}
```

This configuration only fires on structural errors (E001 broken callers, E004 function removed, E005 arity mismatch) -- the violations that cannot be ignored.
