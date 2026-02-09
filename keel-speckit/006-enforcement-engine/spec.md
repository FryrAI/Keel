# Spec 006: Enforcement Engine — Compile + All Validation

```yaml
tags: [keel, spec, enforcement, compile, validation]
owner: Agent B (Enforcement)
dependencies: [000-graph-schema, 001-treesitter-foundation]
prd_sections: [4.4, 4.9, 4.10, 8, 12, 13]
priority: P0 — highest-value spec, core differentiator
```

## Summary

This spec defines keel's enforcement engine — the `compile` command that validates structural integrity after every edit, the `explain` command that exposes resolution reasoning, and the circuit breaker that prevents LLM retry loops. This is keel's core differentiator: the backpressure mechanism that forces LLMs to verify before and validate after every code change.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| §4.4 | `compile` command — incremental parsing, validation, error severity, batch mode, suppress mechanism, dynamic dispatch, type hint enforcement per language |
| §4.9 | `explain` command — resolution chain, confidence score, resolution tier, JSON output |
| §4.10 | Circuit breaker — escalation sequence, tracking, design decisions, configuration |
| §8 | Code placement — module responsibility profiles, placement scoring, known limitations |
| §12 | JSON output schemas — compile errors, explain output, error codes, common fields |
| §13 | Configuration schema — enforcement settings, circuit breaker config, suppress config, overrides |

---

## Compile Command (`keel compile [file...]`)

### Behavior (sequential steps)

1. **Re-parse changed file(s) only** — tree-sitter incremental parsing
2. **Update affected nodes and edges** in the graph
3. **Recompute hashes** for changed functions
4. **Validate adjacent contracts:**
   - If function signature changed (parameters, return type): check all callers still match
   - If function was removed: report all broken callers
   - If function was added: check for duplicate names in the codebase
5. **Enforce type hints:** Error if any function lacks type annotations
6. **Enforce docstrings:** Error if any public function lacks a docstring
7. **Validate placement:** If new function added, check if it belongs in current module based on module's responsibility pattern
8. **Return:** updated hashes, list of warnings/errors, affected downstream nodes

### Clean Compile Behavior

When compile passes with zero errors AND zero warnings:
- Exit code: `0`
- stdout: **empty** (no output at all)
- The `info` block (`nodes_updated`, `edges_updated`, `hashes_changed`) is only emitted when `--verbose` is passed, or alongside errors/warnings
- Info data is always written to `graph.db` and available via `keel stats`
- This keeps the LLM's context window clean

### Error Severity Levels

| Level | Meaning | Hook Behavior |
|-------|---------|---------------|
| `ERROR` | Callers will break. Missing type hints. Missing docstrings on public functions. | Blocks via hook (exit 2). Blocks commit via git hook. |
| `WARNING` | Placement suggestion. Naming issue. Similar function exists. Dynamic dispatch ambiguity. | Shown but does not block. |
| `INFO` | Graph updated successfully. N nodes affected. Suppressed violations. | Only with `--verbose` or when errors/warnings present. |

### Error Codes (Frozen)

| Code | Category | Severity | Description | fix_hint Required |
|------|----------|----------|-------------|-------------------|
| E001 | broken_caller | ERROR | Function signature changed, callers expect old signature | Always |
| E002 | missing_type_hints | ERROR | Function parameters or return type lack type annotations | Always |
| E003 | missing_docstring | ERROR | Public function has no docstring | Always |
| E004 | function_removed | ERROR | Function deleted but still has callers | Always |
| E005 | arity_mismatch | ERROR | Parameter count changed, callers pass wrong argument count | Always |
| W001 | placement | WARNING | Function may be better placed in different module | Where applicable |
| W002 | duplicate_name | WARNING | Function with same name exists elsewhere | Where applicable |
| S001 | suppressed | INFO | Violation suppressed via inline or config | N/A |

