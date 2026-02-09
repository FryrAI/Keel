# keel — Product Requirements Document

**Version:** 2.1 **Author:** Ben  **Status:** Architecture defined, ready to build **Website:** keel.engineer **License:** FSL (Functional Source License)

---

## 1. Problem Statement

LLM-driven coding tools (Claude Code, Codex, Cursor, Antigravity, Gemini CLI, Aider, Windsurf) navigate codebases poorly. They grep, they guess, they lose context. The consequences are predictable and expensive:

- **Blind edits:** LLMs change a function without checking what calls it or what it calls — breaking upstream consumers silently.
- **Duplicate implementations:** Unable to find an existing utility, the LLM writes a new one in the wrong place, fragmenting the codebase.
- **Spaghetti placement:** LLMs dump new functions wherever they happen to be editing, ignoring module boundaries, separation of concerns, and project conventions. Over time, the codebase loses its architecture.
- **Cross-repo blindness:** Backend API changes don't propagate to the frontend. The LLM doesn't even know the frontend exists.
- **Wasted tokens:** Without a structural map, LLMs burn context window on exploratory file reads — often 50-70% of available tokens just on orientation.
- **No human overview:** Engineers using LLM tools lose the mental model of their own codebase as AI-generated code accumulates.
- **Code quality decay:** LLMs produce untyped functions, skip docstrings, ignore naming conventions. Without enforcement, AI-generated code gradually degrades codebase quality.

These are not LLM intelligence problems — they are **tooling infrastructure problems**. The LLM is working blind because no tool gives it a structural map of the codebase with enforcement.

## 2. Foundational Design Principle: The LLM Is the User

This principle shapes every design decision in keel. The LLM is the primary user. The human is the engineer — the one who designs the system, reviews the output, and sets the rules. keel enforces those rules on the LLM.

A human developer would revolt against mandatory type hints on every function, forced docstrings, pre-edit adjacency checks, and placement constraints. An LLM doesn't care — it will comply with any structural requirement at negligible cost. keel exploits this asymmetry: it demands things from the LLM that would be unreasonable to demand from humans, and in doing so, it produces better code than either humans or unassisted LLMs would write alone.

**All code is treated as LLM-written.** There is no "legacy" vs "enforced" distinction. keel assumes the entire codebase is under LLM stewardship. Enforcement applies universally. If a human writes an untyped function by hand, keel catches it at commit time the same way it catches an LLM writing one.

**Adoption nuance:** For existing codebases, enforcement starts as `WARNING` for pre-existing code and `ERROR` for new/modified code. This avoids the "wall of 500 errors on init" problem. Teams can configure an escalation schedule in `.keel/config.toml` to gradually tighten enforcement. The goal is universal enforcement; the path is progressive adoption.

This means:

- **Mandatory type hints** on all functions (Python type hints, JSDoc/TypeScript, Go already typed, Rust already typed). New code: `ERROR`. Pre-existing untyped code: `WARNING` (configurable escalation to `ERROR`).
- **Mandatory docstrings** on all public functions. The docstring feeds the human visual map and enriches the LLM map for future context. Same progressive enforcement as type hints.
- **Mandatory adjacency verification** before edits. The LLM must call `discover` before modifying a function with upstream callers.
- **Mandatory post-edit validation.** The LLM must call `compile` after every edit and fix any errors before proceeding.
- **Mandatory placement guidance.** Before creating a new function, the LLM must consult the map to determine where it belongs. keel flags misplaced code.

None of this adds friction for the human engineer. The human sees better, more documented, more consistent, better-organized code emerging from the LLM.

### The four pillars

1. **Backpressure** (core differentiator) — Force the LLM to verify before and validate after every edit. "Look before you leap" + "prove you didn't break anything." Like a ship's keel creating hydrodynamic resistance that prevents capsizing, keel creates structural resistance that prevents architectural collapse. No other tool in the ecosystem does this.

2. **Contract enforcement** — Type contracts, signature contracts, adjacency contracts. Like `tsc` for architecture. The LLM cannot silently change a function's parameters without updating every caller. The LLM cannot remove a function that other code depends on. Structural contracts are verified on every edit — not at review time, not at build time, but at generation time.

3. **Structural navigation** (complementary to context) — The LLM-optimized map shows how code is connected: call graph, module boundaries, external endpoints. Context providers (Augment, Aider repo map, Cursor indexing) tell the LLM what the code looks like. keel tells the LLM how it's connected and what it's not allowed to break. These are complementary: "Augment gives your agent perfect recall. keel gives it perfect discipline. Use both."

4. **Placement intelligence** — Where new code belongs. Module boundaries as first-class concepts. The LLM cannot dump a database function in the checkout module. keel encodes module responsibilities in the graph and surfaces placement guidance via `discover` and `compile`.
    

## 3. Product Vision

### Category: Structural Guardrails for Code Agents

The LLM tooling landscape has three occupied categories:

1. **Context providers** (Augment, Aider repo map, Cursor indexing) — help the LLM understand code
2. **Review-time checkers** (Greptile, CodeQL, linters) — catch problems after code is written
3. **Agent guardrails** (LangGraph, Letta, MCP-Scan) — constrain agent behavior generically

keel creates **category 4: structural enforcement during generation.** The LLM cannot break callers, skip types, misplace code, or ignore adjacency — enforced in real-time via tool hooks. This is a category that doesn't exist yet.

keel is a **generation-time architectural enforcement layer** that sits between LLM coding agents and the codebase. It provides:

1. A fast, incrementally-updated **structural graph** of the entire codebase (functions, classes, modules, their connections, and their external touchpoints).
2. A **token-efficient representation** of that graph optimized for LLM consumption (not human-readable — minimal tokens, maximum navigability).
3. A **human-readable visual map** for engineers to maintain oversight of AI-modified codebases.
4. An **enforcement layer** that structurally prevents LLMs from making changes without verifying adjacent code, maintaining type contracts, placing code correctly, and respecting conventions. This is enforced at generation time — not at review time, not at build time.
5. **Cross-repo awareness** linking frontend and backend at the code level (Phase 2).

**Category naming note:** "Structural Guardrails" is the internal working label. Market validation is needed before launch — buyer vocabulary may differ. Alternative framings to test: "Architectural Governance for AI Code," "Code Quality Enforcement," "Guardrails as Code." The market's own language includes "architectural drift" (what buyers complain about), "comprehension debt," and "guardrails as code" (the aspiration). The name should be tested with prospects before launch.

### Target Buyer

**Primary:** VP/Director of Engineering + Platform/DevEx leads at organizations with 50+ developers who have adopted AI coding tools and are watching architectural coherence erode. These are the guardians of codebase integrity — not individual developers.

**Pain quantified:**
- 29% trust in AI-generated code (industry survey)
- 41% increase in complexity after AI adoption (Carnegie Mellon study)
- 8x increase in duplicate code blocks (GitClear)
- 67% of developers spend more time debugging AI code than before (Harness)
- 25% more AI adoption = 7.2% less delivery stability (Google DORA)
- 53% cite code quality as top barrier to AI coding adoption (OpsLevel survey)

**Secondary:** Senior/Staff engineers who feel "AI slop" pain daily and champion keel internally. They adopt CLI tools locally and champion to leadership. They are the **land** part of land-and-expand.

**Anti-persona:** AI-native agencies and indie hackers who view enforcement as friction. They generate massive technical debt but are not the immediate buyer until they scale.

### What keel is NOT

