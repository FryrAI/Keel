# Spec 008: Output Formats — JSON, LLM, and CLI Output

```yaml
tags: [keel, spec, output-formats, json, llm, cli]
owner: Agent B (Enforcement)
dependencies:
  - "[[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]]"
  - "[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]"
prd_sections: [6, 12]
priority: P0 — all tool integrations and LLM interactions depend on output format correctness
```

## Summary

This spec defines all output formats keel produces: structured `--json` for programmatic consumption, token-optimized `--llm` for LLM context injection, and default human-readable CLI output. It includes the full JSON schemas for every command, the error codes table, common fields on all error/warning objects, the LLM format design with token budgets, and clean compile behavior. Every output format defined here is consumed by [[keel-speckit/009-tool-integration/spec|Spec 009]] (hooks read JSON), [[keel-speckit/010-mcp-http-server/spec|Spec 010]] (server returns these formats), and [[keel-speckit/011-vscode-extension/spec|Spec 011]] (extension displays diagnostics from JSON).

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 6 | Output format design: `--json`, `--llm`, default CLI. LLM format principles, token budget, scoped maps, `--llm-verbose` |
| 12 | Full JSON output schemas: compile errors, discover, map, explain. Error codes table. Common fields (confidence, resolution_tier, fix_hint) |

---

## Dependencies

- **[[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]]** — JSON schemas serialize graph structures defined here
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]** — error codes and severity levels originate from enforcement logic

---

## Three Output Modes

Every command (`map`, `discover`, `compile`, `explain`, `where`) supports three output modes:

| Flag | Target Audience | Description |
|------|-----------------|-------------|
| `--json` | Hooks, CI, custom tooling | Full-fidelity structured JSON. Every node, edge, error, warning. |
| `--llm` | LLM context window | Non-human-readable, token-budgeted summary optimized for LLM consumption. |
| *(default)* | Human developer at CLI | Human-readable formatted text. Errors/warnings as colored text. Map as summary table. |

---

## JSON Output Schemas

### `keel compile` Error Output

This is the schema the LLM sees via stderr when the PostToolUse hook fires with exit code 2. Every error/warning includes `confidence` (0.0-1.0) and `resolution_tier` so the LLM can gauge reliability. Every `ERROR`-level violation includes a `fix_hint`. These fields feed the circuit breaker escalation.

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

**Note on `info` block:** When compile passes with zero errors and zero warnings, stdout is **empty** (exit 0, no output). The `info` block is only emitted with `--verbose` or when errors/warnings are present. Info data is always written to `graph.db`.

---

### `keel discover` JSON Output

```json
{
  "version": "1.0",
  "command": "discover",
  "target": {
    "hash": "xK2p9Lm4Q",
    "name": "login",
    "signature": "login(email: str, pw: str) -> Token",
    "file": "src/auth/login.ts",
    "line_start": 42,
    "line_end": 68,
    "docstring": "Authenticate user and return JWT token.",
    "type_hints_present": true,
    "has_docstring": true
  },
  "upstream": [
    {
      "hash": "mN7rT2wYs",
      "name": "handleLogin",
      "signature": "handleLogin(req: Request, res: Response) -> void",
      "file": "src/routes/auth.ts",
      "line": 15,
      "docstring": "Express route handler for POST /auth/login.",
      "call_line": 23
    }
  ],
  "downstream": [
    {
      "hash": "pQ4sV8nXe",
      "name": "validateCredentials",
      "signature": "validateCredentials(email: str, pw: str) -> User",
      "file": "src/auth/validate.ts",
      "line": 8,
      "docstring": "Check credentials against database.",
      "call_line": 45
    }
  ],
  "module_context": {
    "module": "src/auth/",
    "sibling_functions": ["login", "logout", "validateToken", "refreshToken", "resetPassword"],
    "responsibility_keywords": ["authentication", "token", "credentials", "session"],
    "function_count": 5,
    "external_endpoints": ["POST /auth/login", "POST /auth/logout", "POST /auth/refresh"]
  }
}
```

---

### `keel map --json` Output

```json
{
  "version": "1.0",
  "command": "map",
  "summary": {
    "total_nodes": 342,
    "total_edges": 891,
    "modules": 28,
    "functions": 287,
    "classes": 27,
    "external_endpoints": 15,
    "languages": ["typescript", "python"],
    "type_hint_coverage": 0.94,
    "docstring_coverage": 0.87
  },
  "modules": [
    {
      "path": "src/auth/",
      "function_count": 5,
      "class_count": 1,
      "external_endpoints": 3,
      "functions": [
        {
          "hash": "xK2p9Lm4Q",
          "name": "login",
          "signature": "login(email: str, pw: str) -> Token",
          "upstream_count": 1,
          "downstream_count": 3,
          "is_public": true,
          "type_hints_present": true,
          "has_docstring": true,
          "external_endpoints": [{"kind": "HTTP", "method": "POST", "path": "/auth/login", "direction": "serves"}]
        }
      ]
    }
  ]
}
```

---

### `keel explain` JSON Output