### Common Fields on All Error/Warning Objects

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `confidence` | float 0.0-1.0 | Always | Resolution confidence. 1.0 = certain (tree-sitter found the node). <0.7 = heuristic/ambiguous |
| `resolution_tier` | string enum | Always | Which tier produced the evidence: `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip` |
| `fix_hint` | string | ERROR: always. WARNING: where applicable | Simple text instruction telling the LLM what to do. Not structured transforms — human/LLM-readable description of the fix |

### Compile Error JSON Output

This is the schema the LLM sees via stderr when the PostToolUse hook fires with exit code 2:

```json
{
  "version": "1.0",
  "command": "compile",
  "status": "error",
  "files_analyzed": ["src/auth/login.ts"],
  "errors": [
    {
      "code": "E001",
      "severity": "ERROR",
      "category": "broken_caller",
      "message": "Function 'login' signature changed: parameter 'password' type changed from 'string' to 'Password'. 3 callers expect 'string'.",
      "file": "src/auth/login.ts",
      "line": 42,
      "hash": "xK2p9Lm4Q",
      "confidence": 0.94,
      "resolution_tier": "tier2_oxc",
      "fix_hint": "Update 3 callers to pass Password instead of string: handleLogin at src/routes/auth.ts:23, autoLogin at src/middleware/session.ts:88, testLogin at tests/auth.test.ts:23",
      "suppressed": false,
      "suppress_hint": "# keel:suppress E001 — if this signature change is intentional and callers will be updated separately",
      "affected": [
        {"hash": "mN7rT2wYs", "name": "handleLogin", "file": "src/routes/auth.ts", "line": 15},
        {"hash": "pQ4sV8nXe", "name": "autoLogin", "file": "src/middleware/session.ts", "line": 88},
        {"hash": "cT5nP2rYu", "name": "testLogin", "file": "tests/auth.test.ts", "line": 23}
      ]
    },
    {
      "code": "E002",
      "severity": "ERROR",
      "category": "missing_type_hints",
      "message": "Function 'processData' missing type annotations. Parameters 'data', 'options' and return type are untyped.",
      "file": "src/auth/login.ts",
      "line": 67,
      "hash": "dU6oQ3sZv",
      "confidence": 1.0,
      "resolution_tier": "tier1_treesitter",
      "fix_hint": "Add type annotations to parameters 'data', 'options' and return type for function 'processData'",
      "affected": []
    },
    {
      "code": "E003",
      "severity": "ERROR",
      "category": "missing_docstring",
      "message": "Public function 'validateToken' has no docstring.",
      "file": "src/auth/login.ts",
      "line": 89,
      "hash": "eV7pR4tAw",
      "confidence": 1.0,
      "resolution_tier": "tier1_treesitter",
      "fix_hint": "Add a docstring to public function 'validateToken'",
      "affected": []
    }
  ],
  "warnings": [
    {
      "code": "W001",
      "severity": "WARNING",
      "category": "placement",
      "message": "Function 'calculateShipping' may belong in module 'shipping' (contains: calculateRate, getCarrier, estimateDelivery) rather than 'checkout'.",
      "file": "src/checkout/utils.ts",
      "line": 120,
      "hash": "fW8qS5uBx",
      "confidence": 0.72,
      "resolution_tier": "tier2_treesitter_heuristic",
      "fix_hint": "Move 'calculateShipping' to src/shipping/ where related functions calculateRate, getCarrier, estimateDelivery already live",
      "suggested_module": "src/shipping/"
    },
    {
      "code": "W002",
      "severity": "WARNING",
      "category": "duplicate_name",
      "message": "Function 'formatDate' already exists in 'src/utils/date.ts:14'. Consider reusing it.",
      "file": "src/checkout/helpers.ts",
      "line": 5,
      "hash": "gX9rT6vCy",
      "confidence": 0.85,
      "resolution_tier": "tier1_treesitter",
      "fix_hint": "Remove duplicate and import existing 'formatDate' from src/utils/date.ts:14 instead",
      "existing": {"hash": "hY0sU7wDz", "file": "src/utils/date.ts", "line": 14}
    }
  ],
  "info": {
    "nodes_updated": 3,
    "edges_updated": 7,
    "hashes_changed": ["xK2p9Lm4Q", "dU6oQ3sZv", "eV7pR4tAw"]
  }
}
```

