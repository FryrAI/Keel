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
| `circuit_breaker.max_failures` | `u32` | `3` | Maximum consecutive failures on the same error-code + hash pair before auto-downgrade. After N failures: attempt 1 = fix_hint, attempt 2 = wider discover context, attempt N = auto-downgrade to WARNING. Resets on success or a different error. |
| `batch.timeout_seconds` | `u64` | `60` | Seconds of inactivity before batch mode auto-expires. Batch mode defers E002, E003, and W001 checks during rapid iteration. |
| `ignore_patterns` | `string[]` | `[]` | Additional glob patterns for files to ignore (beyond `.keelignore`). Uses gitignore syntax. |

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