Returns the resolution chain — the evidence keel used to determine a dependency. Used by the circuit breaker at attempt 3 and whenever the LLM needs to diagnose a potential false positive.

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

**Explain output fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `error_code` | string | Always | The error code being explained (e.g., `E001`) |
| `hash` | string | Always | The function hash keel flagged |
| `confidence` | float 0.0-1.0 | Always | How confident keel is in the resolution |
| `resolution_tier` | string enum | Always | Which tier produced the evidence |
| `resolution_chain` | array | Always | Ordered list of evidence steps. Each has `kind` (import/call/type_ref/re_export), `file`, `line`, `text` |
| `summary` | string | Always | Human/LLM-readable summary of the resolution reasoning |

---

## Error Codes Table

| Code | Category | Severity | Description |
|------|----------|----------|-------------|
| E001 | broken_caller | ERROR | Function signature changed, callers expect old signature |
| E002 | missing_type_hints | ERROR | Function parameters or return type lack type annotations |
| E003 | missing_docstring | ERROR | Public function has no docstring |
| E004 | function_removed | ERROR | Function was deleted but still has callers |
| E005 | arity_mismatch | ERROR | Function parameter count changed, callers pass wrong number of arguments |
| W001 | placement | WARNING | Function may be better placed in a different module |
| W002 | duplicate_name | WARNING | Function with same name exists elsewhere in the codebase |
| W003 | naming_convention | WARNING | Function name doesn't match module's naming pattern (Phase 2) |
| W004 | cross_repo_endpoint | WARNING | Changed endpoint is consumed by a linked repo (Phase 2) |
| S001 | suppressed | INFO | Violation suppressed via inline `# keel:suppress` or `[suppress]` config. Logged for visibility but does not block. |

---

## Common Fields on All Error/Warning Objects

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `confidence` | float 0.0-1.0 | Always | How confident keel is in the resolution. 1.0 = certain (e.g., tree-sitter found the node). < 0.7 = heuristic/ambiguous. Feeds instruction template guidance: "If confidence < 0.7, attempt one fix. If unresolved, move on." |
| `resolution_tier` | string enum | Always | Which resolution tier produced the evidence. One of: `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip`. |
| `fix_hint` | string | ERROR: always. WARNING: where applicable. | Simple text instruction telling the LLM what to do. Not structured code transforms — just a human-readable (LLM-readable) description of the fix. Feeds the circuit breaker escalation at attempt 1. |

---

## LLM Format (`--llm`)

### Design Principles

- Non-human-readable (optimized for token count, not readability)
- Hierarchical: module -> function, compressed
- Includes: name, truncated hash (7 chars — unique within any realistic codebase), edge counts (in/out), external endpoint flag
- **Excludes:** signatures (use `discover` to fetch on demand), file paths (use `where` to resolve), docstrings (use `discover` to fetch), function bodies
- Every token in the map must earn its place — if information is discoverable on demand, it doesn't belong in the always-loaded map

### Format Specification

```
mod:auth[5,3E]
 login:xK2p9Lm↑1↓3E
 logout:mN7rT2w↑2↓1
 validate:pQ4sV8n↑5↓2
 refreshToken:bR3kL9m↑2↓2E
 resetPassword:cT5nP2r↑1↓3E
mod:payments[2,1E]
 charge:dU6oQ3s↑2↓4E
 refund:eV7pR4t↑1↓2
```

### Key

- `mod:name[N,ME]` — module with N functions, M external endpoints
- `name:hash↑N↓M` — function name, 7-char truncated hash, upstream caller count (↑), downstream callee count (↓)
- `E` suffix — function has external endpoints (HTTP routes, gRPC, etc.). Use `discover` to see details.
- Signatures are **not** in the map — use `keel discover <hash>` to fetch full signatures, callers, and callees on demand. This is the key tradeoff: the map tells the LLM *what exists and how it's connected*; `discover` tells it *the details* when it's about to edit.

### Token Budget

- Target: ~4 tokens per function
- Fits within 5% of 200k context for codebases up to ~2,500 functions (~10k tokens)
- Codebases up to ~5,000 functions fit within 10% (~20k tokens)
- Above ~5,000 functions, scoped maps are required

### Scoped Maps

For larger codebases: `keel map --llm --scope=auth,payments` returns only the subgraph for specified modules.

### `--llm-verbose`

`keel map --llm-verbose` includes full signatures in the map, reverting to a verbose format. Useful for smaller codebases (<1,000 functions) where signatures in context are worth the token cost.

---

## Clean Compile Behavior

When `keel compile` passes with:
- Zero errors AND zero warnings: exit 0, **empty stdout**. No info block.
- The `info` block (`nodes_updated`, `edges_updated`, `hashes_changed`) is only emitted when `--verbose` is passed, or alongside errors/warnings.
- Info data is always written to `graph.db` and available via `keel stats`.

This design keeps the LLM's context window clean — the LLM never sees compile output unless something needs attention.

---

## Inter-Agent Contracts

### Exposed by this spec (Agent B -> Agent C):