- Not a context provider (Augment does that — keel is complementary, not competitive)
- Not an LLM coding agent (it doesn't write code)
- Not a code reviewer (Greptile does that — keel prevents problems, not catches them)
- Not a generic agent guardrail (LangGraph does that — keel is code-structure-specific)
- Not an IDE plugin (it's infrastructure that IDE plugins can consume)
- Not a linter or formatter (it validates structural relationships, not style)
- Not a replacement for LSP (it complements LSP with graph-level intelligence and enforcement)

### The "Better Models" Defense

Enforcement is orthogonal to model quality. TypeScript runs `tsc` even though developers know JavaScript. Rust runs the borrow checker even though developers understand memory. Better models make fewer obvious errors; they do not make zero architectural errors. The errors better models make are *harder to spot* because the surrounding code is more competent. keel catches structural violations regardless of model quality — and will become *more* valuable as models get better and developers rely on them more heavily.

## 4. Core Commands

### 4.1 `keel init`

**Purpose:** Initialize keel in a repository. Zero-config for common project structures.

**Behavior:**

- Auto-detect languages present in the repo
- Read existing project configuration to derive initial enforcement settings (`tsconfig.json`, `pyproject.toml`, `.eslintrc`, etc.)
- Parse entire codebase via tree-sitter into function/class/module graph
- Discover external touchpoints (HTTP endpoints served, API calls made, database queries, message queue producers/consumers, gRPC service definitions)
- Generate initial graph stored in `.keel/` directory
- Generate hook configs for all detected enforced tools: Claude Code (`.claude/settings.json`), Cursor (`.cursor/hooks.json`), Gemini CLI (`.gemini/settings.json`), Windsurf (`.windsurf/hooks.json`), Letta Code (Letta config) — see §9
- Generate instruction files for all detected tools (see §9)
- Append keel workflow instructions to `CLAUDE.md` (create if absent)
- Output summary: node count, edge count, external endpoints found, languages detected
- Generate initial hash for every function/class node
- Generate `.keelignore` with sensible defaults (`generated/`, `vendor/`, `node_modules/`, `**/migrations/`, `dist/`, `build/`, `.next/`, `__pycache__/`). If `.keelignore` already exists, leave it untouched.

**Config merge strategy:** When `keel init` runs in a project with existing tool configurations (`.claude/settings.json`, `CLAUDE.md`, `.cursor/hooks.json`, etc.), keel merges its entries rather than overwriting:
- **JSON files:** Deep-merge keel hook entries into existing hooks arrays. Warn if a conflicting hook exists for the same event/matcher.
- **Markdown files:** Insert keel sections between `<!-- keel:start -->` / `<!-- keel:end -->` markers. If markers already exist, replace that section. If not, append to end.
- **TOML/YAML files:** Add keel-specific sections. Warn on key conflicts.
- **On conflict:** keel prints a warning and skips the conflicting entry, leaving the existing config intact. The developer resolves manually.

**Graph storage:**

- `.keel/manifest.json` — **committed.** Lightweight, human-readable. Lists top-level modules, their function counts, endpoint counts. Needed for cross-repo linking (Phase 2).
- `.keel/graph.db` — **gitignored.** SQLite database (keel graph + resolution cache + metadata). Regenerated locally via `keel init` or `keel map`.
- `.keel/config.toml` — **committed.** Shared team configuration.

### 4.2 `keel map`

**Purpose:** Full re-map of the codebase. Used after major refactors, branch switches, or initial setup.

**Behavior:**

- Re-parse all files (respecting `.keelignore` and `[exclude]` patterns from `config.toml`), rebuild graph from scratch
- Diff against previous graph and report: new nodes, removed nodes, changed signatures, broken edges
- Regenerate all hashes
- Output formats controlled by flags (see §6)

**Performance target:** <5 seconds for 100k LOC repo. <30 seconds for 500k LOC. Incremental updates (via `compile`) should be <200ms for single-file changes.

### 4.3 `keel discover <hash>`

**Purpose:** Given a function/class hash, return its upstream callers, downstream callees, and placement context.

**Behavior:**

- Return adjacency list: direct callers (upstream), direct callees (downstream)
- Each entry includes: hash, function signature, file path relative to root, line number, docstring (first line)
- Return **module context**: what module/file this function lives in, what other functions live in that module, what the module's responsibility is (derived from its functions and docstring)
- Configurable depth (default: 1 hop, max: configurable via `config.toml`)
- Output format: JSON (for LLM consumption) or tree (for human CLI usage)

**LLM usage pattern:** Before editing function X, the LLM calls `discover X` to understand what depends on X, what X depends on, and what module X lives in. This is the "look before you leap" mechanism.

### 4.4 `keel compile [file...]`

**Purpose:** Incrementally update the graph after a file change and validate structural integrity.

**Behavior:**

1. Re-parse changed file(s) only (tree-sitter incremental parsing)
2. Update affected nodes and edges in the graph
3. Recompute hashes for changed functions
4. **Validate adjacent contracts:**
    - If function signature changed (parameters, return type): check all callers still match
    - If function was removed: report all broken callers
    - If function was added: check for duplicate names in the codebase
5. **Enforce type hints:** Error if any function lacks type annotations
6. **Enforce docstrings:** Error if any public function lacks a docstring
7. **Validate placement:** If a new function was added, check if it belongs in its current module based on the module's existing responsibility pattern (see §8)
8. Return: updated hashes, list of warnings/errors, affected downstream nodes

**Clean compile behavior:** When compile passes with zero errors AND zero warnings: exit 0, **empty stdout**. No info block. The `info` block (`nodes_updated`, `edges_updated`, `hashes_changed`) is only emitted when `--verbose` is passed, or alongside errors/warnings. Info data is always written to `graph.db` and available via `keel stats`. This keeps the LLM's context window clean — the LLM never sees compile output unless something needs attention.

**Error severity levels:**

- `ERROR`: Callers will break (type mismatch, missing function, changed arity), missing type hints, missing docstrings on public functions. Blocks via hook / blocks commit via git hook.
- `WARNING`: Placement suggestion (function may belong elsewhere), potential naming issue, similar function exists elsewhere, cross-repo endpoint affected (Phase 2).
- `INFO`: Graph updated successfully, N nodes affected.

**Batch mode:** When scaffolding multiple files, per-edit enforcement on incomplete code creates noise. `keel compile --batch-start` defers non-structural validations (type hints, docstrings, placement) until `keel compile --batch-end`. Structural errors (broken callers, removed functions, arity mismatches) still fire immediately during batch mode. Batch auto-expires after 60s of inactivity. See §20.7 for full UX.

**Suppress mechanism:** False positives are inevitable in heuristic-based enforcement. keel provides three suppression layers:
- **Inline:** `# keel:suppress E001 — reason` on the line above the function. Suppresses the specific error code for that function.
- **Config:** `[suppress]` section in `.keel/config.toml` for persistent suppressions with required reason field (see §13).
- **CLI:** `keel compile --suppress W001` to suppress a code for a single invocation (useful during exploration).
- Suppressed violations are downgraded to `INFO` severity (code S001) and remain visible in `--json` output and telemetry. They are never silently hidden.

**Dynamic dispatch note:** Call edges resolved with low confidence (dynamic dispatch, trait/interface methods, untyped method calls) are enforced at `WARNING`, not `ERROR`. This prevents false positives from blocking the LLM on ambiguous resolutions. See §10.1 Tier 3 for how LSP/SCIP can promote these to `ERROR`.

**Type hint enforcement per language:**

- **TypeScript, Go, Rust:** Already typed. keel validates signature changes against callers using existing type information.
- **Python:** Requires type hints on all parameters and return types. `def process(data)` → `ERROR`. `def process(data: dict[str, Any]) -> ProcessResult` → passes.
- **JavaScript:** Requires JSDoc `@param` and `@returns` annotations. The LLM is instructed to prefer TypeScript for new files.

### 4.5 `keel where <hash>`

**Purpose:** Resolve a hash to a file location.

**Behavior:**

- Return: file path relative to project root, start line, end line
- If hash is stale (function was modified since hash was generated): return location with `STALE` flag and suggest re-running `compile`

### 4.6 `keel deinit`

**Purpose:** Cleanly remove all keel-generated files and configurations from a project.

**Behavior:**

- Remove `.keel/` directory (graph.db, manifest.json, hooks/, telemetry.db)
- Remove keel sections from: `.claude/settings.json`, `CLAUDE.md`, `.cursor/hooks.json`, `.cursor/rules/keel.mdc`, `.gemini/settings.json`, `GEMINI.md`, `.windsurf/hooks.json`, `.windsurfrules`, `AGENTS.md`, `.agent/rules/keel.md`, `.agent/skills/keel/`, Letta config, `.github/copilot-instructions.md`
- Remove pre-commit hook (or keel's section from it if other hooks exist)
- Preserve `.keel/config.toml` (so `keel init` can re-initialize with same settings)
- Report what was removed

### 4.7 `keel link <remote-repo-url-or-path>` (Phase 2)

**Purpose:** Link two repositories for cross-repo awareness.

**Behavior:**

- Register another repo's keel manifest as a linked dependency
- Match external endpoints: if repo A serves `POST /api/users` and repo B calls `POST /api/users`, create a cross-repo edge
- Store link configuration in `.keel/config.toml`
- When `compile` detects a change to a linked endpoint, emit a `CROSS_REPO_WARNING` with the consuming function in the other repo

**Matching strategy:** Endpoint URL patterns, gRPC service/method names, GraphQL type/field names, message queue topic names. Heuristic matching of URL path patterns with variable segments (e.g., `/api/users/:id` ↔ `/api/users/${userId}`).

### 4.8 `keel serve`

**Purpose:** Run a persistent local server exposing keel commands via MCP (stdio), HTTP, or Unix socket. Powers the VS Code extension and provides integration surface for any MCP-compatible tool.

**Modes:**

- `keel serve --mcp` — MCP over stdio. Integrates with Claude Code, Cursor, Antigravity, Codex, any MCP client.
- `keel serve --http` — HTTP API on `localhost:4815`. Powers the VS Code extension. Provides REST endpoints for custom integrations.
- `keel serve --watch` — File system watcher. Auto-runs `compile` on file save. Combines with `--mcp` or `--http`.

**Behavior:**

- Wraps all CLI commands as server endpoints/tools
- Holds graph in memory for sub-millisecond responses (vs. CLI's load-from-SQLite-per-call)
- Watches file system for changes and auto-runs `compile`
- MCP tools exposed: `keel_discover`, `keel_compile`, `keel_where`, `keel_map`, `keel_explain`
- HTTP endpoints: `GET /map`, `GET /discover/:hash`, `POST /compile`, `GET /where/:hash`, `GET /explain/:error_code/:hash`, `GET /health`

**Memory footprint:** `keel serve` holds the full graph in memory for sub-millisecond responses. Expected usage: ~50-100MB for a 50k LOC repo, ~200-400MB for a 200k LOC repo. CLI mode (`keel compile`, `keel discover`) loads a subgraph from SQLite per call and uses ~20-50MB — suitable for constrained environments or CI.

**Implementation:** Thin wrapper (~500 lines) over the core library. No new logic — just transport.

### 4.9 `keel explain <error-code> <hash>`

**Purpose:** Expose keel's resolution reasoning so the LLM can diagnose false positives. This is the "show your work" command — when keel reports an error, the LLM can ask *why* keel believes the dependency exists.

**Behavior:**

- Takes an error code (e.g., `E001`) and the function hash keel flagged
- Returns the **resolution chain**: the concrete evidence keel used to determine the dependency
  - Import statement at file:line
  - Call expression at file:line
  - Type reference at file:line
  - Re-export chain (if the dependency was resolved through re-exports)
- Shows **confidence score** (0.0–1.0) for the resolution
- Shows **resolution tier** that produced the evidence: `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip`
- Output: JSON for LLM consumption (default), human-readable tree with `--tree`

**LLM usage pattern:** Used at circuit breaker attempt 3 (see §4.10) — when repeated fixes haven't resolved an error, the LLM inspects keel's reasoning to determine if the error is a false positive or if the fix strategy is wrong. Also useful when the LLM encounters a `WARNING` with low confidence and wants to understand the evidence before acting.

**JSON output:**

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

### 4.10 Circuit Breaker / Escalation Design

**Purpose:** Define how keel handles repeated compile failures on the same error. Instead of the LLM banging its head on the same wall, keel progressively escalates its guidance and eventually downgrades to let the LLM move on.

**Tracking:** keel tracks consecutive failed compiles per function/error-code pair. In `keel serve` mode, this is session state in memory. In CLI mode, state is stored in `.keel/session.json` (temp file, gitignored).

**Escalation sequence:**

1. **Attempt 1 fails:** Normal error output + `fix_hint` in the JSON response (see §12). The fix hint is a simple text instruction: *"Update 3 callers to pass Password instead of string: handleLogin at src/routes/auth.ts:23, autoLogin at src/middleware/session.ts:88, testLogin at tests/auth.test.ts:23."*

2. **Attempt 2 fails (same error-code + hash):** Error output + fix hint + **escalation instruction**: *"Run `keel discover <hash> --depth 2` to inspect the wider dependency chain. The issue may be upstream of the direct callers."*

3. **Attempt 3 fails (same error-code + hash):** **Auto-downgrade to WARNING** for this session. Instruction: *"Run `keel explain <error-code> <hash>` to inspect the resolution chain. Add findings as a code comment so the next session can resolve with full context."*

**Design decisions:**

- **3 retries regardless of confidence score.** Keep it simple — let the instructions handle nuance, not the retry logic.
- **Downgraded errors re-enforce on next session.** The downgrade is a session-scoped escape valve, not a permanent suppression. Next `keel serve` restart or next CLI invocation resets the counter.
- **Counter resets on success.** If the LLM fixes the error on attempt 2, the counter resets. Only consecutive failures on the same error-code + hash pair escalate.
- **Batch mode interaction:** Circuit breaker counters are paused during `--batch-start` / `--batch-end`. Scaffolding sessions shouldn't trigger escalation on deferred validations.
- **Configuration:** Configurable via `[circuit_breaker]` in `.keel/config.toml` (see §13). Defaults (`max_retries = 3`, `auto_downgrade = true`) should rarely be changed.

**Why this matters for the LLM:** Without escalation, the LLM enters a retry loop — it tries the same fix, gets the same error, tries again. The circuit breaker converts a frustrating loop into a progression: fix → investigate wider → inspect reasoning → document and move on. This is how a senior engineer would handle it.

## 5. Hash Design

### Requirements

- Deterministic: same function content → same hash
- Compact: short enough for LLM context efficiency (≤12 chars)
- Content-addressed: captures function signature + body + docstring
- Collision-resistant within a single codebase (not globally unique)

### Scheme

```
hash = base62(xxhash64(canonical_signature + body_normalized + docstring))
```

- **xxHash64** for speed (>10GB/s, vastly faster than SHA-256)
- **base62 encoding** for compact, URL-safe representation (11 chars for 64-bit hash)
- **Canonical signature:** normalized function declaration (name, params with types where available, return type where available) — whitespace/comment stripped
- **Body normalized:** AST-based normalization (strip comments, normalize whitespace) — NOT raw text, to avoid hash churn from formatting changes
- **Docstring included:** Forces hash change when documentation changes, ensuring the map stays current

### Hash stability — solved by backpressure, not by clever hashing

When a function is renamed, its hash changes. All references to the old hash become invalid. keel's backpressure system handles it:

1. LLM renames function `processPayment` → `handlePayment`
2. LLM calls `keel compile`
3. `compile` detects: old hash `xK2p9Lm4Q` gone, new hash `bR3kL9mWq` appeared. 5 callers still reference `processPayment`.
4. `compile` returns `ERROR`: broken references listed with file locations
5. LLM updates all callers

The hash doesn't need to be stable across refactors — the enforcement layer catches breakage immediately. As a convenience, `.keel/graph.db` maintains a `previous_hashes` list per node (last 3 hashes) so `discover` and `where` can resolve recently-changed hashes with a `RENAMED` flag during the same editing session.

## 6. Output Formats

keel supports two output flags, usable on `map`, `discover`, `compile`, and `explain`:

### `--json` (structured, for programmatic consumption)

Full-fidelity JSON output. Every node, edge, error, warning. Used by hooks, CI, and custom tooling. Schema defined in §12.

### `--llm` (token-optimized, for LLM context injection)

Non-human-readable, token-budgeted summary optimized for LLM consumption. This is the format injected into context by the SessionStart hook.

**Format design principles:**

- Non-human-readable (optimized for token count, not readability)
- Hierarchical: module → function, compressed
- Includes: name, truncated hash (7 chars — unique within any realistic codebase), edge counts (in/out), external endpoint flag
- **Excludes:** signatures (use `discover` to fetch on demand), file paths (use `where` to resolve), docstrings (use `discover` to fetch), function bodies
- Every token in the map must earn its place — if information is discoverable on demand, it doesn't belong in the always-loaded map

**Example output (target: ~3-5 tokens per function):**

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

Key:
- `mod:name[N,ME]` — module with N functions, M external endpoints
- `name:hash↑N↓M` — function name, 7-char hash, upstream caller count, downstream callee count
- `E` suffix — function has external endpoints (HTTP routes, gRPC, etc.). Use `discover` to see details.
- Signatures are **not** in the map — use `keel discover <hash>` to fetch full signatures, callers, and callees on demand. This is the key tradeoff: the map tells the LLM *what exists and how it's connected*; `discover` tells it *the details* when it's about to edit.

**Token budget:** At ~4 tokens per function, the map fits within 5% of 200k context for codebases up to ~2500 functions (~10k tokens). Codebases up to ~5000 functions fit within 10% (~20k tokens). Above ~5000 functions, scoped maps are required.

For larger codebases, keel supports scoped maps: `keel map --llm --scope=auth,payments` returns only the subgraph for specified modules.

**Full signature format (opt-in):** `keel map --llm-verbose` includes full signatures in the map, reverting to the verbose format. Useful for smaller codebases (<1000 functions) where signatures in context are worth the token cost.

**Default behavior (no flag):** Human-readable CLI output. Errors/warnings as formatted text. Map as summary table.

## 7. Human Visual Map

A lightweight browser-based graph visualization so the engineer can see what the LLM is building. This is a feature, not a product. Minimal investment, maximum transparency.

### Requirements

- Static HTML file generated by `keel map --visual`
- Interactive directed graph (nodes = functions/classes, edges = calls/imports)
- Color-coded by module/package
- Hover shows: full signature, docstring, file location
- External endpoints visually distinct
- No server required — open the HTML file in a browser

### Implementation

Single self-contained HTML file using D3.js force-directed graph (bundled inline). Graph data embedded as JSON. Total file size target: <2MB for a 5000-function codebase. This is a one-week feature, not a product line.

**Phase 1: out of scope.** Phase 2 feature.

## 8. Code Placement

This is the "where does new code go?" problem. LLMs dump functions wherever they happen to be editing, creating spaghetti. keel solves this by making module boundaries and responsibilities explicit in the graph.

### How it works

**During `keel init` / `keel map`:**

1. keel builds a module responsibility profile for each directory/module. This profile is derived from:
    - The module's name and path
    - The functions it contains (names, types, docstrings)
    - Its import/export patterns
    - Its external endpoint definitions
2. The profile is stored in `.keel/graph.db` as metadata on module nodes.

**During `keel compile` (new function added):**

1. keel computes a simple placement score: does this new function's name, type signature, and dependencies align with the module it was placed in?
2. Scoring heuristic (not ML-based — pure structural):
    - Does the function call other functions in this module? (+score)
    - Do other functions in this module call it? (+score)
    - Does the function's name share a prefix/domain with sibling functions? (+score)
    - Does the function import types from different modules than its siblings? (-score)
    - Is there another module where this function would score higher? (→ `WARNING` with suggestion)
3. If placement score is below threshold, emit `WARNING: Function 'calculateShipping' may belong in module 'shipping' rather than 'checkout'. 'shipping' contains: calculateRate, getCarrier, estimateDelivery.`

**During `keel discover` (context for new function creation):** The `--suggest-placement` flag (or equivalent data in `--json` output) returns the top 3 modules where a function with a given purpose would best fit, based on the existing graph structure.

### Known limitations (honest assessment)

- **Utility modules** (`utils/`, `helpers/`, `common/`): Placement scoring is weak here because utility modules are inherently cross-cutting — they contain functions that don't belong to any single domain. keel's heuristic scores poorly on these. Mitigation: configurable utility pattern exclusions via `[exclude]` patterns in `config.toml`.
- **Facades and orchestrator modules:** Modules that call many other modules score ambiguously. A function in `checkout/service.ts` that orchestrates auth + payments + shipping will have dependencies everywhere — placement scoring can't distinguish "correctly orchestrating" from "misplaced."
- **Small modules (<5 functions):** Not enough signal for a meaningful responsibility profile. Placement scoring is unreliable until a module has 5+ functions with consistent naming/typing patterns.
- **Realistic false positive rate:** 15-25% overall on correctly-placed functions. On well-structured code with clear domain boundaries: 5-10%. On utility-heavy or orchestrator-heavy codebases: higher. This is WARNING-level by design — the cost of a false positive is the LLM reading a suggestion it ignores, not a blocked edit.

### Phase 1 scope

- Module responsibility profile generation (structural, name-based)
- Placement scoring on `compile` (WARNING level only)
- Suggested modules in `discover` output

### Phase 2 scope

- Configurable placement rules in `config.toml` (e.g., "all database access functions must be in `repository/`")
- Stricter enforcement (ERROR level for rule violations)

## 9. LLM Tool Integration

keel is tool-agnostic by design. Every LLM coding tool can execute shell commands — that's the universal integration surface. On top of that, each tool has its own configuration mechanism for deeper integration. `keel init` detects which tools are present and generates configuration for all of them.

### 9.1 The Universal Interface: CLI

Every LLM coding tool — Claude Code, Codex, Cursor, Antigravity, Gemini CLI, Windsurf, Aider, any future tool — can call `keel` via shell commands. This is the baseline that always works.

The LLM needs three things:

1. **Instructions** telling it to use keel (delivered via the tool's instruction file)
2. **The map** injected into context at session start (delivered via session hook or instruction file preamble)
3. **Automatic validation** after every edit (delivered via post-edit hook or instruction to self-validate)

Tools with hook systems get enforced integration (the LLM _cannot_ skip validation). Tools without hooks get cooperative integration (the LLM is _instructed_ to validate — and the git pre-commit hook catches anything it misses).

### 9.2 Integration Matrix (Updated Feb 2026)

The hook landscape changed dramatically in late 2025-early 2026. Cursor, Gemini CLI, Windsurf, and Letta Code all shipped full hook systems with exit-code-2 blocking — the same enforcement mechanism Claude Code pioneered. keel can now enforce backpressure in **5 tools**, not just 1. GitHub Copilot adds a 6th via MCP-based policies. This is a major tailwind.

|Tool|Instruction file|Hooks (enforced)|Hook config format|Session context injection|Post-edit validation|
|---|---|---|---|---|---|
|**Claude Code**|`CLAUDE.md`|✅ 14 event types, 3 handler types (command/prompt/agent)|`.claude/settings.json`|✅ SessionStart hook|✅ Enforced (exit 2 blocks)|
|**Cursor**|`.cursor/rules/keel.mdc`|✅ Full hooks since v1.7 (Oct 2025). 15+ events, exit 2 blocks|`.cursor/hooks.json`|✅ SessionStart hook|✅ Enforced (exit 2 blocks)|
|**Gemini CLI**|`GEMINI.md`|✅ Full hooks since v0.26.0. 8 events, exit 2 blocks|`.gemini/settings.json`|✅ SessionStart hook|✅ Enforced (exit 2 blocks)|
|**Windsurf**|`.windsurfrules`|✅ Cascade Hooks (late 2025). 11 events, exit 2 pre-hook blocks|`.windsurf/hooks.json`|✅ SessionStart hook|✅ Enforced (exit 2 blocks)|
|**Letta Code**|Letta instruction config|✅ ~12 events, exit 2 blocks (same semantics as Claude Code)|Letta config|✅ SessionStart hook|✅ Enforced (exit 2 blocks)|
|**GitHub Copilot**|`.github/copilot-instructions.md`|⚠️ MCP policies with JSON `permissionDecision: "deny"` (not exit-code-2)|Copilot settings / registry policies|⚠️ Via custom instructions|⚠️ Governance (MCP policy provider)|
|**Codex CLI**|`AGENTS.md`|⚠️ `notify` only (no blocking). Top community request (#2109)|`.codex/config.toml`|⚠️ Via AGENTS.md preamble|⚠️ Cooperative (AGENTS.md instruction)|
|**Antigravity**|`.agent/rules/keel.md` + `.agent/skills/keel/SKILL.md`|⚠️ No blocking hooks yet|—|⚠️ Via rule + skill|⚠️ Cooperative (rule instruction)|
|**Aider**|`.aider.conf.yml`|❌ No hook system|—|⚠️ Via `map-tokens`/`map-refresh` config|⚠️ Cooperative (instruction)|
|**Any CLI tool**|System prompt|❌|—|⚠️ Manual `keel map --llm`|⚠️ Cooperative|

**Key insight:** keel now has enforced backpressure in **5 tools** (Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code) with a 6th (GitHub Copilot) at the governance level via MCP policies. Codex remains cooperative-only (blocking hooks are the #1 community request). Aider uses `map-tokens` and `map-refresh` for context injection (NOT `map-file` — that option doesn't exist).

**Tier classification:**
- **Tier 1 (Enforced):** Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code — full hook systems with exit-code-2 blocking
- **Tier 2 (Cooperative + Governance):** GitHub Copilot (MCP policies), Codex CLI, Antigravity, Aider — instruction/policy-based with git pre-commit catch-all

### 9.3 Claude Code (Tier 1 — Enforced)

Claude Code has the most mature hook system: 14 event types, 3 handler types (command, prompt, agent). `keel init` generates:

**`.claude/settings.json`:**

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "keel map --llm"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR\"/.keel/hooks/post-edit.sh"
          }
        ]
      }
    ]
  }
}
```

**SessionStart hook:** Injects the LLM-optimized map into Claude's context at the start of every session. Plain text stdout with exit code 0 → added as context.

**PostToolUse hook:** After every Edit/Write/MultiEdit, runs `keel compile` on changed files:

```bash
#!/bin/bash
set -euo pipefail
# .keel/hooks/post-edit.sh
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
[ -z "$FILE_PATH" ] && exit 0

# Validate file path — reject metacharacters that could enable argument injection
if [[ "$FILE_PATH" =~ [^a-zA-Z0-9_./-] ]]; then
  echo "keel: rejected file path with unexpected characters: $FILE_PATH" >&2
  exit 2
fi

RESULT=$(keel compile -- "$FILE_PATH" --json 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "$RESULT" >&2
  exit 2  # Blocking: stderr shown to Claude, must fix before proceeding
fi
exit 0
```

**`CLAUDE.md` template:** (see §9.12 for full template — shared across all tools)

### 9.4 Codex CLI (Tier 2 — Cooperative + Safety Net)

Codex uses `AGENTS.md` for instructions and `.codex/config.toml` for configuration. It can execute shell commands but has no blocking post-edit hook.

**`AGENTS.md`** (generated by `keel init`):

```markdown
## keel — Code Graph Enforcement

<keel-map>
<!-- Auto-populated by: keel map --llm -->
<!-- Run `keel map --llm` to regenerate -->
</keel-map>

### Mandatory workflow:
1. Before editing a function with callers (↑ > 0 in the map above), run: `keel discover <hash>`
2. After EVERY file edit, run: `keel compile <file> --json`
3. If `keel compile` returns errors, FIX THEM before proceeding
4. Before creating a new function, check the map for existing similar functions
5. Place new functions in the module where they logically belong

### Commands:
- `keel discover <hash>` — callers, callees, module context
- `keel compile <file>` — validate changes (MUST run after every edit)
- `keel explain <error-code> <hash>` — inspect resolution reasoning (see §4.9)
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — regenerate map
```

### 9.5 Cursor (Tier 1 — Enforced)

Cursor shipped a full hook system in v1.7 (October 2025) with 15+ event types and exit-code-2 blocking — functionally equivalent to Claude Code's hook system. `keel init` generates both the hook config AND the rules file.

**`.cursor/hooks.json`** (generated by `keel init`):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "command": "keel map --llm",
        "type": "command"
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "command": "\"$PROJECT_DIR\"/.keel/hooks/post-edit.sh",
        "type": "command"
      }
    ]
  }
}
```

**`.cursor/rules/keel.mdc`** (supplementary instructions — hooks handle enforcement):

```markdown
---
description: keel code graph enforcement — structural validation for every edit
globs: ["**/*.ts", "**/*.tsx", "**/*.py", "**/*.go", "**/*.rs", "**/*.js", "**/*.jsx"]
alwaysApply: true
---

# keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.
Hooks handle automatic validation. Follow this workflow for proactive checks:

## Mandatory workflow:
1. Before editing a function with callers (↑ > 0), run: `keel discover <hash>`
2. After EVERY file edit, `keel compile` runs automatically via hooks
3. If errors returned, FIX THEM immediately
4. Type hints mandatory. Docstrings mandatory on public functions.
5. Check map before creating new functions. Place in correct module.

## Commands:
- `keel discover <hash>` — callers, callees, module context
- `keel compile <file>` — validate (auto-runs via hooks, can also run manually)
- `keel where <hash>` — resolve hash to file:line
```

**Known limitation (Cursor v2.0+):** The `agent_message` / `userMessage` field in hook responses is ignored in Cursor v2.0+. keel's blocking message (why the edit was rejected) may not be visible to the Cursor agent. Workaround: keel writes error context to a temporary file (`.keel/last-error.json`) that the Cursor rules file (`.cursor/rules/keel.mdc`) instructs the agent to read after a block.

### 9.6 Google Antigravity (Tier 2 — Cooperative + Safety Net)

Antigravity uses two mechanisms: Rules (always-on system instructions) and Skills (on-demand capabilities). keel uses both.

**`.agent/rules/keel.md`** (workspace rule — always active):

```markdown
# keel Code Graph Enforcement

This project uses keel. After EVERY file edit, run `keel compile <file> --json`.
Fix all errors before proceeding. Type hints and public docstrings are mandatory.
Before editing functions with upstream callers, run `keel discover <hash>`.
```

**`.agent/skills/keel/SKILL.md`** (skill — agent-triggered on code changes):

```markdown
---
name: keel-enforcement
description: Use this skill when editing code, creating new functions, or refactoring. Validates structural integrity via keel code graph enforcement.
---

# keel Code Graph Enforcement Skill

## When editing existing code:
1. Find the function hash from the keel map (run `keel map --llm` if not in context)
2. Run `keel discover <hash>` to see callers and callees
3. Make the edit
4. Run `keel compile <changed-file> --json`
5. Fix any errors before continuing

## When creating new functions:
1. Run `keel map --llm` and check if a similar function exists
2. Place in the module with the best semantic fit
3. Add type hints on all parameters and return type
4. Add docstring if the function is public
5. Run `keel compile <file> --json` to validate placement

## Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel compile <file> --json` — validate changes
- `keel explain <error-code> <hash>` — inspect resolution reasoning (see §4.9)
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — token-optimized codebase map
```

### 9.7 Gemini CLI (Tier 1 — Enforced)

Gemini CLI shipped full hooks in v0.26.0 with 8 event types and exit-code-2 blocking.

**`.gemini/settings.json`** (generated by `keel init`):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "command": "keel map --llm",
        "type": "command"
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "command": "\"$PROJECT_DIR\"/.keel/hooks/post-edit.sh",
        "type": "command"
      }
    ]
  }
}
```

**`GEMINI.md`** — supplementary instructions following the shared template (§9.12).

**Gemini CLI unique — AfterAgent self-correction loop:** Gemini CLI's `AfterAgent` event with exit code 2 triggers an automatic retry turn — the agent self-corrects without user intervention. keel can use this to inspect code post-generation, reject it, and force the agent to fix it in a continuous loop until all contracts pass. This is architecturally superior to Claude Code's blocking model (which requires the LLM to decide to re-edit after seeing the error). keel should expose an `AfterAgent` hook in the generated `.gemini/settings.json` to enable this self-correction loop.

### 9.8 Windsurf (Tier 1 — Enforced)

Windsurf shipped Cascade Hooks in late 2025 with 11 event types and exit-code-2 pre-hook blocking.

**`.windsurf/hooks.json`** (generated by `keel init`):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "command": "keel map --llm",
        "type": "command"
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
        "command": "\"$PROJECT_DIR\"/.keel/hooks/post-edit.sh",
        "type": "command"
      }
    ]
  }
}
```

> **Note:** Windsurf uses `PreToolUse` (not `PostToolUse`) for blocking hooks. The hook script validates the previous edit on the next edit trigger. `.windsurfrules` file provides supplementary instructions.

### 9.9 Aider (Tier 2 — Cooperative)

Aider has no hook system. It uses `map-tokens` and `map-refresh` settings (NOT `map-file` — that option doesn't exist) for context injection.

**`.aider.conf.yml`** (generated by `keel init`):

```yaml
# keel integration — cooperative enforcement
map-tokens: 2048
map-refresh: auto
```

Instruction content follows the shared template (§9.12), delivered via Aider's system prompt configuration.

### 9.10 Letta Code (Tier 1 — Enforced)

Letta Code has ~12 hook events with exit-code-2 blocking — the same semantics as Claude Code. `keel init` generates hook configuration when Letta is detected.

**Hook config:** keel registers as a hook provider in Letta's configuration, using the same `post-edit.sh` script shared across all Tier 1 tools. Events: `PreToolUse`, `PostToolUse`, `SessionStart`, `SessionEnd`, `PermissionRequest`, `Notification`, `Stop`, `SubagentStop`, `PreCompact`, `Setup`, and others.

**Instruction delivery:** Via Letta's instruction configuration, following the shared template (§9.12).

**Strategic note:** Letta's memory-first architecture (persistent architectural learning) is complementary to keel's enforcement. Letta remembers past decisions; keel enforces current contracts. Together they create an agent with both institutional memory and structural discipline.

### 9.11 GitHub Copilot (Tier 2 — Cooperative + Governance)

GitHub Copilot has MCP-based policies with JSON `permissionDecision: "deny"` — a different mechanism than exit-code-2, but representing the largest market share of any AI coding tool. Supporting Copilot — even at Tier 2 — dramatically expands keel's addressable market.

**Integration approach:** keel as MCP policy provider, using Copilot's registry policies. keel exposes policy endpoints that Copilot queries before executing code modifications.

**Instruction delivery:** Via `.github/copilot-instructions.md` (Copilot's custom instructions file) and Copilot workspace settings. `keel init` generates Copilot-specific config when `.github/copilot-instructions.md` or Copilot workspace settings are detected.

**Limitations:** JSON-based governance, not scriptable exit-code-2 hooks. keel cannot force Copilot to stop mid-generation and fix errors the way it can with Tier 1 tools. The git pre-commit hook remains the enforcement safety net for Copilot users.

### 9.12 Shared Instruction Template

All instruction files share the same core content. `keel init` generates tool-specific wrappers around this template:

```markdown
## keel — Code Graph Enforcement

