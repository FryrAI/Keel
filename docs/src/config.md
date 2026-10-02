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
| `enforce.duplication` | `bool` | `true` | Enforce non-duplication. When true, function bodies identical (whitespace-normalized, Type-1) or structurally identical after identifier/literal normalization (Type-2, lower confidence) to a function elsewhere in the graph produce W006 warnings. |
| `enforce.oversized_files` | `bool` | `true` | Enforce file size budgets. When true, files that exceed `enforce.max_file_lines` and grew since the last `keel map` produce W007 warnings. |
| `enforce.max_file_lines` | `u32` | `400` | Line budget used by the W007 oversized-file check. |
| `circuit_breaker.max_failures` | `u32` | `3` | Maximum consecutive failures on the same error-code + hash pair before auto-downgrade. After N failures: attempt 1 = fix_hint, attempt 2 = wider discover context, attempt N = auto-downgrade to WARNING. Resets on success or a different error. |
| `batch.timeout_seconds` | `u64` | `60` | Seconds of inactivity before batch mode auto-expires. Batch mode defers E002, E003, and W001 checks during rapid iteration. |
| `ignore_patterns` | `string[]` | `[]` | Additional glob patterns for files to ignore (beyond `.keelignore`). Uses gitignore syntax. |
| `tier3.enabled` | `bool` | `false` | Enable Tier 3 (LSP/SCIP) resolution for references tree-sitter and per-language enhancers can't resolve. Higher precision, slower — on-demand only. |
| `architecture.count_type_deps` | `bool` | `false` | Count type-only references as cross-boundary dependencies for W009. Off by default: depending on another package's *types* is the behaviour you want, and in a workspace sharing a canonical types crate that pattern dominates. Only `calls` count unless this is enabled. |
| `architecture.deny` | `[string, string][]` | `[]` | Ordered boundary pairs that must never depend on each other, e.g. `[["harness", "core"]]`. A dependency matching a pair is reported as E006 `layer_violation` (ERROR, exit 1) instead of W009. Empty by default — keel stays non-opinionated about which layers a repo has. |
| `review.gate` | `string[]` | `[]` | Violation codes that make `keel review --gate` exit 1 when the diff *introduced* one, e.g. `["E003", "W007"]`. Empty by default: a new finding is a report, not a broken build, until a repo opts in per code. `--gate` without a list gates nothing, deliberately — turning the switch on in CI before agreeing the list must not fail every PR. |

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

### Expression homes (W011 / E007)

Opt in to case-sensitive literal or regex patterns whose meaning belongs in one place:

```json
{
  "homes": [{
    "name": "local-civil-day",
    "patterns": ["date_naive()", {"regex": "AT TIME ZONE '[A-Za-z/_]+'"}],
    "home": ["src/time.rs"],
    "scope": "crates/*/src"
  }],
  "enforce": {"homes": "error"},
  "review": {"gate": ["E007"]}
}
```

`homes` defaults to `[]` (no work). `enforce.homes` is `"warning"` by default (W011),
with `"error"` selecting E007. Each rule requires a nonempty `name` and nonempty
`patterns`; `home` and `scope` accept a string or an array of globs. Missing/empty
`home` bans the patterns everywhere in scope; missing/empty `scope` means the
whole repo. A glob matches a path or any ancestor directory: `crates/*/src`
covers descendants, while `*` never crosses `/`. Paths use `/` relative to the
current worktree root, including when invoked from a subdirectory.
Glob normalization drops leading `/`, empty components and `.` components;
`/src`, `.//src` and `src/` therefore mean `src`. A `..` component rejects the
rule with a named warning. A glob normalized to the repo root means whole-repo
scope, but is rejected as a home; use an empty `home` to ban the pattern.
Malformed rules are skipped with a named stderr warning without discarding
other config. Duplicate names keep the first valid rule. Warnings are emitted
once per identical malformed rule per process, including telemetry reloads.

String patterns are literal substrings; `{"regex": "…"}` patterns use Rust's
`regex` syntax and match one line at a time, never across newlines. Invalid,
empty, or empty-string-matching regexes reject their containing rule with a
named warning. Regex objects accept only the `regex` key. An explicit
`keel config homes '…'` write refuses invalid regex objects without changing the file.
There is no semantic/template inference. One violation per file/line lists all
matching rules, patterns, and homes, with an empty hash; regexes display as `regex "…"`.

Matching includes code, string literals (including embedded SQL), and test source.
Comments are excluded by default for Rust (including nested block and doc comments),
Python, Go, TypeScript, TSX, JavaScript, JSX, and Bash. Syntax-tree comment nodes
(including JavaScript HTML comments) and hash-bang lines are deleted except for
newlines, on both sides of the comparison. Blank resulting lines never match.
Each eligible file is parsed without a raw-match shortcut. Strings still match;
Python docstrings are strings and remain checked. SQL, Typst, and raw Svelte/Astro
markup remain unmasked, so their comments still match. Removing comments is silent
when it preserves which code lands on each line; removing a multi-line comment
inside a statement can change that line split and produce a finding.
Uncommenting a matching baselined line introduces a finding.

The baseline is Git text, independent of `graph.db`: compile compares with
HEAD, or the `--since` commit; review compares with `--base`. Occurrences are
multisets keyed by rule, pattern, and the comment-free line with whitespace runs
collapsed. Moving or reindenting an unchanged line is silent; another identical
line adds an occurrence; other line edits are checked again. Removed occurrences
cancel matching additions across the checked file set: review uses all diffed
files, while compile uses the files selected for that invocation. Compiling only
a move's destination still reports it unless Git detects a rename. Home or out-of-scope paths contribute no
removals to this pool. Symlinks are skipped; their targets are checked only when
selected under their own paths. Base blobs are read only for eligible base paths.
Compile and review recognize Git renames and check home/scope eligibility on both
sides, so moving an expression out of its permitted home fires. Compile detects
renames against the same immutable base used for homes, consuming each renamed
base once even when the old deletion is selected separately. The destination must
be indexed: a plain unstaged `mv` leaves only the deletion visible to `--changed`,
and explicitly compiling its untracked destination reports the occurrence.
`--since` selects files from `<base>..HEAD` but compares their working-tree text
against `<base>`, including renames committed in that range.
Non-Git repos, unborn HEAD, or unresolvable compile bases skip homes with one
`--verbose` note. Missing base files have an empty baseline; Git read failures
or non-UTF-8 base blobs skip that file instead of treating it as new.

For a committed ratchet, CI needs BOTH the gate configuration above and this
command (fetch the base ref first):

```sh
keel map
keel review --base origin/main --gate
```

With warning severity, use `review.gate: ["W011"]`. `--gate` with an empty list
gates nothing, and `enforce.homes: "error"` alone does not gate review. A fresh
map at PR head cannot erase review's base-relative findings. The bundled action
must be configured/customized to run this gate command; its default review call
is a report. Ordinary compile's HEAD comparison stops reporting a committed
addition, so the review gate is the committed ratchet.

CLI compile applies ordinary suppression, batch deferral (W011 only), circuit
breaker and `--delta`; E007 starts as ERROR regardless of progressive adoption.
The empty hash gives E007 one breaker counter per file. Its stored fingerprint
is the set of normalized offending line identities in that file's own surplus,
before cross-file move cancellation. Unchanged sets never advance the counter;
a strict subset resets it as progress. A new identity counts as an attempt,
with the third attempt downgrading the remaining findings. Changing the selected
file set alone cannot advance it. Persisted errors do not gate `--delta` a second
time. Review is stateless and does not apply these compile controls.
Server/watch/HTTP/MCP compile do not run homes; CLI and MCP review do.