**Output format serializers:** All JSON schemas defined here have corresponding Rust serialization types that Agent C's server layer uses directly.

```rust
pub struct CompileOutput {
    pub version: String,
    pub command: String,
    pub status: String,
    pub files_analyzed: Vec<String>,
    pub errors: Vec<CompileError>,
    pub warnings: Vec<CompileWarning>,
    pub info: Option<CompileInfo>,
}

pub struct DiscoverOutput {
    pub version: String,
    pub command: String,
    pub target: DiscoverTarget,
    pub upstream: Vec<AdjacencyEntry>,
    pub downstream: Vec<AdjacencyEntry>,
    pub module_context: ModuleContext,
}

pub struct MapOutput {
    pub version: String,
    pub command: String,
    pub summary: MapSummary,
    pub modules: Vec<MapModule>,
}

pub struct ExplainOutput {
    pub version: String,
    pub command: String,
    pub error_code: String,
    pub hash: String,
    pub confidence: f64,
    pub resolution_tier: String,
    pub resolution_chain: Vec<ResolutionStep>,
    pub summary: String,
}
```

### Consumed by this spec:

- **[[keel-speckit/000-graph-schema/spec|Spec 000]]** — graph structures that JSON schemas serialize
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006]]** — error codes, severity levels, confidence scores originate from enforcement engine

---

## Acceptance Criteria

**GIVEN** a compile with errors
**WHEN** `keel compile <file> --json` is run
**THEN** the JSON output validates against the compile error schema defined above, with all required fields present on every error and warning object.

**GIVEN** a compile error of severity ERROR
**WHEN** the JSON output is inspected
**THEN** the `fix_hint` field is non-empty.

**GIVEN** a compile with zero errors and zero warnings
**WHEN** `keel compile <file>` is run (no `--verbose`)
**THEN** stdout is empty and exit code is 0.

**GIVEN** a compile with zero errors and zero warnings
**WHEN** `keel compile <file> --verbose` is run
**THEN** the info block is emitted with `nodes_updated`, `edges_updated`, `hashes_changed`.

**GIVEN** a function with known callers and callees
**WHEN** `keel discover <hash> --json` is run
**THEN** the JSON output validates against the discover schema with correct upstream/downstream entries and module context.

**GIVEN** an initialized project with 50 functions
**WHEN** `keel map --json` is run
**THEN** the JSON output validates against the map schema with correct summary counts and all modules/functions listed.

**GIVEN** an initialized project with 50 functions
**WHEN** `keel map --llm` is run
**THEN** the output is in the compressed LLM format with module headers, function entries with 7-char hashes, edge counts, and endpoint flags. Total token count is approximately 4 tokens per function.

**GIVEN** an error E001 on a function
**WHEN** `keel explain E001 <hash> --json` is run
**THEN** the JSON output validates against the explain schema with a non-empty resolution chain, confidence score, and resolution tier.

**GIVEN** a project with 3,000 functions
**WHEN** `keel map --llm --scope=auth` is run
**THEN** only the `auth` module subgraph is returned, not the full 3,000-function map.

**GIVEN** a suppressed violation (code S001)
**WHEN** `keel compile <file> --json` is run
**THEN** the suppressed violation appears in the JSON output with `severity: "INFO"` and `suppressed: true`, and does NOT appear in errors or warnings arrays.

---

## Test Strategy

**Oracle:** JSON schema validation.
- Every JSON output from every command must validate against its schema.
- Property-based tests: generate random graph states, verify JSON output is always valid.
- Token counting: verify LLM format stays within budget for various codebase sizes.

**Test files to create:**
- `tests/output/test_compile_json_schema.rs` (~8 tests)
- `tests/output/test_discover_json_schema.rs` (~6 tests)
- `tests/output/test_map_json_schema.rs` (~5 tests)
- `tests/output/test_explain_json_schema.rs` (~5 tests)
- `tests/output/test_llm_format.rs` (~6 tests)
- `tests/output/test_llm_token_budget.rs` (~4 tests)
- `tests/output/test_clean_compile.rs` (~3 tests)
- `tests/output/test_error_codes.rs` (~5 tests)

**Estimated test count:** ~42

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| JSON schema changes break hook consumers | Version field in all JSON output. Hooks check version before parsing. |
| LLM format token count exceeds budget for large codebases | Enforce scoped maps above 5,000 functions. Warn if map exceeds `max_tokens` config. |
| LLM format ambiguous for LLMs to parse | Test with Claude, GPT-4, Gemini on real codebase maps. Iterate format if parsing errors occur. |
| `fix_hint` text too vague for LLM to act on | Include specific file:line references in every fix hint. Test with LLM-in-the-loop during dogfooding. |

---

## Related Specs

- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — graph structures serialized by these formats
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — error codes and validation logic
- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — commands that produce these outputs
- [[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]] — hooks consume JSON output
- [[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]] — server returns these formats
- [[keel-speckit/011-vscode-extension/spec|Spec 011: VS Code Extension]] — displays diagnostics from JSON