This project uses keel (keel.engineer) for code graph enforcement.

### Before editing a function:
- Before changing a function's **parameters, return type, or removing/renaming it**, run `keel discover <hash>` to understand what depends on it. The hash is shown in the keel map (injected at session start or embedded below).
- For **body-only changes** (bug fixes, refactoring internals, improving logging), skip discover — compile will catch any issues.
- If the function has upstream callers (↑ > 0), you MUST understand them before changing its interface.

### After every edit:
- Run `keel compile <file> --json` (automatic via hooks in Claude Code, manual in other tools)
- If it returns errors, FIX THEM before doing anything else. Follow the `fix_hint` in the error output.
- Type hints are mandatory on all functions
- Docstrings are mandatory on all public functions
- If a warning has `confidence` < 0.7, attempt one fix. If it doesn't resolve, move on — the resolution may be incorrect.

### If compile keeps failing (circuit breaker):
- If `keel compile` returns the same error after your fix attempt, follow keel's escalation instructions at each step:
  1. **First failure:** Fix using the `fix_hint` provided
  2. **Second failure (same error):** Run `keel discover <hash> --depth 2` as instructed — the issue may be upstream
  3. **Third failure (same error):** keel auto-downgrades to WARNING. Run `keel explain <error-code> <hash>` to inspect the resolution chain. Add findings as a code comment so the next session can resolve with full context.