### Type Hint Enforcement Per Language

| Language | Enforcement Rule |
|----------|-----------------|
| TypeScript, Go, Rust | Already typed. Validate signature changes against callers using existing type info. |
| Python | Requires type hints on all parameters and return types. `def process(data)` = ERROR. `def process(data: dict[str, Any]) -> ProcessResult` = passes. |
| JavaScript | Requires JSDoc `@param` and `@returns` annotations. LLM instructed to prefer TypeScript for new files. |

### Progressive Adoption

| Code Type | New/Modified Code | Pre-existing Code |
|-----------|------------------|-------------------|
| Type hints | `ERROR` | `WARNING` (configurable escalation to `ERROR`) |
| Docstrings | `ERROR` | `WARNING` (configurable escalation to `ERROR`) |
| Placement | `WARNING` | `WARNING` |

Controlled by `[enforcement]` section in `.keel/config.toml`.

### Dynamic Dispatch

Call edges resolved with low confidence (dynamic dispatch, trait/interface methods, untyped method calls) are enforced at `WARNING`, not `ERROR`. This prevents false positives from blocking the LLM on ambiguous resolutions.

When Tier 3 (LSP/SCIP) is enabled, it can promote these to `ERROR` by confirming the resolution with full type information.

---

## Batch Mode (`--batch-start` / `--batch-end`)

For scaffolding sessions where per-edit enforcement on incomplete code creates noise.

**`keel compile --batch-start`:** Defers non-structural validations until `--batch-end`:
- **Still fires immediately:** E001 (broken callers), E004 (removed functions), E005 (arity mismatches) — structural errors that compound if left unfixed
- **Deferred to batch-end:** E002 (type hints), E003 (docstrings), W001 (placement), W002 (duplicate names) — quality checks that only make sense on completed code

**`keel compile --batch-end`:** All deferred validations fire now.

**Auto-expiry:** Batch mode expires after 60 seconds of inactivity (no `compile` calls). Expiry triggers all deferred validations.

**Circuit breaker interaction:** Circuit breaker counters are paused during batch mode. Deferred validation failures at `--batch-end` do not count as consecutive failures for escalation.

---

## Suppress Mechanism

Three suppression layers for false positives:

1. **Inline:** `# keel:suppress E001 — reason` on the line above the function. Suppresses specific error code for that function.
2. **Config:** `[suppress]` section in `.keel/config.toml`:
   ```toml
   [suppress]
   "src/legacy/adapter.ts:validateInput" = { codes = ["W001"], reason = "Adapter pattern — intentionally cross-cutting" }
   "src/utils/index.ts:*" = { codes = ["W001"], reason = "Utility barrel file — placement scoring not meaningful" }
   ```
   Each entry requires a `reason` field — unexplained suppressions are rejected.
3. **CLI:** `keel compile --suppress W001` for single invocation (useful during exploration).

**Behavior:** Suppressed violations are downgraded to `INFO` severity (code S001) and remain visible in `--json` output. They are never silently hidden.

---

## Code Placement Scoring (PRD §8)

### How It Works

**During `keel init` / `keel map`:**
1. Build module responsibility profile for each directory/module from: module name/path, functions it contains, import/export patterns, external endpoints
2. Store profile in `graph.db` as `ModuleProfile`

**During `keel compile` (new function added):**
1. Compute placement score — does new function align with module it was placed in?
2. Scoring heuristic (pure structural, not ML-based):
   - Does the function call other functions in this module? (+score)
   - Do other functions in this module call it? (+score)
   - Does the function's name share a prefix/domain with sibling functions? (+score)
   - Does the function import types from different modules than its siblings? (-score)
   - Is there another module where this function would score higher? (-> WARNING with suggestion)
