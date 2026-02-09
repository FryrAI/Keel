# Spec 007: CLI Commands — All User-Facing Commands

```yaml
tags: [keel, spec, cli, commands, enforcement]
owner: Agent B (Enforcement)
dependencies:
  - "[[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]]"
  - "[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]"
prd_sections: [4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.8, 4.9, 4.10, 20]
priority: P0 — all user and LLM interactions flow through CLI
```

## Summary

This spec defines every CLI command keel exposes: `init`, `map`, `discover`, `compile`, `where`, `explain`, `deinit`, `serve`, and `stats`. Each command is fully specified with behavior, flags, exit codes, and output semantics. The CLI is the universal integration surface — every LLM tool can call keel via shell commands. Agent B owns the enforcement semantics; Agent C owns the server/transport layer for `serve` (see [[keel-speckit/010-mcp-http-server/spec|Spec 010]]).

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 4.1 | `keel init` — auto-detect, parse, generate graph, generate configs, merge strategy |
| 4.2 | `keel map` — full re-parse, diff, performance targets |
| 4.3 | `keel discover` — adjacency list, module context, configurable depth |
| 4.4 | `keel compile` — incremental parse, contract validation, batch mode, suppress mechanism |
| 4.5 | `keel where` — hash to file:line, STALE flag |
| 4.6 | `keel deinit` — remove all generated files, preserve config.toml |
| 4.8 | `keel serve` — MCP, HTTP, watch modes (transport spec in Spec 010) |
| 4.9 | `keel explain` — resolution chain, confidence, tier |
| 4.10 | Circuit breaker / escalation design |
| 20 | Developer experience — exit codes, colored output, quiet defaults, batch mode |

---

## Dependencies

- **[[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]]** — all commands read/write the graph defined here
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]** — `compile` delegates enforcement logic (contract validation, type hint checks, docstring checks, placement scoring) to the enforcement engine

---

## CLI Framework

**Argument parsing:** `clap` (Rust). All commands follow the pattern `keel <command> [args] [flags]`.

**Global flags (available on all commands):**