- Downgraded errors re-enforce on next session — they are not permanently suppressed.

### Before creating a new function:
1. Check the keel map to see if a similar function already exists
2. Place the function in the module where it logically belongs
3. If keel warns about placement, move the function to the suggested module

### When scaffolding (creating multiple new files at once):
1. Run `keel compile --batch-start` before creating files
2. Create files normally — structural errors (broken callers) still fire immediately
3. Type hint and docstring errors are deferred until batch ends
4. Run `keel compile --batch-end` when scaffolding is complete — all deferred validations fire

### Commands:
- `keel discover <hash>` — show callers, callees, and module context
- `keel compile <file>` — validate changes
- `keel compile --batch-start` / `--batch-end` — batch mode for scaffolding sessions
- `keel explain <error-code> <hash>` — inspect resolution reasoning (see §4.9)
- `keel where <hash>` — resolve hash to file:line
- `keel map --llm` — regenerate the LLM-optimized map
```

### 9.13 How Backpressure Works

**With Tier 1 tools (Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code — enforced):**

1. Developer gives task → SessionStart hook injects map into context
2. Instruction file instructs: call `discover` before editing functions with callers
3. LLM makes an edit → PostToolUse hook auto-runs `compile`
4. If errors → hook exits code 2 → stderr shown to LLM → must fix before proceeding
5. LLM fixes → `compile` passes → continues

This is the same flow in all 5 tools. The hook config format varies (`.claude/settings.json`, `.cursor/hooks.json`, `.gemini/settings.json`, `.windsurf/hooks.json`, Letta config) but the mechanism is identical: exit code 2 blocks, stderr is shown to the LLM.

**With Tier 2 tools (GitHub Copilot, Codex, Antigravity, Aider — cooperative + governance + safety net):**

1. Developer gives task → instruction file / MCP policy provides map + workflow rules
2. LLM follows instructions: calls `discover`, then edits, then runs `compile`
3. If LLM skips `compile` → git pre-commit hook catches violations at commit time
4. If LLM skips pre-commit (force push) → CI catches violations at build time

**Batch scaffolding:** When an LLM is creating multiple files at once, per-edit enforcement creates noise. The LLM can call `keel compile --batch-start` to defer non-structural validations (type hints, docstrings, placement) until `keel compile --batch-end`. Structural errors (broken callers, removed functions) still fire immediately during batch mode. See §20.7 for full batch mode UX.

**Phase 1 includes `fix_hint` on every error** (see §12) — keel already emits fix instructions like "Update 3 callers to pass Password instead of string: handleLogin at src/routes/auth.ts:23…" so the LLM can act on them immediately. Combined with the **circuit breaker** (see §4.10), the LLM gets progressively smarter guidance on repeated failures: fix hint → wider discover → explain resolution chain → auto-downgrade and document.

**Phase 2 explores full auto-correction:** keel emits structured code transforms (not just text hints) that tools with retry loops (e.g., Gemini's AfterAgent — see §9.7) can apply autonomously.

**Enforcement layers (defense in depth):**

1. **Proactive (instruction files):** LLM instructed to use `discover` before editing. Cooperative. All tools.
2. **Reactive (tool hooks):** `compile` auto-runs after every edit. **Enforced** in Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code. Governance-level in GitHub Copilot. Not available in Codex, Antigravity, Aider.
3. **Commit gate (git pre-commit hook):** `compile --strict` blocks commit. Catches everything that slipped past hooks.
4. **Build gate (CI):** `keel map --json --strict` fails the build. Final safety net.

### 9.14 MCP Server (`keel serve`)

`keel serve --mcp` exposes all CLI commands as MCP tools over stdio. Any tool that supports MCP (Claude Code, Antigravity, Cursor, Codex) can use keel as a tool server.

**Exposed tools:**

- `keel_discover` — takes hash, returns adjacency + module context
- `keel_compile` — takes file path(s), returns validation results (with `fix_hint`, `confidence`, `resolution_tier` — see §12)
- `keel_explain` — takes error code + hash, returns resolution chain (see §4.9)
- `keel_where` — takes hash, returns file:line
- `keel_map` — returns current graph (LLM-optimized or JSON)

**Also supports:**

- `keel serve --http` — HTTP API on localhost for custom integrations
- `keel serve --watch` — file system watcher, auto-runs `compile` on save

The MCP server is a thin wrapper (~300 lines) over the CLI. It adds file watching and persistent graph state in memory (vs. CLI's load-from-SQLite-per-call).

### 9.15 VS Code Extension

A lightweight VS Code extension (`keel-vscode`) providing:

- **Status bar indicator:** Shows keel graph status (✓ clean, ⚠ warnings, ✗ errors)
- **Inline diagnostics:** keel `compile` errors shown as VS Code diagnostic markers (red/yellow squiggles)
- **CodeLens:** Shows `↑N ↓M` (caller/callee counts) above each function
- **Command palette:** `keel: Discover`, `keel: Compile`, `keel: Show Map`
- **Hover info:** Hover a function → shows hash, callers, callees, module context

**Implementation:** Thin client that calls `keel serve --http` (auto-started). All intelligence lives in the Rust binary. The extension is ~500 lines of TypeScript — display layer only.

**Works with:** VS Code, Cursor (VS Code fork), Antigravity (VS Code fork), Windsurf (VS Code fork). One extension, four IDEs.

### 9.16 CI Integration

`keel init` generates a CI configuration snippet for common providers:

**GitHub Actions (`.github/workflows/keel.yml`):**

```yaml
name: keel
on: [push, pull_request]
jobs:
  keel:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install keel
        run: curl -fsSL https://keel.engineer/install.sh | sh
      - name: Validate codebase
        run: keel map --json --strict