3. If below threshold: emit `W001` with suggested module

### Known Limitations (Honest Assessment)

- **Utility modules** (`utils/`, `helpers/`, `common/`): Scoring is weak — inherently cross-cutting. Mitigation: configurable exclusions via `[exclude]` patterns.
- **Facades and orchestrators:** Modules calling many modules score ambiguously. Can't distinguish "correctly orchestrating" from "misplaced."
- **Small modules (<5 functions):** Not enough signal for meaningful profile. Scoring unreliable.
- **Realistic false positive rate:** 15-25% overall on correctly-placed functions. 5-10% on well-structured code with clear domain boundaries. Higher on utility-heavy codebases. WARNING-level by design.

---

## Circuit Breaker / Escalation (PRD §4.10)

### Purpose

Prevent the LLM from retrying the same failing fix endlessly. Convert a frustrating loop into a progression: fix -> investigate wider -> inspect reasoning -> document and move on.

### Tracking

Per function/error-code pair. In `keel serve` mode: session state in memory. In CLI mode: stored in `.keel/session.json` (temp file, gitignored).

### Escalation Sequence

**Attempt 1 fails:** Normal error output + `fix_hint` in JSON response.

**Attempt 2 fails (same error-code + hash):** Error output + fix hint + escalation instruction:
> *"Run `keel discover <hash> --depth 2` to inspect the wider dependency chain. The issue may be upstream of the direct callers."*

**Attempt 3 fails (same error-code + hash):** **Auto-downgrade to WARNING** for this session. Instruction:
> *"Run `keel explain <error-code> <hash>` to inspect the resolution chain. Add findings as a code comment so the next session can resolve with full context."*

### Design Decisions

- **3 retries regardless of confidence score.** Keep it simple — instructions handle nuance.
- **Downgraded errors re-enforce on next session.** Session-scoped escape valve, not permanent suppression.
- **Counter resets on success.** Fix on attempt 2 = counter resets. Only consecutive failures escalate.
- **Batch mode interaction:** Counters paused during `--batch-start` / `--batch-end`.
- **Configuration:** `[circuit_breaker]` in `.keel/config.toml`:
  ```toml
  [circuit_breaker]
  max_retries = 3
  auto_downgrade = true
  ```

---

## Explain Command (`keel explain <error-code> <hash>`)

### Behavior

Takes an error code and function hash. Returns the resolution chain — the concrete evidence keel used to determine the dependency.

### JSON Output

```json
{
  "version": "1.0",
  "command": "explain",
  "error_code": "E001",
  "hash": "xK2p9Lm4Q",
  "confidence": 0.94,
  "resolution_tier": "tier2_oxc",
  "resolution_chain": [
    {"kind": "import", "file": "src/routes/auth.ts", "line": 3, "text": "import { login } from '../auth/login'"},
    {"kind": "call", "file": "src/routes/auth.ts", "line": 23, "text": "const token = login(email, password)"},
    {"kind": "type_ref", "file": "src/routes/auth.ts", "line": 23, "text": "password: string (expected Password)"}
  ],
  "summary": "Edge resolved via static import + direct call expression. High confidence — oxc confirmed the call site and argument types."
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `error_code` | string | Always | The error code being explained |
| `hash` | string | Always | The function hash keel flagged |
| `confidence` | float 0.0-1.0 | Always | Resolution confidence |
| `resolution_tier` | string enum | Always | Which tier produced the evidence |
| `resolution_chain` | array | Always | Ordered evidence steps. Each: `kind` (import/call/type_ref/re_export), `file`, `line`, `text` |
| `summary` | string | Always | Human/LLM-readable summary of resolution reasoning |

---

## Configuration Schema (relevant sections)

```toml
[enforcement]
type_hints = "error"                    # New/modified code
type_hints_existing = "warning"         # Pre-existing code
docstrings = "error"                    # New/modified public functions
docstrings_existing = "warning"         # Pre-existing public functions
docstring_format = "first_line"         # "first_line" | "full"
placement = "warning"                   # "warning" | "off"
duplicate_detection = "warning"         # "warning" | "off"

