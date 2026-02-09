# Test Harness Strategy — Oracles, Corpus, and Harness Scripts

```yaml
tags: [keel, spec, testing, harness, oracles]
owner: All agents (Phase 0 setup by single agent)
dependencies: [000-graph-schema, 001-treesitter-foundation, 006-enforcement-engine]
prd_sections: [19]
priority: P0 — Phase 0 deliverable, must exist before any product code
```

## Summary

This spec defines keel's automated test harness: the 4 verification oracles, the test repo corpus, the test categories, the harness scripts, and the pre-written test pattern. Following Anthropic's C compiler approach: define comprehensive test suite first, run agents against it, feed failures back, iterate. Every test outputs structured results for the agent self-correction loop.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| §19 | Full test harness specification — corpus, categories, harness scripts, thresholds |
| §17 | Performance targets used as benchmark thresholds |
| §12 | JSON schemas used for schema validation oracle |

---

## The 4 Verification Oracles

These are the "verifiers" from [[design-principles#Principle 1 The Verifier Is King|Principle 1]]. Each oracle provides a different dimension of correctness.

### Oracle 1: LSP Ground Truth (Graph Correctness)

**What it verifies:** keel's graph is correct — functions, edges, and endpoints match reality.

**How it works:**
1. For each test corpus repo, run the language's native LSP to extract all cross-file call edges (ground truth)
2. Run `keel init` + `keel map --json` on the same repo
3. Compare:
   - **Precision** = edges keel found that LSP confirms / total edges keel found
   - **Recall** = edges LSP found that keel also found / total edges LSP found
4. Per-language thresholds:

| Language | Precision Target | Recall Target |
|----------|-----------------|---------------|
| TypeScript | >85% | >80% |
| Python | >82% | >78% |
| Go | >85% | >82% |
| Rust | >75% | >70% |

**LSP tools for ground truth:**
- TypeScript: `tsserver` (canonical)
- Python: `pyright` or `pylsp`
- Go: `gopls`
- Rust: `rust-analyzer`

**Implementation:** Script that runs LSP `textDocument/references` and `textDocument/definition` on all functions, builds edge set, compares against keel's edge set.

### Oracle 2: Mutation Testing (Enforcement Correctness)

**What it verifies:** keel catches known-breaking changes.

**How it works:**
1. Start with a clean test corpus repo where `keel compile` passes
2. Automatically introduce known-breaking mutations:
   - Rename function parameter -> expect E001 (broken callers)
   - Change return type -> expect E001 (type contract violation)
   - Remove function with callers -> expect E004
   - Add function without type hints -> expect E002
   - Add public function without docstring -> expect E003
   - Change function arity -> expect E005
   - Add function in wrong module -> expect W001
3. Run `keel compile --json` after each mutation
4. Verify correct error code, severity, and that `fix_hint` is non-empty and references specific locations

**Thresholds:**
- True positive rate: >95% (mutations caught / total mutations)
- False positive rate: <5% (false alarms on clean code / total compile invocations)
- False negative rate: <5% (mutations missed / total mutations)

**Additional mutation tests:**
- Circuit breaker: trigger 3 consecutive failures on same error-code + hash -> verify auto-downgrade
- Fix on attempt 2 -> verify counter resets
- Batch mode failures -> verify counters paused
- fix_hint: every ERROR has non-empty `fix_hint` referencing specific callers/locations

### Oracle 3: Performance Benchmarks (Hard Numeric Targets)

**What it verifies:** keel meets its performance contracts.

**Benchmarks (pass/fail, not aspirational):**

| Operation | Target | Test Repo |
|-----------|--------|-----------|
| `keel init` | <10s | 50k LOC TypeScript + Python |
| `keel map` | <5s | 100k LOC (excalidraw) |
| `keel compile` (single file) | <200ms | Any test repo, single file edit |
| `keel discover` | <50ms | Any test repo, any function hash |
| `keel explain` | <50ms | Any test repo, any error code + hash |
| `keel map --llm` | <5% of 200k context (~10k tokens) | 2500-function codebase |
| Clean compile | empty stdout + exit 0 | Any clean repo |

**Implementation:** Rust benchmark tests using `criterion` crate. Each benchmark runs 10 iterations, reports p50 and p99. p99 must be under target.

### Oracle 4: JSON Schema Validation (Output Contracts)

**What it verifies:** all `--json` output conforms to the defined schemas.

**Schemas validated:**
- `compile` error output (from [[keel-speckit/008-output-formats/spec|Spec 008]])
- `discover` output
- `map --json` output
- `explain` output

**Implementation:** JSON Schema files in `tests/schemas/`. Every `--json` test validates output against schema before checking content.

---

## Test Repo Corpus (Minimum 10)

### TypeScript

| # | Repo | LOC | Why |
|---|------|-----|-----|
| 1 | excalidraw | ~120k | Complex React app, barrel files, path aliases |
| 2 | cal.com | ~200k | Monorepo, tRPC endpoints, multiple packages |
| 3 | typescript-eslint | ~80k | Heavy AST work, well-typed, deep call chains |

### Python

| # | Repo | LOC | Why |
|---|------|-----|-----|
| 4 | FastAPI | ~30k | Well-typed with Pydantic, clear API endpoints |
| 5 | httpx | ~25k | Excellent type hints, clean module structure |
| 6 | django-ninja | ~15k | API framework, endpoint discovery testing |

### Go

| # | Repo | LOC | Why |
|---|------|-----|-----|
| 7 | cobra | ~15k | Clean Go module structure |
| 8 | fiber | ~30k | HTTP framework, route definitions, middleware |

### Rust

| # | Repo | LOC | Why |
|---|------|-----|-----|
| 9 | ripgrep | ~25k | Well-structured crate workspace |
| 10 | axum | ~20k | Web framework, route definitions |

### Multi-Language

| # | Repo | LOC | Why |
|---|------|-----|-----|
| 11 | Purpose-built test repo | ~5k | Known cross-file references, known breaking changes, expected keel outputs (ground truth) |

**Corpus management:**
- Repos cloned and cached locally in `test-corpus/`
- Pinned to specific commits (reproducible)
- `scripts/setup_test_repos.sh` handles clone/cache/pin
- Purpose-built repo (#11) is maintained in the keel workspace — the only repo where mutations are scripted

---

## Test Categories

### Category 1: Graph Correctness (`keel init` + `keel map`)

**Validates:**
- All public functions are in the graph
- All cross-file call edges are present
- External endpoints are detected
- No phantom edges (edges to functions that don't exist)
- Module profiles populated with non-empty keywords

**Metric:** precision and recall of call edges vs. LSP ground truth

**Test files:**
```
tests/graph_correctness/
├── test_typescript_graph.rs         # excalidraw, cal.com, typescript-eslint
├── test_python_graph.rs             # FastAPI, httpx, django-ninja
├── test_go_graph.rs                 # cobra, fiber
├── test_rust_graph.rs               # ripgrep, axum
├── test_multi_language_graph.rs     # purpose-built test repo
├── test_endpoint_detection.rs       # HTTP routes, gRPC, GraphQL across all repos
└── test_module_profiles.rs          # module responsibility profiles
```

### Category 2: Enforcement Correctness (`keel compile`)

**Validates:**
- Known breaking changes are caught (mutation testing)
- Type hint enforcement works per language
- Docstring enforcement works
- Placement warnings trigger on misplaced functions
- Circuit breaker escalation sequence works
- fix_hint is present and references specific locations
- Batch mode defers correctly and fires on batch-end
- Suppress mechanism works (inline, config, CLI)

**Metric:** true positive rate, false positive rate, false negative rate

**Test files:**
```
tests/enforcement/
├── test_broken_callers.rs           # E001: signature changes, type mismatches
├── test_removed_functions.rs        # E004: deleted functions with callers
├── test_arity_mismatch.rs           # E005: parameter count changes
├── test_type_hints.rs               # E002: missing type annotations per language
├── test_docstrings.rs               # E003: missing docstrings on public functions
├── test_placement.rs                # W001: misplaced functions, suggested modules
├── test_duplicate_names.rs          # W002: duplicate function detection
├── test_circuit_breaker.rs          # 3-attempt escalation, counter reset, batch interaction
├── test_fix_hints.rs                # Every ERROR has non-empty fix_hint
├── test_batch_mode.rs               # Deferred validation, auto-expiry, structural fires immediately
├── test_suppress.rs                 # Inline, config, CLI suppress. S001 downgrade.
├── test_progressive_adoption.rs     # New vs existing code enforcement levels
└── test_clean_compile.rs            # Zero violations = empty stdout + exit 0
```

### Category 3: Performance Benchmarks

**Validates:** all performance targets met (see Oracle 3 table above)

**Test files:**
```
tests/benchmarks/
├── bench_init.rs                    # <10s for 50k LOC
├── bench_map.rs                     # <5s for 100k LOC
├── bench_compile.rs                 # <200ms for single-file change
├── bench_discover.rs                # <50ms
├── bench_explain.rs                 # <50ms
├── bench_llm_tokens.rs              # <5% of 200k context for 2500 functions
└── bench_clean_compile.rs           # Exit 0, empty stdout timing
```

### Category 4: LLM Integration (End-to-End)

**Validates:** keel works with real LLM tools.

**Tests (manual + semi-automated):**
- Give Claude Code a task on test repo with keel installed -> verify enforced backpressure works
- Give Cursor a task -> verify enforced path (including v2.0 `agent_message` workaround)
- Give Gemini CLI a task -> verify AfterAgent self-correction loop
- Give Codex a task -> verify AGENTS.md instructions followed
- Verify: LLM uses map context, calls `discover`, responds to `compile` errors
- Compare: token usage with keel vs. without keel on same task
- Verify: `keel init` generates correct configs when multiple tools are present
- Verify: `explain` returns valid resolution chain
- Verify: LLM follows circuit breaker escalation instructions

**Test files:**
```
tests/integration/
├── test_claude_code_e2e.rs          # Claude Code hooks + enforcement
├── test_cursor_e2e.rs               # Cursor hooks + v2.0 workaround
├── test_codex_e2e.rs                # Codex cooperative path
├── test_init_multi_tool.rs          # Init with multiple tools detected
├── test_mcp_server.rs               # MCP tool calls
└── test_http_server.rs              # HTTP endpoint responses
```

---

## Harness Scripts

### `scripts/setup_test_repos.sh`

Clones and caches test corpus repos, pinned to specific commits.

### `scripts/test_graph_correctness.sh`

Runs Oracle 1 (LSP ground truth) on all test corpus repos. Outputs `results/graph_correctness.json`.

### `scripts/test_enforcement.sh`

Runs Oracle 2 (mutation testing) on purpose-built test repo. Outputs `results/enforcement.json`.

### `scripts/test_performance.sh`

Runs Oracle 3 (performance benchmarks) on test corpus repos. Outputs `results/performance.json`.

### `scripts/test_schema_validation.sh`

Runs Oracle 4 (JSON schema validation) on all `--json` outputs. Outputs `results/schema_validation.json`.

### `scripts/aggregate_results.sh`

Combines all oracle results into `results/summary.md`. Exits with failure if below thresholds:
- Graph correctness: >85% precision, >80% recall (per language)
- Enforcement: >95% true positive, <5% false positive
- Performance: all targets met
- Schema: 100% validation pass

### `scripts/test-fast.sh`

Quick test suite (~60 seconds). Runs:
- `cargo check` (compilation)
- `cargo test` with `--lib` (unit tests only)
- Schema validation on fixture outputs
- Random sample of 20 enforcement tests

### `scripts/test-full.sh`

Full test suite (~5-10 minutes). Runs all 4 oracles on all test corpus repos.

---

## Pre-Written Test Pattern

Phase 0 creates ALL test files with `#[ignore]` annotations. Agents un-ignore as they implement features. Progress = passing tests / total tests.

```rust
// tests/enforcement/test_broken_callers.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Not yet implemented — Phase 1 Agent B"]
    fn test_parameter_type_change_detects_broken_callers() {
        // GIVEN a function 'login(email: str, pw: str)' with 3 callers
        // WHEN the parameter type is changed to 'login(email: str, pw: Password)'
        // AND keel compile is run
        // THEN E001 is returned with 3 affected callers
        todo!("Agent B: implement this test")
    }

    #[test]
    #[ignore = "Not yet implemented — Phase 1 Agent B"]
    fn test_return_type_change_detects_broken_callers() {
        // GIVEN a function 'getUser() -> User' with 2 callers
        // WHEN the return type is changed to 'getUser() -> Option<User>'
        // AND keel compile is run
        // THEN E001 is returned with 2 affected callers
        todo!("Agent B: implement this test")
    }
}
```

Agents un-ignore by removing the `#[ignore = "..."]` attribute when they implement the feature.

---

## Test Count Summary

| Category | Test Files | Estimated Tests |
|----------|-----------|----------------|
| Graph correctness | 7 | ~50 |
| Enforcement | 13 | ~102 |
| Performance benchmarks | 7 | ~25 |
| LLM integration | 6 | ~30 |
| Graph schema (from Spec 000) | 8 | ~70 |
| Tree-sitter parsing (from Spec 001) | 8 | ~60 |
| Per-language resolution (from Specs 002-005) | ~16 | ~130 |
| CLI commands (from Spec 007) | ~8 | ~50 |
| Output formats (from Spec 008) | ~6 | ~40 |
| Tool integration (from Spec 009) | ~8 | ~45 |
| MCP/HTTP server (from Spec 010) | ~5 | ~30 |
| VS Code extension (from Spec 011) | ~3 | ~20 |
| Distribution (from Spec 012) | ~3 | ~15 |
| **Total** | **~98** | **~667** |

---

## Error Fingerprinting for Agent Loop

When agents run in ralph loops, the same test failure may repeat. Error fingerprinting prevents infinite retry loops.

**Fingerprint computation:**
```
fingerprint = hash(test_name + error_message_pattern + file_path)
```

**Escalation thresholds** (from [[design-principles#Principle 6 Error Fingerprinting and Escalation|Principle 6]]):

| Consecutive Failures | Action |
|---------------------|--------|
| 5 | Inject hint into agent prompt |
| 8 | Force-skip task, return to unclaimed pool |
| 15 | 30-minute cooldown, human review flag |

**Reset:** Counter resets when the error changes (different test fails or different error message).

---

## Acceptance Criteria (for the test harness itself)

**GIVEN** `scripts/setup_test_repos.sh` is run
**WHEN** all 11 repos are downloaded
**THEN** each repo is at a pinned commit and cached locally

**GIVEN** `scripts/test-fast.sh` is run on a fresh workspace
**WHEN** no product code exists yet
**THEN** all tests are skipped (ignored) and the script exits 0

**GIVEN** `scripts/test-full.sh` is run after full implementation
**WHEN** all 4 oracles complete
**THEN** `results/summary.md` reports per-oracle pass/fail and overall status

**GIVEN** a mutation is introduced to the purpose-built test repo
**WHEN** `scripts/test_enforcement.sh` runs
**THEN** the mutation is caught with correct error code and fix_hint

**GIVEN** `keel compile --json` output for any test
**WHEN** validated against JSON schema
**THEN** schema validation passes

---

## Known Risks

| Risk | Severity | Mitigation |
|------|----------|-----------|
| Test corpus repos change upstream (breaking pinned commits) | Medium | Pin to specific commit SHA, not branch. Archive locally. |
| LSP ground truth is imperfect (LSPs have bugs too) | Medium | Use LSP as approximate ground truth. Allow 5% margin. Manual review of discrepancies. |
| Performance benchmarks vary by hardware | Medium | Run on standardized CI hardware. Report as pass/fail, not absolute numbers. |
| Purpose-built test repo becomes stale as PRD evolves | Low | Maintained alongside specs. Updated in Phase 0. |
| ~667 tests is a lot to write in Phase 0 | Medium | Tests are stubs with `#[ignore]`. Only the test signatures and comments are written, not implementations. |

---

## Related Documents

- [[design-principles|Design Principles]] — Principle 1 (Verifier Is King) defines the oracles
- [[constitution|Constitution]] — Article 7 (Testing Standards) defines thresholds
- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — the primary system under test
- [[agent-swarm|Agent Swarm Playbook]] — how agents interact with the harness