```

The `--strict` flag exits non-zero if any `ERROR`-level violations exist. This catches anything that slipped past hooks and pre-commit.

## 10. Architecture

### Language: Pure Rust

**Rust stack:**

- **tree-sitter** — parses source files into concrete syntax trees. The universal foundation for all code analysis (Tier 1).
- **Oxc** (`oxc_resolver` + `oxc_semantic`) — TypeScript/JavaScript resolution engine. Rust-native, MIT-licensed, production-ready v0.111+, 30x faster than webpack's resolver. Used in production by Rolldown and Biome.
- **ty** — Python type checking and resolution. Subprocess in Phase 1 (`ty --output-format json`), library integration in Phase 2 when API stabilizes. Built on Salsa incremental framework, 4.7ms incremental updates (80x faster than Pyright).
- **rust-analyzer** (`ra_ap_ide` crates) — Rust resolution engine. Lazy-loaded due to 60s+ startup. API explicitly unstable (0.0.x).
- **petgraph** — graph data structure for keel's function/class/module graph with call edges, import edges, and enforcement metadata.
- **xxhash-rust** — hash computation (>10GB/s). See §5 for hash design and §10.3 for collision handling.
- **rusqlite** (with `bundled` feature) — SQLite for graph storage. Stores keel graph, metadata (hashes, enforcement flags, placement profiles, external endpoints), and resolution caches.
- **clap** — CLI argument parsing
- **serde** / **serde_json** — JSON serialization for `--json` output

**Why pure Rust?** Eliminates FFI complexity. tree-sitter is natively C/Rust. Oxc is native Rust. SQLite statically linked. Single binary with zero runtime dependencies (ty and rust-analyzer invoked as subprocesses where needed).

**Binary size note:** 4 languages of tree-sitter grammars + Oxc crates + SQLite + the resolution engine. Expected range: 20-35MB. Acceptable for a developer tool (comparable to ripgrep + tree-sitter builds). Investigate stripping and LTO if it exceeds 40MB.

### 10.1 Resolution Engine — Converged 3-Tier Architecture

> **stack-graphs is dead.** GitHub archived the repository on September 9, 2025. Zero active forks. The "universal DSL" model failed — the industry has corrected toward fast, language-specific tooling. keel's architecture embraces this reality.

Three independent deep-research sources (Perplexity, Gemini, Claude) converge on the same conclusion: **the hybrid 3-tier approach with per-language engines is the only viable path.** No single engine meets all requirements simultaneously. The performance targets (<5s/100k LOC, <200ms incremental, <2GB memory) rule out LSP and SCIP as primary engines. The precision target (>90%) rules out tree-sitter heuristics alone.

#### Tier 1 — Universal fast path (tree-sitter)

- Parse all files with tree-sitter
- Extract definitions, call sites, imports via query patterns (leveraging `tags.scm`)
- Build file-level index with incremental updates in <1ms
- Use rayon for parallel parsing
- **Resolves ~75-92% of cross-file references** depending on language
- Go is nearly complete at this tier; other languages need enhancement

#### Tier 2 — Per-language enhancers (native Rust where available)

| Language | Enhancer | Status | Precision (with Tier 1+2) | Notes |
|----------|----------|--------|---------------------------|-------|
| **TypeScript** | **Oxc** (`oxc_resolver` + `oxc_semantic`) | Production-ready v0.111+, MIT, 30x faster than webpack | ~85-93% | Barrel files and re-exports handled by `oxc_resolver`. `oxc_semantic` is strictly per-file — cross-file stitched with tree-sitter queries. |
| **Python** | **ty** subprocess (Phase 1), library (Phase 2) | Beta v0.0.15, multiple releases/week, Salsa incremental, 4.7ms updates | ~82-99% (ty-dependent) | Not yet consumable as Rust library (crates inside ruff monorepo, not on crates.io). Subprocess via `ty --output-format json` for Phase 1. |
| **Go** | Tree-sitter heuristics alone | Stable | ~85-92% | Go's explicit imports, package scoping, and capitalization convention make heuristics work. No Go analysis library exists for Rust (FFI impractical). |
| **Rust** | **rust-analyzer** (`ra_ap_ide` crates) | API unstable (0.0.x), architecturally mature | ~75-99% (r-a dependent) | Lazy-loaded due to 60s+ startup for large workspaces. Has built-in SCIP emission. |

#### Tier 3 — On-demand fallback (LSP/SCIP)

For references flagged as ambiguous by Tier 1+2:
- Multiple candidate definitions
- Method calls on unresolved types (`foo.bar()` where foo's type is unknown)
- Star imports, dynamic imports, conditional imports
- Trait/interface dispatch

Resolution approach:
- Query LSP server or pre-built SCIP index
- Cache results aggressively
- **Not always-on** — a precision knob the user can enable
- Lifts precision to **>95%** where needed

**Dynamic dispatch enforcement implications:** When Tier 1+2 cannot confidently resolve a call edge (e.g., trait dispatch, interface method calls, dynamic imports), the edge is marked with a `confidence` field in compile output. Low-confidence edges produce `WARNING`, not `ERROR` — preventing false positives from blocking the LLM. When Tier 3 (LSP/SCIP) is enabled, it can promote these to `ERROR` by confirming the resolution with full type information.

#### Why this architecture

| Approach | 100k LOC Parse | Incremental | Memory | Precision | Verdict |
|----------|---------------|-------------|--------|-----------|---------|
| Pure tree-sitter | 2-3s | 10-50ms | ~300MB | 65-85% | Too imprecise for enforcement |
| Pure LSP | 4-6s startup/server | 100-400ms | 5-13GB (4 servers) | 95-100% | Memory/startup impossible |
| Pure SCIP | 20-100s indexing | No incremental | ~400MB | 95-98% | Too slow, no incrementality |
| **Hybrid 3-tier (chosen)** | **1-5s** | **10-80ms** | **200MB-1.5GB** | **~87-95%** | **Only viable path** |

### 10.2 What the resolution engine doesn't affect

The resolution engine is an internal implementation detail. Everything above it is unchanged:

- **keel graph schema** (§11) — function nodes, call edges, module profiles
- **Commands** (§4) — init, map, discover, compile, where, serve
- **Enforcement logic** — type hint validation, adjacency checking, placement scoring
- **Output formats** (§6) — JSON, LLM-optimized, human CLI
- **Tool integration** (§9) — hooks, instruction files, MCP server
- **Hash design** (§5) — content-addressed hashes independent of resolution

The resolution engine plugs into one interface: given a call site in file A, resolve which function definition it points to. Everything else consumes that resolution.

### 10.3 Hash Collision Handling

xxHash64 provides 64-bit hashes (2^64 space). For a codebase with 10,000 functions, birthday paradox probability of collision is ~2.7 × 10^-12 — negligible. However, keel must handle collisions defensively:

- **Detection:** When computing a new hash, check if it already exists in the graph for a different function. If so, append a disambiguator (e.g., file path hash) and re-hash.
- **Reporting:** If a collision is detected and resolved, emit `INFO: Hash collision detected and resolved for function 'X' in file 'Y'.` Collisions should be vanishingly rare; if they're frequent, the hashing input is wrong.
- **Invariant:** No two distinct functions in the graph may share the same hash. This is enforced at write time.

### Build and distribution

- Single binary via `cargo build --release`
- Pre-built binaries for Linux (x86_64, arm64), macOS (arm64, x86_64), Windows (x86_64)
- Install via: `curl -fsSL https://keel.engineer/install.sh | sh` or `brew install keel` or `cargo install keel` or `winget install keel` or `scoop install keel`
- No runtime dependencies — tree-sitter grammars compiled in, SQLite statically linked
- Windows: native binary, no WSL required. Path handling uses platform-native separators internally, forward slashes in all output.

### Supported languages

**Phase 1:** TypeScript, Python, Go, Rust (covers most LLM-driven development) **Phase 2:** Java, C#, C++, PHP, Ruby, Kotlin, Swift

## 11. Graph Schema

### Node types

```rust
enum NodeKind {
    Module,    // A file or directory-level module
    Class,     // A class, struct, trait, interface
    Function,  // A standalone function or method
}

struct GraphNode {
    id: u64,                     // Internal graph ID
    hash: String,                // base62(xxhash64(...)), 11 chars
    kind: NodeKind,
    name: String,                // Function/class/module name
    signature: String,           // Full normalized signature (e.g., "login(email: str, pw: str) -> Token")
    file_path: String,           // Relative to project root
    line_start: u32,
    line_end: u32,
    docstring: Option<String>,   // First line of docstring, if present
    is_public: bool,             // Exported / public visibility
    type_hints_present: bool,    // All params and return type annotated?
    has_docstring: bool,         // Docstring present?
    external_endpoints: Vec<ExternalEndpoint>,  // HTTP routes, gRPC, etc.
    previous_hashes: Vec<String>, // Last 3 hashes for rename tracking
    module_id: u64,              // Parent module node ID
}

struct ExternalEndpoint {
    kind: String,     // "HTTP", "gRPC", "GraphQL", "MessageQueue"
    method: String,   // "POST", "GET", etc. (for HTTP)
    path: String,     // "/api/users/:id"
    direction: String, // "serves" or "calls"
}
```

### Edge types

```rust
enum EdgeKind {
    Calls,      // Function A calls function B
    Imports,    // Module A imports from module B
    Inherits,   // Class A extends/implements class B
    Contains,   // Module contains function/class
}

struct GraphEdge {
    source_id: u64,
    target_id: u64,
    kind: EdgeKind,
    file_path: String,  // Where the reference occurs
    line: u32,          // Line number of the reference
}
```

### Module placement profile

```rust
struct ModuleProfile {
    module_id: u64,
    path: String,                    // e.g., "src/auth/"
    function_count: u32,
    function_name_prefixes: Vec<String>,  // Common prefixes (e.g., ["validate", "check", "verify"])
    primary_types: Vec<String>,      // Most-used types in signatures
    import_sources: Vec<String>,     // Modules this module imports from
    export_targets: Vec<String>,     // Modules that import from this module
    external_endpoint_count: u32,
    responsibility_keywords: Vec<String>, // Derived from function names + docstrings
}
```

## 12. JSON Output Schemas

### `keel compile` error output

This is the schema Claude sees via stderr when the PostToolUse hook fires with exit code 2. Every error/warning includes `confidence` (0.0–1.0) and `resolution_tier` (see §10.1) so the LLM can gauge reliability. Every `ERROR`-level violation includes a `fix_hint` — a simple text instruction telling the LLM what to do. `WARNING`-level violations include `fix_hint` where applicable. These fields feed the circuit breaker escalation (see §4.10).

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

**Note on `info` block:** When compile passes with zero errors and zero warnings, stdout is empty (exit 0, no output). The `info` block is only emitted with `--verbose` or when errors/warnings are present. Info data is always written to `graph.db`. See §4.4.

### Error codes

|Code|Category|Severity|Description|
|---|---|---|---|
|E001|broken_caller|ERROR|Function signature changed, callers expect old signature|
|E002|missing_type_hints|ERROR|Function parameters or return type lack type annotations|
|E003|missing_docstring|ERROR|Public function has no docstring|
|E004|function_removed|ERROR|Function was deleted but still has callers|
|E005|arity_mismatch|ERROR|Function parameter count changed, callers pass wrong number of arguments|
|W001|placement|WARNING|Function may be better placed in a different module|
|W002|duplicate_name|WARNING|Function with same name exists elsewhere in the codebase|
|W003|naming_convention|WARNING|Function name doesn't match module's naming pattern (Phase 2)|
|W004|cross_repo_endpoint|WARNING|Changed endpoint is consumed by a linked repo (Phase 2)|
|S001|suppressed|INFO|Violation suppressed via inline `# keel:suppress` or `[suppress]` config. Logged for visibility but does not block.|

### Common fields on all error/warning objects

|Field|Type|Required|Description|
|---|---|---|---|
|`confidence`|float 0.0–1.0|Always|How confident keel is in the resolution. 1.0 = certain (e.g., tree-sitter found the node). < 0.7 = heuristic/ambiguous. Feeds instruction template guidance: "If confidence < 0.7, attempt one fix. If unresolved, move on."|
|`resolution_tier`|string enum|Always|Which resolution tier produced the evidence. One of: `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip`. See §10.1.|
|`fix_hint`|string|ERROR: always. WARNING: where applicable.|Simple text instruction telling the LLM what to do. Not structured code transforms — just a human-readable (LLM-readable) description of the fix. Feeds the circuit breaker escalation at attempt 1 (see §4.10).|

### `keel explain` JSON output

Returns the resolution chain — the evidence keel used to determine a dependency. Used by the circuit breaker at attempt 3 (§4.10) and whenever the LLM needs to diagnose a potential false positive. Canonical schema; §4.9 contains usage context.

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

|Field|Type|Required|Description|
|---|---|---|---|
|`error_code`|string|Always|The error code being explained (e.g., `E001`)|
|`hash`|string|Always|The function hash keel flagged|
|`confidence`|float 0.0–1.0|Always|How confident keel is in the resolution|
|`resolution_tier`|string enum|Always|Which tier produced the evidence (see §10.1)|
|`resolution_chain`|array|Always|Ordered list of evidence steps. Each has `kind` (import/call/type_ref/re_export), `file`, `line`, `text`|
|`summary`|string|Always|Human/LLM-readable summary of the resolution reasoning|

### `keel discover` JSON output

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

### `keel map --json` output

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

## 13. Configuration Schema

`.keel/config.toml` — committed to git, shared across team:

```toml
[keel]
version = "2.0"

[languages]
# Languages to parse. Auto-detected if omitted.
enabled = ["typescript", "python", "go", "rust"]

[enforcement]
# Type hint enforcement for NEW/MODIFIED code: "error" | "warning" | "off"
type_hints = "error"

# Type hint enforcement for PRE-EXISTING code: "error" | "warning" | "off"
# Progressive adoption: starts as "warning", escalate to "error" when ready
type_hints_existing = "warning"

# Docstring enforcement for NEW/MODIFIED public functions: "error" | "warning" | "off"
docstrings = "error"

# Docstring enforcement for PRE-EXISTING public functions
docstrings_existing = "warning"

# Minimum docstring — just the first line, or full format?
docstring_format = "first_line"  # "first_line" | "full" (full = params + returns documented)

# Placement validation level: "warning" | "off"
# (Phase 2: "error" for strict placement rules)
placement = "warning"

# Duplicate function name detection: "warning" | "off"
duplicate_detection = "warning"

[discovery]
# Default depth for `keel discover`
default_depth = 1
# Maximum allowed depth
max_depth = 5

[map]
# Maximum tokens for --llm output
max_tokens = 15000
# Scoped modules to include (empty = all)
default_scope = []

[circuit_breaker]
# Maximum consecutive failures on same error-code + hash before auto-downgrade to WARNING.
# Default is intentionally low — 3 retries is enough to distinguish real errors from false positives.
max_retries = 3
# Auto-downgrade errors to WARNING after max_retries consecutive failures.
# Downgraded errors re-enforce on next session.
auto_downgrade = true

[hooks]
# Auto-generate hook configs for enforced tools (Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code)
generate_enforced_hooks = true
# Auto-generate instruction files for all detected tools
generate_instruction_files = true
# Install git pre-commit hook on `keel init`
install_git_hooks = true
# Auto-detect and generate configs for all found LLM tools
auto_detect_tools = true
# Explicitly enable/disable specific tool integrations
# (overrides auto-detection)
# tools = ["claude-code", "codex", "cursor", "antigravity", "gemini-cli", "windsurf", "letta", "github-copilot", "aider"]

[exclude]
# Gitignore-syntax patterns for files/directories keel should ignore entirely.
# These are not parsed, not added to the graph, not enforced.
patterns = [
    "generated/**",
    "vendor/**",
    "node_modules/**",
    "**/migrations/**",
    "**/*.generated.ts",
    "**/*.pb.go",
    "*_test.go",       # Go test files (optional — remove if you want test enforcement)
]

[suppress]
# Persistent false-positive suppressions. Use when inline `# keel:suppress` is impractical
# (e.g., generated code you don't control, third-party patterns).
# Each entry requires a reason field — unexplained suppressions are rejected.
# "src/legacy/adapter.ts:validateInput" = { codes = ["W001"], reason = "Adapter pattern — intentionally cross-cutting" }
# "src/utils/index.ts:*" = { codes = ["W001"], reason = "Utility barrel file — placement scoring not meaningful" }

[enforcement.overrides]
# Per-glob enforcement rules. Overrides the top-level [enforcement] settings for matching paths.
# Useful for tests, scripts, fixtures, and monorepo packages with different standards.
"tests/**" = { type_hints = "warning", docstrings = "off" }
"scripts/**" = { type_hints = "warning", docstrings = "off" }
"fixtures/**" = { type_hints = "off", docstrings = "off", placement = "off" }