[enforcement.overrides]
"tests/**" = { type_hints = "warning", docstrings = "off" }
"scripts/**" = { type_hints = "warning", docstrings = "off" }
"fixtures/**" = { type_hints = "off", docstrings = "off", placement = "off" }

[circuit_breaker]
max_retries = 3
auto_downgrade = true

[suppress]
# "src/legacy/adapter.ts:validateInput" = { codes = ["W001"], reason = "Adapter pattern" }
```

---

## Inter-Agent Contracts

### Exposed by this spec (Agent B -> Agent C):

```rust
pub struct CompileResult {
    pub version: String,              // "1.0"
    pub command: String,              // "compile"
    pub status: String,               // "ok" | "error" | "warning"
    pub files_analyzed: Vec<String>,
    pub errors: Vec<Violation>,
    pub warnings: Vec<Violation>,
    pub info: CompileInfo,
}

pub struct Violation {
    pub code: String,                 // "E001", "W001", etc.
    pub severity: String,             // "ERROR", "WARNING", "INFO"
    pub category: String,             // "broken_caller", "placement", etc.
    pub message: String,
    pub file: String,
    pub line: u32,
    pub hash: String,
    pub confidence: f64,
    pub resolution_tier: String,
    pub fix_hint: Option<String>,
    pub suppressed: bool,
    pub suppress_hint: Option<String>,
    pub affected: Vec<AffectedNode>,
    pub suggested_module: Option<String>,  // W001 only
    pub existing: Option<ExistingNode>,    // W002 only
}