| Flag | Description |
|------|-------------|
| `--json` | Structured JSON output (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |
| `--llm` | Token-optimized LLM output (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |
| `--verbose` | Include info block in output |
| `--help` | Show usage |
| `--version` | Print keel version |

---

## Command Specifications

### `keel init`

**Purpose:** Initialize keel in a repository. Zero-config for common project structures.

**Behavior:**

1. **Auto-detect languages** present in the repo from file extensions and config files (TypeScript, Python, Go, Rust in Phase 1).
2. **Read existing project configuration** to derive initial enforcement settings:
   - `tsconfig.json` strict mode -> type enforcement enabled
   - `pyproject.toml` `[tool.mypy]` -> type hint expectations
   - `.eslintrc` -> naming conventions
3. **Parse entire codebase** via tree-sitter into function/class/module graph. Discover external touchpoints (HTTP endpoints served, API calls made, database queries, message queue producers/consumers, gRPC service definitions).
4. **Generate graph** stored in `.keel/` directory:
   - `.keel/graph.db` — SQLite database (gitignored). Full graph + resolution cache + metadata.
   - `.keel/manifest.json` — committed. Lightweight, human-readable module summary.
   - `.keel/config.toml` — committed. Team configuration with sane defaults.
   - `.keel/hooks/post-edit.sh` — committed. Shared hook script for all enforced tools.
   - `.keel/telemetry.db` — SQLite (gitignored). Local telemetry.
5. **Generate hook configs** for all detected enforced tools:
   - Claude Code: `.claude/settings.json` (SessionStart + PostToolUse hooks)
   - Cursor: `.cursor/hooks.json` + `.cursor/rules/keel.mdc`
   - Gemini CLI: `.gemini/settings.json` + `GEMINI.md`
   - Windsurf: `.windsurf/hooks.json` + `.windsurfrules`
   - Letta Code: Letta config with same hook semantics
6. **Generate instruction files** for all detected tools (see [[keel-speckit/009-tool-integration/spec|Spec 009]]).
7. **Append to `CLAUDE.md`** (create if absent) between `<!-- keel:start -->` / `<!-- keel:end -->` markers.
8. **Generate `.keelignore`** with sensible defaults: `generated/`, `vendor/`, `node_modules/`, `**/migrations/`, `dist/`, `build/`, `.next/`, `__pycache__/`. If `.keelignore` already exists, leave it untouched.
9. **Generate initial hash** for every function/class node.
10. **Install git pre-commit hook** in `.git/hooks/pre-commit`.
11. **Add `.gitignore` entries** for `.keel/graph.db` and `.keel/telemetry.db`.
12. **Output summary:** node count, edge count, external endpoints found, languages detected, tools configured, type hint coverage, docstring coverage.

**Auto-detection targets:**

- Languages: file extensions + config files
- LLM tools: scans for `.claude/`, `.cursor/`, `.gemini/`, `.windsurf/`, `.codex/`, `.agent/`, `GEMINI.md`, `.windsurfrules`, `.aider.conf.yml`, Letta config, `.github/copilot-instructions.md`
- Package manager: npm, pip, cargo, go
- Git configuration
- CI provider: `.github/workflows/` -> GitHub Actions template

**Config merge strategy:** When `keel init` runs in a project with existing tool configurations, keel merges its entries rather than overwriting:

- **JSON files:** Deep-merge keel hook entries into existing hooks arrays. Warn if a conflicting hook exists for the same event/matcher.
- **Markdown files:** Insert keel sections between `<!-- keel:start -->` / `<!-- keel:end -->` markers. If markers already exist, replace that section. If not, append to end.
- **TOML/YAML files:** Add keel-specific sections. Warn on key conflicts.
- **On conflict:** keel prints a warning and skips the conflicting entry, leaving the existing config intact. The developer resolves manually.

**Example output:**

```
keel v2.0.0 — initializing...

Detected languages:  TypeScript (847 files), Python (123 files)
Detected tools:      Claude Code, Cursor, Gemini CLI, Codex

Parsing codebase... done (3.2s)
  Functions: 1,247  Classes: 89  Modules: 64
  Call edges: 3,891  External endpoints: 23
  Type hint coverage: 78%  Docstring coverage: 62%

Generated:
  ✓ .keel/graph.db              (local graph — gitignored)
  ✓ .keel/manifest.json         (module summary — committed)
  ✓ .keel/config.toml           (team config — committed)
  ✓ .keel/hooks/post-edit.sh    (shared hook script)
  ✓ .claude/settings.json       (Claude Code — enforced hooks)
  ✓ CLAUDE.md                   (appended keel instructions)
  ✓ .cursor/hooks.json          (Cursor — enforced hooks)
  ✓ .cursor/rules/keel.mdc      (Cursor — supplementary rules)
  ✓ .gemini/settings.json       (Gemini CLI — enforced hooks)
  ✓ GEMINI.md                   (Gemini CLI — supplementary instructions)
  ✓ AGENTS.md                   (Codex — cooperative instructions)
  ✓ .git/hooks/pre-commit       (safety net)

⚠ 274 functions missing type hints (WARNING for existing code, ERROR for new code)
⚠ 89 public functions missing docstrings (WARNING for existing, ERROR for new)

Ready. 3 tools with enforced backpressure. 1 tool with cooperative enforcement.
```

---

### `keel map`

**Purpose:** Full re-map of the codebase. Used after major refactors, branch switches, or initial setup.

**Behavior:**

1. Re-parse all files (respecting `.keelignore` and `[exclude]` patterns from `config.toml`), rebuild graph from scratch.
2. Diff against previous graph and report: new nodes, removed nodes, changed signatures, broken edges.
3. Regenerate all hashes.
4. Output formats controlled by flags: `--json`, `--llm`, `--llm-verbose`, default (human CLI).

**Flags specific to `map`:**

| Flag | Description |
|------|-------------|
| `--llm` | Token-optimized LLM format |
| `--llm-verbose` | LLM format with full signatures |
| `--scope=<modules>` | Comma-separated module names for scoped maps |
| `--visual` | Generate static HTML visualization (Phase 2) |
| `--strict` | Exit non-zero on any ERROR-level violations |

**Performance targets:**

- <5 seconds for 100k LOC repo
- <30 seconds for 500k LOC repo
- Incremental updates via `compile` should be <200ms for single-file changes

---

### `keel discover <hash>`

**Purpose:** Given a function/class hash, return its upstream callers, downstream callees, and placement context. This is the "look before you leap" mechanism.

**Behavior:**

1. Return adjacency list: direct callers (upstream), direct callees (downstream).
2. Each entry includes: hash, function signature, file path relative to root, line number, docstring (first line).
3. Return **module context**: what module/file this function lives in, what other functions live in that module, what the module's responsibility is (derived from its functions and docstring).
4. Configurable depth: default 1 hop, max configurable via `config.toml` `[discovery]` section (default max: 5).

**Flags specific to `discover`:**

| Flag | Description |
|------|-------------|
| `--depth <N>` | Number of hops to traverse (default: 1) |
| `--suggest-placement` | Return top 3 modules where a function with given purpose would best fit |

**LLM usage pattern:** Before editing function X, the LLM calls `discover X` to understand what depends on X, what X depends on, and what module X lives in.

---

### `keel compile [file...]`

**Purpose:** Incrementally update the graph after a file change and validate structural integrity. This is the core enforcement command.

**Behavior:**

1. Re-parse changed file(s) only (tree-sitter incremental parsing).
2. Update affected nodes and edges in the graph.
3. Recompute hashes for changed functions.
4. **Validate adjacent contracts:**
   - If function signature changed (parameters, return type): check all callers still match -> `E001`
   - If function was removed: report all broken callers -> `E004`
   - If function was added: check for duplicate names in the codebase -> `W002`
   - If function parameter count changed, callers pass wrong number of arguments -> `E005`
5. **Enforce type hints:** Error if any function lacks type annotations -> `E002`
6. **Enforce docstrings:** Error if any public function lacks a docstring -> `E003`
7. **Validate placement:** If a new function was added, check if it belongs in its current module based on the module's existing responsibility pattern -> `W001`
8. Return: updated hashes, list of warnings/errors, affected downstream nodes.

**Clean compile behavior:** When compile passes with zero errors AND zero warnings: **exit 0, empty stdout**. No info block. The `info` block (`nodes_updated`, `edges_updated`, `hashes_changed`) is only emitted when `--verbose` is passed, or alongside errors/warnings. Info data is always written to `graph.db` and available via `keel stats`. This keeps the LLM's context window clean.

**Error severity levels:**

- `ERROR`: Callers will break (type mismatch, missing function, changed arity), missing type hints, missing docstrings on public functions. Blocks via hook / blocks commit via git hook.
- `WARNING`: Placement suggestion, potential naming issue, similar function exists elsewhere, cross-repo endpoint affected (Phase 2).
- `INFO`: Graph updated successfully, N nodes affected.

**Flags specific to `compile`:**

| Flag | Description |
|------|-------------|
| `--batch-start` | Begin batch mode — defer non-structural validations |
| `--batch-end` | End batch mode — fire all deferred validations |
| `--strict` | Treat warnings as errors |
| `--suppress <code>` | Suppress a specific error/warning code for this invocation |

**Batch mode:** When scaffolding multiple files, per-edit enforcement on incomplete code creates noise.

- `keel compile --batch-start` defers non-structural validations (type hints E002, docstrings E003, placement W001, duplicates W002) until `keel compile --batch-end`.
- Structural errors (broken callers E001, removed functions E004, arity mismatches E005) still fire immediately during batch mode.
- Batch auto-expires after 60 seconds of inactivity (no `compile` calls). Expiry triggers all deferred validations.
- Circuit breaker counters are paused during batch mode.

**Suppress mechanism:** Three suppression layers for false positives:

1. **Inline:** `# keel:suppress E001 — reason` on the line above the function. Suppresses the specific error code for that function.
2. **Config:** `[suppress]` section in `.keel/config.toml` for persistent suppressions with required reason field.
3. **CLI:** `keel compile --suppress W001` to suppress a code for a single invocation.

Suppressed violations are downgraded to `INFO` severity (code S001) and remain visible in `--json` output and telemetry. They are never silently hidden.

**Dynamic dispatch note:** Call edges resolved with low confidence (dynamic dispatch, trait/interface methods, untyped method calls) are enforced at `WARNING`, not `ERROR`. This prevents false positives from blocking the LLM on ambiguous resolutions. Tier 3 (LSP/SCIP) can promote these to `ERROR`.

**Type hint enforcement per language:**

- **TypeScript, Go, Rust:** Already typed. keel validates signature changes against callers using existing type information.
- **Python:** Requires type hints on all parameters and return types. `def process(data)` -> `ERROR`. `def process(data: dict[str, Any]) -> ProcessResult` -> passes.
- **JavaScript:** Requires JSDoc `@param` and `@returns` annotations. The LLM is instructed to prefer TypeScript for new files.

---

### `keel where <hash>`

**Purpose:** Resolve a hash to a file location.

**Behavior:**

1. Return: file path relative to project root, start line, end line.
2. If hash is stale (function was modified since hash was generated): return location with `STALE` flag and suggest re-running `compile`.

---

### `keel explain <error-code> <hash>`

**Purpose:** Expose keel's resolution reasoning so the LLM can diagnose false positives. This is the "show your work" command.

**Behavior:**

1. Takes an error code (e.g., `E001`) and the function hash keel flagged.
2. Returns the **resolution chain**: the concrete evidence keel used to determine the dependency:
   - Import statement at file:line
   - Call expression at file:line
   - Type reference at file:line
   - Re-export chain (if dependency resolved through re-exports)
3. Shows **confidence score** (0.0-1.0) for the resolution.
4. Shows **resolution tier** that produced the evidence: `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip`.
5. Output: JSON for LLM consumption (default), human-readable tree with `--tree`.

**Flags specific to `explain`:**

| Flag | Description |
|------|-------------|
| `--tree` | Human-readable tree output instead of JSON |

**LLM usage pattern:** Used at circuit breaker attempt 3 — when repeated fixes haven't resolved an error, the LLM inspects keel's reasoning to determine if the error is a false positive or if the fix strategy is wrong.

---

### `keel deinit`

**Purpose:** Cleanly remove all keel-generated files and configurations from a project.

**Behavior:**

1. Remove `.keel/` directory (`graph.db`, `manifest.json`, `hooks/`, `telemetry.db`).
2. Remove keel sections from all tool configs:
   - `.claude/settings.json` — remove keel hook entries
   - `CLAUDE.md` — remove content between `<!-- keel:start -->` / `<!-- keel:end -->`
   - `.cursor/hooks.json` — remove keel hook entries
   - `.cursor/rules/keel.mdc` — remove file
   - `.gemini/settings.json` — remove keel hook entries
   - `GEMINI.md` — remove keel sections
   - `.windsurf/hooks.json` — remove keel hook entries
   - `.windsurfrules` — remove keel sections
   - `AGENTS.md` — remove keel sections
   - `.agent/rules/keel.md` — remove file
   - `.agent/skills/keel/` — remove directory
   - Letta config — remove keel entries
   - `.github/copilot-instructions.md` — remove keel sections
3. Remove pre-commit hook (or keel's section from it if other hooks exist).
4. **Preserve `.keel/config.toml`** so `keel init` can re-initialize with same settings.
5. Report what was removed.

---

### `keel serve`

**Purpose:** Run a persistent local server exposing keel commands via MCP (stdio), HTTP, or file watcher. Full transport specification in [[keel-speckit/010-mcp-http-server/spec|Spec 010]].

**Modes:**

| Flag | Description |
|------|-------------|
| `--mcp` | MCP over stdio. Integrates with Claude Code, Cursor, Antigravity, Codex, any MCP client. |
| `--http` | HTTP API on `localhost:4815`. Powers the VS Code extension. |
| `--watch` | File system watcher. Auto-runs `compile` on file save. Combines with `--mcp` or `--http`. |

**Behavior:**

- Wraps all CLI commands as server endpoints/tools.
- Holds graph in memory for sub-millisecond responses (vs. CLI's load-from-SQLite-per-call).
- Watches file system for changes and auto-runs `compile` (with `--watch`).
- Implementation: thin wrapper (~300-500 lines) over the core library. No new logic — just transport.

**Memory footprint:**

- ~50-100MB for a 50k LOC repo
- ~200-400MB for a 200k LOC repo
- CLI mode loads a subgraph from SQLite per call and uses ~20-50MB

---

### `keel stats`

**Purpose:** Display telemetry dashboard from local data. Shows a summary of keel's activity and effectiveness.

**Behavior:**

1. Reads from `.keel/telemetry.db` (SQLite, gitignored).
2. Displays per-session and per-week aggregate metrics.

**Example output:**

```
keel stats — last 7 days

Errors caught:        47  (12 broken callers, 23 missing types, 12 missing docs)
Errors resolved:      45  (95.7% — LLM fixed after backpressure)
False positives:       2  (4.3%)
Discover calls:       89  (LLM checked adjacency before 89 edits)
Compile invocations: 312
Explain calls:        3  (LLM inspected resolution chains)
Circuit breaker:      5 escalations, 1 downgrade
Map token usage:    8,247 tokens (4.1% of 200k context)
```

**Metrics tracked:**

- `errors_caught` — count of ERROR-level violations caught by `compile`
- `warnings_issued` — count of WARNING-level violations
- `errors_resolved` — count of errors the LLM fixed after being blocked
- `false_positives_dismissed` — count of warnings/errors overridden
- `compile_invocations` — how many times `compile` ran
- `discover_invocations` — how many times `discover` was called
- `explain_invocations` — how many times `explain` was called
- `circuit_breaker_escalations` — count of times same error hit attempt 2
- `circuit_breaker_downgrades` — count of times same error hit attempt 3
- `map_token_count` — tokens used by the `--llm` map for this session

**Per-week aggregates:**

- `error_catch_rate` — errors caught / total edits
- `false_positive_rate` — dismissed warnings / total warnings
- `backpressure_effectiveness` — errors resolved by LLM / errors caught
- `map_utilization` — discover calls / total editing sessions

---

## Circuit Breaker / Escalation Design

keel tracks consecutive failed compiles per function/error-code pair. In `keel serve` mode, this is session state in memory. In CLI mode, state is stored in `.keel/session.json` (temp file, gitignored).

**Escalation sequence:**

1. **Attempt 1 fails:** Normal error output + `fix_hint` in JSON response. The fix hint is a simple text instruction: *"Update 3 callers to pass Password instead of string: handleLogin at src/routes/auth.ts:23, autoLogin at src/middleware/session.ts:88, testLogin at tests/auth.test.ts:23."*

2. **Attempt 2 fails (same error-code + hash):** Error output + fix hint + **escalation instruction**: *"Run `keel discover <hash> --depth 2` to inspect the wider dependency chain. The issue may be upstream of the direct callers."*

3. **Attempt 3 fails (same error-code + hash):** **Auto-downgrade to WARNING** for this session. Instruction: *"Run `keel explain <error-code> <hash>` to inspect the resolution chain. Add findings as a code comment so the next session can resolve with full context."*

**Design decisions:**

- 3 retries regardless of confidence score. Simple — instructions handle nuance, not retry logic.
- Downgraded errors re-enforce on next session. The downgrade is a session-scoped escape valve, not a permanent suppression.
- Counter resets on success. Only consecutive failures on same error-code + hash pair escalate.
- Batch mode interaction: circuit breaker counters are paused during `--batch-start` / `--batch-end`.
- Configurable via `[circuit_breaker]` in `.keel/config.toml` (`max_retries = 3`, `auto_downgrade = true`).

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success, no violations |
| `1` | Violations found (errors or warnings, depending on `--strict`) |
| `2` | keel internal error (graph corrupt, parse failure) |

---

## Colored Output

- Errors in red
- Warnings in yellow
- Info in dim
- Respects `NO_COLOR` environment variable
- Respects `TERM=dumb`

---

## Inter-Agent Contracts

### Exposed by this spec (Agent B -> Agent C):

**CLI command dispatch:** All commands are implemented as functions callable from both the CLI binary and the server layer. Agent C's `keel serve` wraps these same functions — no re-implementation.

```rust
pub trait KeelCommands {
    fn init(&self, opts: InitOptions) -> Result<InitReport, KeelError>;
    fn map(&self, opts: MapOptions) -> Result<MapOutput, KeelError>;
    fn discover(&self, hash: &str, opts: DiscoverOptions) -> Result<DiscoverOutput, KeelError>;
    fn compile(&self, files: &[&str], opts: CompileOptions) -> Result<CompileOutput, KeelError>;
    fn r#where(&self, hash: &str) -> Result<WhereOutput, KeelError>;
    fn explain(&self, error_code: &str, hash: &str) -> Result<ExplainOutput, KeelError>;
    fn deinit(&self) -> Result<DeinitReport, KeelError>;
    fn stats(&self, opts: StatsOptions) -> Result<StatsOutput, KeelError>;
}
```

### Consumed by this spec:

- **[[keel-speckit/000-graph-schema/spec|Spec 000]]** — `GraphStore` trait for all graph operations
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006]]** — enforcement engine for `compile` validation logic

---

## Acceptance Criteria

**GIVEN** a fresh TypeScript + Python repository with no `.keel/` directory
**WHEN** `keel init` is run
**THEN** `.keel/graph.db`, `.keel/manifest.json`, `.keel/config.toml`, `.keel/hooks/post-edit.sh` are created, languages are auto-detected, LLM tools are auto-detected, hook configs are generated for detected Tier 1 tools, instruction files are generated for all detected tools, and exit code is 0.

**GIVEN** an existing `.claude/settings.json` with user-defined hooks
**WHEN** `keel init` is run
**THEN** keel deep-merges its hook entries alongside existing hooks without overwriting them. If a conflicting hook exists, keel warns and skips.

**GIVEN** an initialized keel project
**WHEN** `keel map` is run
**THEN** the full graph is rebuilt, a diff is reported against the previous graph (new/removed/changed nodes), all hashes are regenerated, and exit code is 0.

**GIVEN** a function with 3 upstream callers
**WHEN** `keel discover <hash>` is run
**THEN** the output contains all 3 callers with hash, signature, file path, line number, and docstring. Module context is included.

**GIVEN** a file edit that changes a function's parameter type from `string` to `Password`
**WHEN** `keel compile <file> --json` is run
**THEN** an `E001` error is returned listing all affected callers with file:line locations and a `fix_hint` describing the required changes.

**GIVEN** a compile with zero errors and zero warnings
**WHEN** `keel compile <file>` is run (without `--verbose`)
**THEN** stdout is empty and exit code is 0.

**GIVEN** a compile with zero errors and zero warnings
**WHEN** `keel compile <file> --verbose` is run
**THEN** the info block with `nodes_updated`, `edges_updated`, `hashes_changed` is emitted.

**GIVEN** a modified function whose hash has changed
**WHEN** `keel where <old_hash>` is run
**THEN** the location is returned with a `STALE` flag and a suggestion to re-run `compile`.

**GIVEN** an `E001` error on a function
**WHEN** `keel explain E001 <hash>` is run
**THEN** the resolution chain is returned with import/call/type_ref evidence, confidence score, and resolution tier.

**GIVEN** an initialized keel project
**WHEN** `keel deinit` is run
**THEN** `.keel/` directory is removed (except `config.toml`), keel sections are removed from all tool configs, pre-commit hook is removed, and a report of removed items is printed.

**GIVEN** batch mode started with `keel compile --batch-start`
**WHEN** files are compiled with type hint violations during batch mode
**THEN** E002 errors are deferred until `keel compile --batch-end` is called. Structural errors (E001, E004, E005) still fire immediately.

**GIVEN** batch mode started with `keel compile --batch-start` and 60 seconds pass with no `compile` calls
**WHEN** the auto-expiry triggers
**THEN** all deferred validations fire as if `--batch-end` were called.

---

## Test Strategy

**Oracle:** Command behavior correctness.
- Verify every command produces correct output for valid inputs.
- Verify every command produces correct error output for invalid inputs.
- Verify exit codes match the specification.
- Verify JSON output validates against schemas in [[keel-speckit/008-output-formats/spec|Spec 008]].

**Test files to create:**
- `tests/cli/test_init.rs` (~10 tests)
- `tests/cli/test_init_merge.rs` (~8 tests) — config merge behavior
- `tests/cli/test_map.rs` (~6 tests)
- `tests/cli/test_discover.rs` (~6 tests)
- `tests/cli/test_compile.rs` (~8 tests)
- `tests/cli/test_compile_batch.rs` (~5 tests)
- `tests/cli/test_where.rs` (~4 tests)
- `tests/cli/test_explain.rs` (~4 tests)
- `tests/cli/test_deinit.rs` (~4 tests)
- `tests/cli/test_stats.rs` (~3 tests)
- `tests/cli/test_exit_codes.rs` (~4 tests)

**Estimated test count:** ~62

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| Config merge on `init` produces broken configs | Test against real-world `.claude/settings.json` and `.cursor/hooks.json` from popular projects. Warn and skip on conflict. |
| `deinit` removes user-written content in shared files | Use `<!-- keel:start -->` / `<!-- keel:end -->` markers. Only remove content between markers. |
| Batch mode auto-expiry fires during long LLM operations | 60s timeout is generous. If problematic, make configurable in `config.toml`. |
| Circuit breaker state lost between CLI invocations | State stored in `.keel/session.json`. Stale sessions cleaned on new `keel serve` start. |
| `clap` breaking changes | Pin `clap` version in `Cargo.toml`. |

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures for all graph operations
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — validation logic used by `compile`
- [[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]] — JSON/LLM/CLI format specifications
- [[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]] — hook configs and instruction files generated by `init`
- [[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]] — transport layer for `keel serve`