[naming]
# Phase 2: naming convention enforcement
# convention = "snake_case"  # "snake_case" | "camelCase" | "PascalCase" | "auto-detect"
# scope = "warning"  # "error" | "warning" | "off"

# Phase 2: custom placement rules
# [placement.rules]
# "repository/" = { must_contain = ["database", "query", "repository"] }
# "service/" = { must_contain = ["service", "use_case", "handler"] }
```

## 14. Naming Conventions (Phase 2)

Naming convention enforcement is deferred to Phase 2 but designed now.

### Approach

1. **Auto-detection:** `keel init` analyzes the existing codebase and detects the dominant naming convention per language (e.g., Python → snake_case, TypeScript → camelCase).
2. **Configuration override:** The engineer can set explicit conventions in `config.toml`.
3. **Enforcement:** When the LLM creates a new function, keel checks the name against the convention. Violations are `WARNING` by default, configurable to `ERROR`.

### What's validated

- Function names match convention (snake_case, camelCase, PascalCase)
- Class names are PascalCase (universal convention)
- Module/file names match language convention
- Consistency within a module (all functions in the same module should use the same convention)

### Why Phase 2

Naming conventions interact with language-specific idioms in complex ways. Getting auto-detection right requires testing across many codebases. The structural enforcement (type hints, docstrings, placement, adjacency) is higher-value and lower-risk for Phase 1.

## 15. Licensing and Business Model

### License: Functional Source License (FSL)

FSL (used by Sentry, GitButler, Codecov, Convex, PowerSync, Liquibase — 10+ products as of Feb 2026) provides:

- **Free for individual developers and internal use** — no restrictions
- **Free for non-competing commercial use** — companies can use it internally
- **Paid license required** for companies embedding keel in products they sell or offer as a coding agent service (e.g., Lovable, Replit, Bolt, Vercel v0)
- **Converts to Apache 2.0 after 2 years** — guaranteeing eventual full open source
- SPDX-recognized, giving institutional legitimacy. Enterprise compliance teams accept FSL because of clear "Competing Use" definition and fixed change date.

**FSL risk (Liquibase precedent):** When Liquibase adopted FSL (Sept 2025), it triggered blocking issues in CNCF-governed projects (Keycloak, Apache Fineract, Spring Boot) that cannot accept FSL dependencies under foundation governance. This risk is low for keel because keel is a standalone CLI tool, not an embeddable library dependency. If keel ever ships as a library (e.g., for embedding in Lovable/Replit), the license model must be reconsidered.

### Pricing model: Two tracks

**Track 1: Building software (any kind) — FREE**

Individual developers, teams, and companies using keel to build their own products: **$0**. No per-developer fees. No team size limits. No ARR gates. Telemetry opt-in (helps improve keel, not required). This is the FSL license in action — free for internal use.

**Track 2: Embedding keel in coding agent products — PAID (OEM/Embedding license)**

Companies that embed keel in products they sell or offer as coding agent services (e.g., Lovable, Replit, Bolt, Vercel v0, any "AI coding platform"). Pricing based on the embedding company's ARR:

| Embedding Company ARR | Monthly License |
|---|---|
| <$1M | Free |
| $1M - $5M | $500/month |
| $5M - $25M | $2,500/month |
| $25M - $100M | $10,000/month |
| >$100M | Enterprise pricing (~$50k+/month, negotiated) |

**Why this model works:**

- **Zero friction for adoption** — every developer, every team, every company building software can use keel for free. Eliminates the "per-dev cost vs. competitors" comparison entirely (keel isn't competing with Snyk $25/dev or Greptile $30/dev because it's free for that use case).
- **Revenue from commercial embedding** — companies that derive commercial value from embedding keel's enforcement in their paid products pay based on their own scale. Lovable at $100M+ ARR saves millions in token costs and code quality — $50k/month is trivial ROI.
- **Aligned incentives** — keel becomes the standard enforcement layer precisely because it's free to use. Revenue grows as the AI coding platform market grows.
- **FSL cleanly enforces this** — "Competing Use" = embedding keel in a coding agent product you sell.

### Revenue justification

**Embedding revenue:** The AI coding platform market is growing rapidly. If 5 platforms at $5M-$25M ARR each pay $2,500/month + 2 platforms at $25M-$100M pay $10,000/month, that's $32,500/month ($390k/year) from just 7 customers. As the market matures and more platforms embed keel, revenue scales with their ARR.

**Token savings for embedders:** If keel reduces LLM token usage by 20% (conservative, given Aider's demonstrated 4-6% context usage vs. 50-70% for naive approaches), a platform processing 1M requests/day at $0.01/request saves $60k/month. The license pays for itself in days.

**Free tier drives adoption:** Bottom-up adoption by individual developers and teams creates the installed base that makes embedding compelling. Platforms embed keel because their users already use it.

**Revenue concentration risk (honest acknowledgment):** All revenue comes from the embedding tier. There is no Team/Enterprise tier for companies using keel internally — this is intentional. The philosophy is maximum adoption: free for every developer and every team building software, paid only when a company embeds keel in a product they sell. This means revenue is concentrated in a small number of AI platform customers. The bet: keel's value is in becoming the standard enforcement layer, not in monetizing builders. Revenue scales as the AI coding platform market grows. If the embedding market doesn't materialize at scale, keel remains a widely-adopted open infrastructure tool with optionality to introduce premium features (telemetry dashboards, hosted graph services) without breaking the free-for-builders promise.

## 16. Risks and Mitigations

### Technical risks

|Risk|Severity|Mitigation|
|---|---|---|
|**Hybrid 3-tier architecture complexity** — 4 languages each with different enhancers (Oxc, ty, tree-sitter-only, rust-analyzer)|**High**|Per-language implementation is more work than a single universal engine, but produces better precision and avoids LSP memory/startup problems. Go is simplest (tree-sitter heuristics), TypeScript next (Oxc production-ready), Python depends on ty stability, Rust depends on lazy-loaded rust-analyzer. See §10.1.|
|Resolution engine precision below 90% for some languages|**High**|Tier 2 enhancers target ~85-95% per language. Tier 3 (LSP/SCIP) available as optional precision knob for ambiguous cases. Measure precision against LSP ground truth in test corpus.|
|**ty (Python) is beta** — v0.0.15, API not stable, crates not on crates.io|**Medium**|Phase 1 uses ty as subprocess (`ty --output-format json`), not library. If ty unavailable, fall back to tree-sitter heuristics + Pyright LSP. Library integration deferred to Phase 2 when API stabilizes.|
|False positives in placement suggestions annoy developers|Medium|Phase 1: WARNING only. Conservative thresholds. Easy to disable in config.toml. Utility/orchestrator modules excluded from scoring via `[exclude]` patterns. Persistent false positives suppressible via inline `# keel:suppress` or `[suppress]` config (see §13).|
|False positives in adjacency validation|Medium|ERROR only on provably-broken contracts (changed arity, removed function). Ambiguous resolution edges (dynamic dispatch, trait dispatch) enforced at WARNING, not ERROR. Persistent false positives suppressible via `# keel:suppress` with reason field. Type mismatches start as WARNING.|
|Type hint enforcement on existing untyped codebase is too strict|Medium|Progressive enforcement: WARNING for pre-existing code, ERROR for new code. Configurable escalation schedule in config.toml. Resolved from v1 contradiction (§2 said "no grandfathering" vs §16 `--adopt` mode).|
|Binary size with 4 language grammars + Oxc crates + SQLite + resolution engine|Medium|Expected 20-35MB. Acceptable for developer tooling. Investigate LTO + stripping if >40MB.|
|`deinit` complexity — modifying 10+ config formats (JSON, YAML, Markdown, TOML, MDC) reliably|Medium|Use format-aware parsers, not regex. JSON → serde_json, TOML → toml crate, YAML → serde_yaml. For Markdown files, use section markers (`<!-- keel:start -->` / `<!-- keel:end -->`) for reliable removal.|
|Graph size for very large codebases (>1M LOC)|Medium|Scoped maps, lazy loading, graph partitioning. Phase 2.|
|Concurrent LLM sessions editing same codebase|Low|File-level locking on SQLite. `compile` is <200ms, lock contention unlikely.|
|**Cursor v2.0+ ignores `agent_message` field** — keel cannot explain *why* it blocked in Cursor|Medium|Workaround: keel writes error context to `.keel/last-error.json`, Cursor rules file instructs agent to read it after a block. Monitor Cursor releases for fix.|

### Product risks

|Risk|Severity|Mitigation|
|---|---|---|
|LLM tools build this natively (Cursor/Antigravity add graph enforcement)|**High**|Ship fast. First-mover in enforcement. keel works across ALL tools simultaneously — no single vendor can match cross-tool enforcement. Cross-tool universality is the moat.|
|Augment Context Engine makes keel's navigation pillar irrelevant|**High**|Reposition around enforcement (done in v2). Augment provides context. keel provides enforcement. Complementary, not competitive. "Augment gives your agent perfect recall. keel gives it perfect discipline."|
|Low adoption because value isn't visible until LLM uses it|High|Killer demo: side-by-side of LLM with keel vs. without on a real codebase. Measure token savings + error prevention. Telemetry dashboard (§22) makes value visible.|
|Backpressure slows down LLM task completion|Medium|Measure actual overhead. If `compile` is <200ms, it adds <1% to task time. Preventing one broken deploy saves hours.|
|FSL license deters open-source contributors|Low-Medium|FSL → Apache 2.0 after 2 years. Core contributors get CLA with immediate Apache rights.|

## 17. MVP Scope (Phase 1)

### In scope

- `init`, `map`, `discover`, `compile`, `where`, `explain`, `serve` commands
- 4 languages: TypeScript, Python, Go, Rust
- 3-tier hybrid resolution engine per §10.1 (tree-sitter + per-language enhancers + optional LSP/SCIP fallback)
- **Multi-tool integration:** Claude Code hooks, Cursor hooks, Gemini CLI hooks, Windsurf hooks, Letta Code hooks (all Tier 1 enforced), GitHub Copilot MCP policies, Codex AGENTS.md, Antigravity rules + skills, Aider (Tier 2 cooperative)
- `keel init` auto-detects installed tools and generates all configuration files
- MCP server (`keel serve --mcp`) + HTTP server (`keel serve --http`) + file watcher (`keel serve --watch`)
- VS Code extension (status bar, inline diagnostics, CodeLens, command palette) — works in VS Code, Cursor, Antigravity, Windsurf
- JSON output (`--json`), LLM output (`--llm`), human CLI output (default)
- Pre-commit git hook generation
- CI integration templates (GitHub Actions, GitLab CI)
- Type hint enforcement on all functions
- Docstring enforcement on public functions
- Placement validation (WARNING level)
- Duplicate function name detection (WARNING level)
- Circuit breaker escalation (3-attempt progressive guidance, see §4.10)
- **Linux + macOS + Windows** binaries
- Documentation at keel.engineer
- Single-repo only (no `link`)

### Out of scope for Phase 1

- `link` (cross-repo) — Phase 2
- Visual map HTML export — Phase 2
- Scoped maps for large codebases — Phase 2
- Naming convention enforcement — Phase 2
- Custom placement rules — Phase 2

### Success criteria

- `init` completes in <10s for a 50k LOC TypeScript + Python project
- `compile` completes in <200ms for single-file change
- `discover` returns in <50ms
- `explain` returns in <50ms (reads from existing resolution engine data)
- Catches 100% of arity-breaking changes and 90%+ of type-breaking changes in typed code
- Token usage for `--llm` map is <5% of 200k context for a 2500-function codebase (~4 tokens/function × 2500 = ~10k tokens). Codebases up to ~5000 functions fit within 10% of 200k context. Larger codebases require scoped maps.
- `keel init` in a project with Claude Code + Cursor installed generates both `.claude/settings.json` and `.cursor/rules/keel.mdc` without manual configuration
- A developer can `cargo install keel && cd my-project && keel init` and be fully operational in under 2 minutes
- Placement warnings trigger correctly on >80% of synthetically-misplaced functions (functions moved from their correct module to a random module in the test corpus). False positive rate 15-25% overall on correctly-placed functions (higher on utility/orchestrator modules which are inherently ambiguous, 5-10% on well-structured domain modules). Utility/orchestrator modules can be excluded from placement scoring via `[exclude]` patterns (see §13).

## 18. Implementation Milestones

Development approach: full-time with Claude Code. The 3-tier hybrid architecture (§10.1) is now defined — 4 languages, each with a different enhancer, making M1 the most complex milestone.

### M1: Core parser + resolution engine + test harness (10-14 days)

- Rust: tree-sitter parsing for all 4 Phase 1 languages (TypeScript, Python, Go, Rust)
- Rust: Tier 1 universal fast path — tree-sitter query patterns for definitions, call sites, imports
- Rust: Tier 2 per-language enhancers:
  - TypeScript: Oxc integration (`oxc_resolver` + `oxc_semantic`) — import resolution, per-file symbol tables
  - Python: ty subprocess integration (`ty --output-format json`) — type checking, cross-file resolution
  - Go: tree-sitter heuristic resolution (explicit imports + package scoping make this viable)
  - Rust: rust-analyzer lazy-loading via `ra_ap_ide` crates (optional, loaded on demand)