pub struct AffectedNode {
    pub hash: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

pub struct CompileInfo {
    pub nodes_updated: u32,
    pub edges_updated: u32,
    pub hashes_changed: Vec<String>,
}

pub struct DiscoverResult {
    pub version: String,
    pub command: String,
    pub target: NodeInfo,
    pub upstream: Vec<CallerInfo>,
    pub downstream: Vec<CalleeInfo>,
    pub module_context: ModuleContext,
}

pub struct ExplainResult {
    pub version: String,
    pub command: String,
    pub error_code: String,
    pub hash: String,
    pub confidence: f64,
    pub resolution_tier: String,
    pub resolution_chain: Vec<ResolutionStep>,
    pub summary: String,
}

pub struct ResolutionStep {
    pub kind: String,    // "import", "call", "type_ref", "re_export"
    pub file: String,
    pub line: u32,
    pub text: String,
}
```

### Dependencies from other specs:

- **Spec 000** (`GraphStore` trait) — reads nodes, edges, module profiles
- **Spec 001** (`LanguageResolver` trait) — triggers incremental parsing on compile

---

## Acceptance Criteria

**GIVEN** a file where a function's parameter type was changed
**WHEN** `keel compile <file> --json` is run
**THEN** E001 is returned with all affected callers listed in `affected[]`, each with hash, name, file, line

**GIVEN** a file with a new function missing type annotations
**WHEN** `keel compile <file> --json` is run
**THEN** E002 is returned with `confidence: 1.0` and `resolution_tier: "tier1_treesitter"`

**GIVEN** a public function without a docstring
**WHEN** `keel compile <file> --json` is run
**THEN** E003 is returned with `fix_hint` containing the function name

**GIVEN** a file where a function was deleted that has 2 callers
**WHEN** `keel compile <file> --json` is run
**THEN** E004 is returned with both callers in `affected[]`

**GIVEN** a function whose arity changed from 2 to 3 params
**WHEN** `keel compile <file> --json` is run
**THEN** E005 is returned with all callers that pass the wrong argument count

**GIVEN** a new function placed in module `checkout` that scores higher in module `shipping`
**WHEN** `keel compile <file> --json` is run
**THEN** W001 is returned with `suggested_module: "src/shipping/"` and relevant sibling functions listed

**GIVEN** a compile that passes with zero errors and zero warnings
**WHEN** `keel compile <file>` is run (without --verbose)
**THEN** stdout is empty and exit code is 0

**GIVEN** compile is run with `--verbose` and zero violations
**WHEN** output is checked
**THEN** the `info` block is present with `nodes_updated`, `edges_updated`, `hashes_changed`

**GIVEN** `keel compile --batch-start` was called
**WHEN** a file with missing type hints is compiled
**THEN** E002 is NOT emitted (deferred)
**AND WHEN** `keel compile --batch-end` is called
**THEN** E002 is emitted for the deferred file

**GIVEN** the same E001 error on the same hash fails 3 consecutive times
**WHEN** the 3rd compile returns
**THEN** the error is downgraded to WARNING severity for this session
**AND** the instruction says to run `keel explain`

**GIVEN** a function with `# keel:suppress E001 -- intentional migration` on the line above
**WHEN** `keel compile <file> --json` is run
**THEN** the violation appears as S001 (INFO severity) with `suppressed: true`

**GIVEN** `keel explain E001 xK2p9Lm4Q` is called
**WHEN** the resolution chain exists
**THEN** JSON output includes `resolution_chain` array with import, call, and/or type_ref steps

---

## Test Strategy

**Oracle:** Mutation testing (Oracle 2 from [[design-principles#Principle 1 The Verifier Is King|Principle 1]])

**Mutation test script:** Automatically introduce breaking changes to test corpus code:
- Rename parameter -> verify caller mismatch detected (E001)
- Change return type -> verify type contract violation detected (E001)
- Remove function -> verify all broken callers reported (E004)
- Add function without type hints -> verify enforcement triggers (E002)
- Add function without docstring -> verify enforcement triggers (E003)
- Change arity -> verify arity mismatch detected (E005)
- Add function in wrong module -> verify placement warning triggers (W001)

**Test files to create:**
- `tests/enforcement/test_broken_callers.rs` (~15 tests)
- `tests/enforcement/test_type_hints.rs` (~10 tests)
- `tests/enforcement/test_docstrings.rs` (~10 tests)
- `tests/enforcement/test_placement.rs` (~12 tests)
- `tests/enforcement/test_duplicate_detection.rs` (~8 tests)
- `tests/enforcement/test_circuit_breaker.rs` (~10 tests)
- `tests/enforcement/test_batch_mode.rs` (~8 tests)
- `tests/enforcement/test_suppress.rs` (~8 tests)
- `tests/enforcement/test_explain.rs` (~8 tests)
- `tests/enforcement/test_clean_compile.rs` (~5 tests)
- `tests/enforcement/test_progressive_adoption.rs` (~8 tests)

**Estimated test count:** ~102

---

## Known Risks

| Risk | Severity | Mitigation |
|------|----------|-----------|
| False positives in caller validation with dynamic dispatch | Medium | WARNING not ERROR for low-confidence edges. Tier 3 promotes. |
| Placement scoring unreliable on utility modules | Medium | Configurable exclusions. WARNING-only. 15-25% FP rate acceptable. |
| Circuit breaker counter persistence across CLI invocations | Low | `.keel/session.json` — temp file, cleaned on next session start |
| Batch mode auto-expiry race condition | Low | 60s is generous. Log warning at expiry. |
| Suppress syntax conflicts with existing comment styles | Low | `# keel:suppress` prefix is unique enough. Test across 4 languages. |

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — data structures this engine reads/writes
- [[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]] — parser this engine invokes
- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — wraps this engine in CLI interface
- [[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]] — serializes this engine's results
- [[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]] — hooks that invoke this engine