- Rust: keel graph layer (function nodes, call edges, hash computation) on petgraph
- Rust: CLI skeleton with `init` and `map` (clap)
- **Test harness:** Set up test repo corpus, implement graph correctness tests, establish self-correction loop
- Validate: parse excalidraw, FastAPI, cobra, and purpose-built test repo. Measure resolution precision/recall against LSP ground truth per language.
- **Critical gate:** Resolution engine must achieve >85% cross-file resolution precision per language before proceeding to M2.

### M2: Enforcement engine (3-5 days)

- `compile` with incremental tree-sitter parsing
- Type hint presence validation
- Adjacent function signature validation (type matching, arity)
- Docstring presence validation
- Placement scoring and warnings
- Duplicate function name detection
- `discover`, `where`, and `explain` commands (`explain` exposes resolution engine data already captured in M1)
- Circuit breaker state tracking (per error-code+hash failure counter, session-scoped)
- JSON output schema implementation (`--json` flag)

### M3: Multi-tool integration (4-5 days)

- LLM-optimized map output format (`--llm` flag, token-budgeted)
- `keel init` tool detection: scan for `.claude/`, `.cursor/`, `.gemini/`, `.windsurf/`, `.agent/`, `.codex/`, `GEMINI.md`, `.windsurfrules`, `.aider.conf.yml`, Letta config, `.github/copilot-instructions.md`
- Generate hook configs for 5 enforced tools: Claude Code (`.claude/settings.json`), Cursor (`.cursor/hooks.json`), Gemini CLI (`.gemini/settings.json`), Windsurf (`.windsurf/hooks.json`), Letta Code (Letta config)
- Generate instruction/policy files for all 9+ tools (CLAUDE.md, AGENTS.md, .cursor/rules/keel.mdc, .agent/rules/keel.md, GEMINI.md, .windsurfrules, .aider.conf.yml, Letta instructions, .github/copilot-instructions.md)
- Post-edit hook script (shared across all enforced tools)
- Pre-commit git hook
- CI template generation (GitHub Actions, GitLab CI)
- Test end-to-end with Claude Code AND Cursor (both enforced paths) and Codex (cooperative path)

### M4: `keel serve` + VS Code extension (3-4 days)

- MCP server over stdio (`keel serve --mcp`)
- HTTP server on localhost (`keel serve --http`)
- File watcher with auto-compile (`keel serve --watch`)
- VS Code extension: status bar, inline diagnostics, CodeLens (↑N ↓M), command palette
- Test extension in VS Code, Cursor, Antigravity (all VS Code forks)

### M5: Go + Rust language polish + Windows + distribution (5-7 days)

- Go language adapter: tree-sitter heuristic resolution tuning, test against cobra/fiber test repos
- Rust language adapter: rust-analyzer lazy-loading integration, test against ripgrep/axum test repos
- Windows build + installer (cross-compilation via cargo-dist, test in CI)
- Binary distribution for all 3 platforms (Linux, macOS, Windows)
- Install scripts (`curl | sh`, `brew`, `winget`/`scoop`, `cargo install`)
- Documentation site at keel.engineer

### M6: Dogfooding + iteration (5-7 days)

- Use keel to develop keel (dedicated dogfooding period, not "ongoing")
- Measure: token savings, error catch rate, false positive rate, placement accuracy
- Test with each LLM tool (Claude Code, Codex, Cursor, Antigravity, Gemini CLI, Letta) on real projects
- Iterate on enforcement strictness defaults based on real usage
- Validate precision per-language against targets in §10.1

**Estimated total to MVP: 10-12 weeks full-time.** The resolution engine (M1) is the long pole — 4 languages each with a different enhancer (Oxc, ty subprocess, tree-sitter heuristics, rust-analyzer) makes this the most complex milestone. Three independent research sources converge on 8-10 weeks for the engine alone; 10-12 weeks accounts for the full integration, extension, distribution, and dogfooding scope.

## 19. Automated Test Harness

Following Anthropic's C compiler approach: define comprehensive test suite, run Claude Code against it, feed failures back, iterate. Every test outputs structured JSON for Claude Code's self-correction loop.

### Test repo corpus (minimum 10)

**TypeScript:**

1. **excalidraw** (~120k LOC) — complex React app, barrel files, path aliases
2. **cal.com** (~200k LOC) — monorepo, tRPC endpoints, multiple packages
3. **typescript-eslint** (~80k LOC) — heavy AST work, well-typed, deep call chains

**Python:** 4. **FastAPI** (~30k LOC) — well-typed with Pydantic, clear API endpoints 5. **httpx** (~25k LOC) — excellent type hints, clean module structure 6. **django-ninja** (~15k LOC) — API framework, endpoint discovery testing

**Go:** 7. **cobra** (~15k LOC) — clean Go module structure 8. **fiber** (~30k LOC) — HTTP framework, route definitions, middleware

**Rust:** 9. **ripgrep** (~25k LOC) — well-structured crate workspace 10. **axum** (~20k LOC) — web framework, route definitions

**Multi-language:** 11. Purpose-built test repo with known cross-file references, known breaking changes, and expected keel outputs (ground truth)

### Test categories

**Category 1: Graph correctness** (`keel init` + `keel map`)

- All public functions are in the graph
- All cross-file call edges are present
- External endpoints are detected
- No phantom edges (edges to functions that don't exist)
- Metric: precision and recall of call edges vs. LSP ground truth

**Category 2: Enforcement correctness** (`keel compile`)

- Introduce known breaking changes (automated via script):
    - Rename parameter → verify caller mismatch detected
    - Change return type → verify type contract violation detected
    - Remove function → verify all broken callers reported
    - Add function without type hints → verify enforcement triggers
    - Add function without docstring → verify enforcement triggers
    - Change arity → verify arity mismatch detected
    - Add function in wrong module → verify placement warning triggers
- Circuit breaker escalation tests:
    - Trigger 3 consecutive failures on same error-code + hash → verify auto-downgrade to WARNING
    - Fix error on attempt 2 → verify counter resets (attempt 3 escalation does not trigger)
    - Trigger failures during batch mode → verify counters are paused, no escalation
- fix_hint tests:
    - Every ERROR-level violation includes a non-empty `fix_hint` field
    - fix_hint text references specific callers/locations (not generic advice)
- Metric: true positive rate, false positive rate, false negative rate

**Category 3: Performance benchmarks**

- `init` time per repo (target: <10s for 50k LOC)
- `compile` time for single-file change (target: <200ms)
- `discover` response time (target: <50ms)
- `explain` response time (target: <50ms)
- Clean compile (zero errors, zero warnings) produces empty stdout and exit 0
- `map --llm` token count vs. codebase size

**Category 4: LLM integration test** (end-to-end)

- Give Claude Code a task on a test repo with keel installed — verify enforced backpressure works
- Give Cursor a task on the same repo — verify enforced path works (including v2.0 workaround for blocked `agent_message`)
- Give Gemini CLI a task — verify AfterAgent self-correction loop works
- Give Codex a task on the same repo — verify AGENTS.md instructions are followed
- Verify: LLM uses the map context, calls `discover`, responds to `compile` errors
- Compare: token usage with keel vs. without keel on the same task
- Verify: `keel init` generates correct configs when multiple tools are present
- Verify: `explain` command returns valid resolution chain with correct fields (§12 schema)
- Verify: LLM follows circuit breaker escalation instructions (runs `discover --depth 2` at attempt 2, runs `explain` at attempt 3)

### Harness implementation

```bash
#!/bin/bash
# test_harness.sh — run by Claude Code in self-correction loop
set -e

./scripts/setup_test_repos.sh                # Clone/cache test repos
./scripts/test_graph_correctness.sh          # → results/graph_correctness.json
./scripts/test_enforcement.sh                # → results/enforcement.json
./scripts/test_performance.sh                # → results/performance.json
./scripts/aggregate_results.sh               # → results/summary.md

# Exit with failure if below thresholds:
# Graph correctness: >90% recall, >95% precision
# Enforcement: >95% true positive, <5% false positive
# Performance: all targets met
```

## 20. Developer Experience

keel's adoption depends on a frictionless first experience. If `keel init` takes more than 2 minutes or requires manual configuration, developers won't use it. Every interaction must feel fast, obvious, and zero-config.

### 20.1 First Run: Zero to Enforced in 90 Seconds

```bash
# Install (one command, any platform)
curl -fsSL https://keel.engineer/install.sh | sh   # macOS / Linux
# or: brew install keel
# or: winget install keel                            # Windows
# or: cargo install keel

# Initialize (auto-detects everything)
cd my-project
keel init
```

**Config merge behavior:** `keel init` merges into existing tool configurations — it does not overwrite them. If `.claude/settings.json` already has hooks, keel adds its hooks alongside existing ones. If `CLAUDE.md` already exists, keel appends its section between `<!-- keel:start -->` / `<!-- keel:end -->` markers. See §4.1 for full merge strategy.

**`keel init` auto-detects:**

- Languages present (TypeScript, Python, Go, Rust) from file extensions and config files
- LLM tools installed (scans for `.claude/`, `.cursor/`, `.gemini/`, `.windsurf/`, `.codex/`, `.agent/`, `GEMINI.md`, `.windsurfrules`, `.aider.conf.yml`, Letta config, `.github/copilot-instructions.md`)
- **Existing project configuration** to derive enforcement rules automatically: `tsconfig.json` strict mode → type enforcement enabled, `pyproject.toml` [tool.mypy] → type hint expectations, `.eslintrc` → naming conventions. `.keel/config.toml` provides overrides, not primary configuration.
- Package manager (npm, pip, cargo, go)
- Git configuration (installs pre-commit hook)
- CI provider (`.github/workflows/` → GitHub Actions template)

**`keel init` generates:**

- `.keel/graph.db` (gitignored — local graph database)
- `.keel/manifest.json` (committed — lightweight module summary)
- `.keel/config.toml` (committed — team configuration)
- `.keel/hooks/post-edit.sh` (committed — shared hook script for all enforced tools)
- `.keel/telemetry.db` (gitignored — local telemetry, see §22)
- Hook configs for all detected enforced tools (`.claude/settings.json`, `.cursor/hooks.json`, `.gemini/settings.json`, `.windsurf/hooks.json`, Letta config)
- Instruction files for all detected tools (see §9)
- `.gitignore` entries for `.keel/graph.db` and `.keel/telemetry.db`
- Pre-commit hook in `.git/hooks/pre-commit`

**`keel init` output:**

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

### 20.2 CLI Design Principles

**Fast:** Every command returns in under 200ms for single-file operations. The developer never waits.

**Quiet by default:** Success produces minimal output. Errors are loud and specific. No progress bars for operations under 1 second.

**Predictable exit codes:**

- `0` — success, no violations
- `1` — violations found (errors or warnings, depending on `--strict`)
- `2` — keel internal error (graph corrupt, parse failure)

**Colored output:** Errors in red, warnings in yellow, info in dim. Respects `NO_COLOR` and `TERM=dumb`.

**JSON everything:** Every command supports `--json` for programmatic consumption. The human-readable output is just a pretty-printed view of the same data.

### 20.3 Updating the Map

The graph stays current automatically via `compile` (runs on every edit via hooks or manually). Full re-map is only needed after major events:

```bash
keel map              # Full re-map (after branch switch, rebase, etc.)
keel map --llm        # Regenerate LLM-optimized output only
keel map --json       # Full JSON export
```

### 20.4 Exploring the Graph

```bash
keel discover xK2p9Lm   # Show callers, callees, module context for hash
keel where xK2p9Lm      # Resolve hash → src/auth/login.ts:42
keel map --llm --scope=auth,payments  # Scoped map for specific modules
```

### 20.5 Configuration

Sane defaults that work for 90% of projects. Override via `.keel/config.toml` when needed. The config is intentionally small — if you need to configure more than 5 things, something is wrong.

### 20.6 Uninstall / Disable

```bash
keel deinit           # Removes all generated files, hooks, and tool configs
                      # Leaves .keel/config.toml for re-init
```

No lock-in. `keel deinit` cleanly removes everything keel added. The codebase is unchanged.

### 20.7 Batch Mode for Scaffolding Sessions

When an LLM is scaffolding multiple files at once (e.g., "create a new auth module with login, logout, and token refresh"), per-edit enforcement creates noise — type hints and docstrings on half-written files are meaningless until the structure is complete. Batch mode defers non-structural validation until the scaffolding session ends.

**CLI usage:**

```bash
keel compile --batch-start         # Begin batch mode
# ... LLM creates multiple files ...
keel compile src/auth/login.ts     # Structural errors (broken callers) still fire immediately
keel compile src/auth/logout.ts    # Type hint / docstring errors deferred
# ... more scaffolding ...
keel compile --batch-end           # All deferred validations fire now
```

**What fires immediately during batch mode:** Structural errors — broken callers (E001), removed functions with dependents (E004), arity mismatches (E005). These indicate real breakage that compounds if left unfixed.

**What is deferred to batch-end:** Type hint enforcement (E002), docstring enforcement (E003), placement warnings (W001), duplicate name warnings (W002). These are quality checks that only make sense on completed code.

**Auto-expiry:** Batch mode auto-expires after 60 seconds of inactivity (no `compile` calls) to prevent accidentally leaving it on. The expiry triggers all deferred validations.

**Circuit breaker interaction:** Circuit breaker counters (see §4.10) are paused during batch mode. Deferred validation failures at `--batch-end` do not count as consecutive failures for escalation purposes.

## 21. Competitive Landscape (Updated Feb 2026)

### Category Positioning: Structural Guardrails for Code Agents

The landscape divides into four categories. keel occupies the fourth — a category that didn't exist before:

**Category 1: Context Providers** — help the LLM understand code

|Tool|What it does|Relationship to keel|
|---|---|---|
|**Augment Code Context Engine MCP** (launched Feb 6, 2026)|70-80% agent improvement. 400k+ file context. Works with Cursor, Claude Code, Zed.|**Complementary.** Augment gives the LLM memory. keel gives it discipline. Use both.|
|**Aider** repo map|tree-sitter + PageRank map (4-6% context usage)|**Complementary.** Good map, no enforcement. LLM can ignore it.|
|**Cursor built-in semantic indexing**|tree-sitter AST chunking + embeddings. Native to Cursor.|**Complementary.** Cursor-only. Context, not enforcement.|
|**Letta Code**|Memory-first coding agent with ~12 hook events and exit-code-2 blocking — same enforcement semantics as Claude Code|**Complementary + enforcement peer.** Letta remembers. keel enforces. Letta's hook system makes it a Tier 1 enforced tool for keel.|

**Category 2: Review-Time Checkers** — catch problems after code is written

|Tool|What it does|Relationship to keel|
|---|---|---|
|**Greptile**|Code graph for PR review and AI-assisted review|**Different timing.** Greptile catches at review. keel catches at generation.|
|**Qodo (CodiumAI)**|"Agentic code integrity platform" — compliance checks, breaking change detection at PR level|**Different timing.** Post-hoc PR-level review, not during-generation enforcement. "The inspector, not the guardrail."|
|**CodeQL / Joern**|Static analysis, vulnerability detection|**Different focus.** Security-focused. No LLM integration.|
|**Sourcegraph Amp**|Replaced Cody Free/Pro. Agentic coding with team collaboration.|**Overlapping but different.** Amp is an agent. keel constrains agents.|

**Category 3: Agent Guardrails / Infrastructure** — constrain agent behavior generically

|Tool|What it does|Relationship to keel|
|---|---|---|
|**Google Antigravity** ($2.4B Windsurf acquisition, Nov 2025)|Agent-first IDE with skills/rules system|**Different layer.** Antigravity is a tool. keel is infrastructure that tools consume.|
|**MCP code-graph servers** (8-10 as of Feb 2026: code-graph-rag-mcp, RepoMapper MCP, mcp-server-tree-sitter, etc.)|Code graph exposed via MCP|**Context only.** No backpressure. No type enforcement. No placement.|
|**Sourcegraph SCIP**|Cross-repo code intelligence indexing|**Resolution data source.** SCIP is a potential input to keel's resolution engine, not a competitor.|

**Category 4: Structural Enforcement During Generation** — keel's category

|Tool|What it does|
|---|---|
|**keel**|Generation-time architectural enforcement layer. Backpressure (verify before, validate after). Contract enforcement (type, signature, adjacency). Placement intelligence. Works across 5 enforced tools + 4 cooperative tools simultaneously.|

**Nobody else fully occupies this category**, but several tools are adjacent:

|Tool|What it does|Why it's not keel|Threat level|
|---|---|---|---|
|**Codacy Guardrails**|Real-time SAST/SCA interception during AI code generation — genuine during-generation enforcement|Enforces security/quality patterns, not architectural contracts. No concept of module boundaries, interface signatures, or adjacency.|**Nearest analog.** Different scope (security vs. architecture) but same timing.|
|**AWS Kiro**|Spec-driven `.kiro/steering/` markdown with architecture violation warnings|Enforcement is LLM-mediated (the model reads natural language rules). No external formal checker — if the LLM misinterprets, violations pass.|**Watch closely.** If AWS invests in making steering enforcement deterministic, the threat escalates significantly.|
|**ArchCodex** (open source)|Four-layer architecture (Boundaries, Constraints, Examples, Validation) mapping precisely to keel's value prop|Single developer, open source. Validates the market but represents a free alternative.|**Validates market.** Proves demand exists for architectural enforcement during generation.|
|**GitHub Copilot** (MCP policies)|Partial hooks via MCP-based policies with JSON `permissionDecision: "deny"`. Governance layer.|Not scriptable exit-code-2 hooks. JSON-based policy system, not general-purpose enforcement. But massive market share.|**Distribution threat.** If Copilot adds richer enforcement hooks, keel must already be integrated.| Context providers give the LLM information — they don't prevent it from breaking things. Review-time checkers catch problems — but the damage is already done and the LLM has moved on. Generic agent guardrails constrain behavior broadly — they don't understand code structure. keel is the only tool that enforces architectural contracts at the moment the LLM generates code.

### Why keel wins

1. **Enforcement, not just context.** Anyone can build a code graph. Nobody else forces the LLM to use it correctly. The `compile` → exit code 2 → LLM must fix loop is unique to keel.

2. **Universality.** keel works across Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code (enforced) and GitHub Copilot, Codex, Antigravity, Aider (cooperative/governance). No single-vendor tool can match this cross-tool enforcement.

3. **Complementary to everything.** keel doesn't compete with Augment (context), Greptile (review), or Antigravity (IDE). It layers on top. "Augment gives your agent perfect recall. keel gives it perfect discipline. Use both."

4. **Enforcement is orthogonal to model quality.** Better models don't eliminate the need for `tsc`, the borrow checker, or pre-commit hooks. keel becomes *more* valuable as developers trust better models more — because the errors better models make are harder to spot.

### Competitive Risks (Honest Assessment)

|Risk|Severity|Mitigation|
|---|---|---|
|Augment Context Engine makes navigation irrelevant|High|Reposition: keel's value is enforcement, not context. Augment + keel is the recommended stack.|
|Cursor/Antigravity build native enforcement|High|Ship fast. Cross-tool universality is the moat. Native enforcement in one tool doesn't help the 8+ other tools.|
|MCP code-graph servers become good enough|Medium|MCP servers provide graphs. keel provides enforcement on top of graphs. Different layer.|
|Letta's persistent memory reduces architectural errors|Medium|Memory reduces errors; it doesn't eliminate them. keel catches what memory misses.|

## 22. Telemetry and Self-Measurement

keel must measure its own value. Without data, the "20% token savings" and "catches 100% of arity-breaking changes" claims are marketing, not facts. Telemetry makes the value visible to the developer and to keel's own iteration loop.

### 22.1 What keel tracks (local-first, opt-in sharing)

All telemetry is stored in `.keel/telemetry.db` (SQLite, gitignored). **Nothing leaves the developer's machine by default.** Sending telemetry is strictly opt-in. Developers who want to share anonymized usage data (to help improve keel) can enable it via `keel config set telemetry.share true` — but the default is off. Feedback, bug reports, and feature requests go through GitHub Issues.

**Per-session metrics:**

- `errors_caught` — count of ERROR-level violations caught by `compile` (broken callers, missing type hints, missing docstrings)
- `warnings_issued` — count of WARNING-level violations (placement, duplicates)
- `errors_resolved` — count of errors the LLM fixed after being blocked (backpressure working)
- `false_positives_dismissed` — count of warnings/errors the developer or LLM overrode (indicates false positive rate)
- `compile_invocations` — how many times `compile` ran
- `discover_invocations` — how many times `discover` was called (LLM is checking adjacency)
- `explain_invocations` — how many times `explain` was called (LLM is diagnosing resolution reasoning)
- `circuit_breaker_escalations` — count of times same error hit attempt 2 (LLM escalated to wider discover)
- `circuit_breaker_downgrades` — count of times same error hit attempt 3 (auto-downgraded to WARNING)
- `map_token_count` — tokens used by the `--llm` map for this session

**Per-week aggregates:**

- `error_catch_rate` — errors caught / total edits (higher = more value)
- `false_positive_rate` — dismissed warnings / total warnings (lower = better)
- `backpressure_effectiveness` — errors resolved by LLM / errors caught (higher = LLM responding to enforcement)
- `map_utilization` — discover calls / total editing sessions (higher = LLM using the map)

### 22.2 Telemetry dashboard

`keel stats` — CLI command that shows a summary of telemetry data.

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

### 22.3 Why this matters

- **For adoption:** Developer can see "keel caught 47 errors this week that would have shipped." Tangible value.
- **For iteration:** False positive rate tells keel developers where thresholds need tuning.
- **For marketing:** Real numbers from real codebases, not theoretical claims.

## 23. Upgrade Path and Schema Evolution

### 23.1 The problem

keel's graph schema (§11) will evolve. New node types, new edge types, new metadata fields. When a developer upgrades keel, the `.keel/graph.db` and `.keel/manifest.json` formats may change. This must be handled gracefully.

### 23.2 Schema versioning

- `.keel/graph.db` stores a `schema_version` integer in a `keel_meta` table
- `.keel/manifest.json` includes a `"schema_version"` field
- `.keel/config.toml` includes a `version` field (already present)

### 23.3 Upgrade behavior

When keel detects a schema version mismatch (binary expects v2, database is v1):

1. **Automatic migration for minor changes:** Adding new columns, new metadata fields, new optional properties. Run `ALTER TABLE` / add fields. No data loss. Transparent to user.
2. **Rebuild for major changes:** If the graph structure changes fundamentally (new node types, changed edge semantics), keel prints: `keel: Graph schema v1 → v2 migration requires rebuild. Running 'keel map'...` and automatically rebuilds the graph. The `.keel/graph.db` is regenerable — it's just a cache derived from source code.
3. **Manifest compatibility:** `.keel/manifest.json` is committed to git. If the format changes, keel writes the new format on rebuild. Team members on older keel versions see a warning: `keel: manifest.json was generated by keel v2.x. Some features may not work. Run 'keel map' to update.`

### 23.4 Backward compatibility guarantee

- keel N can always read graphs generated by keel N-1 (one version back)
- keel N can always rebuild the graph from source code (infinite backward compatibility via rebuild)
- `.keel/config.toml` is always forward-compatible (unknown keys are ignored, not errors)

## 24. Multi-Language Repository Handling

### 24.1 The problem

Many real-world repos contain multiple languages. A FastAPI backend (Python) + Next.js frontend (TypeScript) in a monorepo. Go microservices alongside Python data pipelines. keel must handle this.

### 24.2 Per-language parsing

keel already supports multiple languages in Phase 1. Each file is parsed by the tree-sitter grammar matching its extension. The graph contains nodes from all languages simultaneously. This is straightforward.

### 24.3 Cross-language call detection (Phase 1 scope)

**What keel detects in Phase 1:**

- **Shared external endpoints:** If Python serves `POST /api/users` and TypeScript calls `fetch('/api/users', { method: 'POST' })`, keel detects this as a cross-language edge via endpoint matching (same mechanism as cross-repo linking in Phase 2, but within a single repo).
- **Shared types via JSON schema / OpenAPI:** If a repo has an OpenAPI spec, keel can use it to validate that the TypeScript client's type expectations match the Python server's response types. Phase 1: detection only (WARNING). Phase 2: enforcement.

**What keel does NOT detect in Phase 1:**

- **FFI calls:** Python calling Rust via PyO3, Go calling C via cgo. These require language-specific FFI detection and are deferred.
- **Shared data structures without explicit contracts:** If Python writes a dict to Redis and TypeScript reads it, keel cannot detect this without explicit schema annotation. Deferred.

### 24.4 Module boundary independence

Each language's modules are independent in the graph. `src/api/` (Python) and `src/frontend/` (TypeScript) are separate module subtrees. Placement scoring operates within a language — keel won't suggest moving a Python function to a TypeScript module.

### 24.5 Enforcement per language

Type hint enforcement respects language norms:

- **Python:** Requires explicit type annotations on all parameters and return types
- **TypeScript:** Already typed. keel validates signature contracts.
- **Go:** Already typed. keel validates signature contracts.
- **Rust:** Already typed. keel validates signature contracts.
- **JavaScript:** Requires JSDoc `@param` and `@returns`. LLM instructed to prefer TypeScript for new files.

Docstring enforcement format varies per language: Python docstrings, JSDoc, Go doc comments, Rust `///` doc comments. keel detects the language-appropriate format.

### 24.6 Monorepo Handling

Many production codebases use workspace-based monorepo tooling (Nx, Turborepo, Lerna, Cargo workspaces, Go workspaces). keel builds a single graph with workspace-aware package boundaries.

**Detection:** `keel init` auto-detects monorepo configs: `nx.json`, `turbo.json`, `lerna.json`, `pnpm-workspace.yaml`, Cargo `[workspace]` in root `Cargo.toml`, `go.work`. When detected, keel treats each workspace package as a distinct module subtree with its own enforcement scope.

**Cross-package enforcement:** Internal import edges between packages are tracked in the graph like any other call edge. If package `@app/auth` exports `validateToken` and package `@app/payments` calls it, that's a cross-package edge. Changing `validateToken`'s signature triggers `ERROR` for callers in `@app/payments` — same enforcement as within a single package.

**Per-package enforcement overrides:** The `[enforcement.overrides]` config (see §13) supports per-package glob patterns. Teams can enforce stricter rules on core packages while keeping utility packages at WARNING level:

```toml
[enforcement.overrides]
"packages/core/**" = { type_hints = "error", docstrings = "error" }
"packages/scripts/**" = { type_hints = "warning", docstrings = "off" }
```

**Phase 2 consideration:** For monorepos exceeding ~500k LOC, per-package graph partitioning enables scoped maps and faster incremental compilation by only loading the subgraph relevant to the current edit context.