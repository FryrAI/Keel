# Keel Constitution — Non-Negotiable Articles

```yaml
tags: [keel, constitution, non-negotiable, governance]
status: governing
authority: "This document governs. Specs implement articles. Code implements specs."
```

> **What this document is**: The non-negotiable constraints for the keel implementation. Every article is extracted from [[PRD_1|PRD v2.1]] and represents a decision that is **already made**. Agents do not revisit these decisions — they implement them.
>
> **Spec authority chain**: Constitution -> Specs -> Code. If a spec contradicts this constitution, the constitution wins. If code contradicts a spec, the spec wins.

---

## Article 1: Technology Stack

keel is a **pure Rust** CLI tool. The following crate selections are final:

| Component | Crate | Version Constraint | Purpose |
|-----------|-------|-------------------|---------|
| Parsing | `tree-sitter` + 4 language grammars | Latest stable | Universal fast-path parsing (Tier 1) |
| TS/JS Resolution | `oxc_resolver` + `oxc_semantic` | v0.111+ | TypeScript/JavaScript Tier 2 enhancement |
| Python Resolution | `ty` (subprocess) | v0.0.15+ | Python Tier 2 enhancement via `ty --output-format json` |
| Go Resolution | tree-sitter heuristics | N/A | Go Tier 2 — no external dependency |
| Rust Resolution | `ra_ap_ide` crates | 0.0.x (unstable) | Rust Tier 2 — lazy-loaded due to 60s+ startup |
| Graph | `petgraph` | Latest stable | Function/class/module graph with call edges |
| Hashing | `xxhash-rust` | Latest stable | Content-addressed function hashing (>10GB/s) |
| Database | `rusqlite` (with `bundled` feature) | Latest stable | SQLite for graph storage, statically linked |
| CLI | `clap` | Latest stable | Argument parsing |
| Serialization | `serde` + `serde_json` | Latest stable | JSON output for `--json` flag |
| Parallelism | `rayon` | Latest stable | Parallel file parsing |

**Constraints:**
- No FFI in the hot path. tree-sitter is natively C/Rust. Oxc is native Rust. SQLite statically linked.
- `ty` is the sole subprocess dependency (Python Tier 2 only). If `ty` is not installed, keel degrades gracefully to tree-sitter heuristics for Python.
- `rust-analyzer` is consumed as a Rust library (`ra_ap_ide`), not as a subprocess.
- Single binary. Zero runtime dependencies. See [[design-principles#Principle 10 One Binary Zero Runtime Dependencies|Principle 10]].

*Extracted from PRD 10.*

---

## Article 2: Resolution Architecture — 3-Tier Hybrid

The resolution engine uses a converged 3-tier architecture. This is not negotiable — three independent research sources (Perplexity, Gemini, Claude) converge on this as the only viable path.

**Tier 1 — Universal fast path (tree-sitter):**
- Parse all files with tree-sitter
- Extract definitions, call sites, imports via query patterns (leveraging `tags.scm`)
- Build file-level index with incremental updates in <1ms
- Use `rayon` for parallel parsing
- Resolves ~75-92% of cross-file references depending on language

**Tier 2 — Per-language enhancers:**

| Language | Enhancer | Precision Target |
|----------|----------|-----------------|
| TypeScript | Oxc (`oxc_resolver` + `oxc_semantic`) | ~85-93% |
| Python | ty subprocess (`ty --output-format json`) | ~82-99% |
| Go | Tree-sitter heuristics (explicit imports + package scoping) | ~85-92% |
| Rust | rust-analyzer lazy-load (`ra_ap_ide`) | ~75-99% |

**Tier 3 — On-demand fallback (LSP/SCIP):**
- For ambiguous references: multiple candidates, unresolved types, star/dynamic/conditional imports, trait/interface dispatch
- Query LSP server or pre-built SCIP index
- Cache results aggressively
- Not always-on — a precision knob the user can enable
- Lifts precision to >95% where needed

**Why no alternatives:**

| Approach | Verdict |
|----------|---------|
| Pure tree-sitter | Too imprecise for enforcement (65-85%) |
| Pure LSP | Memory/startup impossible (5-13GB for 4 servers) |
| Pure SCIP | Too slow, no incrementality (20-100s indexing) |
| stack-graphs | Dead — GitHub archived Sept 2025 |
| **Hybrid 3-tier** | **Only viable path** — 1-5s parse, 10-80ms incremental, 200MB-1.5GB memory, ~87-95% precision |

*Extracted from PRD 10.1.*

---

## Article 3: Graph Schema

The graph uses three node types and four edge types. These struct definitions are the bedrock — every other subsystem depends on them.

**Node types:**
```
enum NodeKind { Module, Class, Function }

struct GraphNode {
    id: u64,                          // Internal graph ID
    hash: String,                     // base62(xxhash64(...)), 11 chars
    kind: NodeKind,
    name: String,                     // Function/class/module name
    signature: String,                // Full normalized signature
    file_path: String,                // Relative to project root
    line_start: u32,
    line_end: u32,
    docstring: Option<String>,        // First line of docstring, if present
    is_public: bool,                  // Exported / public visibility
    type_hints_present: bool,         // All params and return type annotated?
    has_docstring: bool,              // Docstring present?
    external_endpoints: Vec<ExternalEndpoint>,
    previous_hashes: Vec<String>,     // Last 3 hashes for rename tracking
    module_id: u64,                   // Parent module node ID
}

struct ExternalEndpoint {
    kind: String,      // "HTTP", "gRPC", "GraphQL", "MessageQueue"
    method: String,    // "POST", "GET", etc.
    path: String,      // "/api/users/:id"
    direction: String, // "serves" or "calls"
}
```

**Edge types:**
```
enum EdgeKind { Calls, Imports, Inherits, Contains }

struct GraphEdge {
    source_id: u64,
    target_id: u64,
    kind: EdgeKind,
    file_path: String,   // Where the reference occurs
    line: u32,           // Line number of the reference
}
```

**Module placement profile:**
```
struct ModuleProfile {
    module_id: u64,
    path: String,
    function_count: u32,
    function_name_prefixes: Vec<String>,
    primary_types: Vec<String>,
    import_sources: Vec<String>,
    export_targets: Vec<String>,
    external_endpoint_count: u32,
    responsibility_keywords: Vec<String>,
}
```

*Extracted from PRD 11.*

---

## Article 4: Hash Design

```
hash = base62(xxhash64(canonical_signature + body_normalized + docstring))
```

- **xxHash64** for speed (>10GB/s)
- **base62 encoding** — 11 chars, URL-safe
- **Canonical signature** — normalized function declaration (name, params with types, return type), whitespace/comment stripped
- **Body normalized** — AST-based normalization (strip comments, normalize whitespace), NOT raw text
- **Docstring included** — forces hash change when documentation changes
- **Collision handling:** If hash exists for different function, append disambiguator (file path hash) and re-hash. No two distinct functions may share the same hash (enforced at write time).
- **Rename tracking:** `previous_hashes` list per node (last 3) for `discover`/`where` to resolve recently-changed hashes with `RENAMED` flag.

*Extracted from PRD 5, 10.3.*

---

## Article 5: Enforcement Philosophy

**The LLM is the primary user.** keel enforces rules on the LLM that would be unreasonable to demand from humans. The human is the engineer who designs the system, reviews output, and sets rules.

**Four pillars:**
1. **Backpressure** — Force verify before and validate after every edit
2. **Contract enforcement** — Type contracts, signature contracts, adjacency contracts
3. **Structural navigation** — How code is connected, not just what it looks like
4. **Placement intelligence** — Where new code belongs

**Progressive adoption:**
- `ERROR` for new/modified code
- `WARNING` for pre-existing code
- Configurable escalation schedule in `.keel/config.toml`
- Goal: universal enforcement. Path: progressive adoption.

**Suppression layers (for false positives):**
1. Inline: `# keel:suppress E001 -- reason`
2. Config: `[suppress]` section in `.keel/config.toml` (reason required)
3. CLI: `keel compile --suppress W001` (single invocation)
4. Suppressed violations downgraded to `INFO` (S001), never silently hidden

**Dynamic dispatch:** Low-confidence edges produce `WARNING`, not `ERROR`. Prevents false positives from blocking the LLM on ambiguous resolutions.

*Extracted from PRD 2, 4.4, 8.*

---

## Article 6: Output Contracts

JSON output schemas are API surfaces. They are frozen and versioned.

**Error codes (frozen):**

| Code | Category | Severity | Description |
|------|----------|----------|-------------|
| E001 | broken_caller | ERROR | Function signature changed, callers expect old signature |
| E002 | missing_type_hints | ERROR | Function parameters or return type lack type annotations |
| E003 | missing_docstring | ERROR | Public function has no docstring |
| E004 | function_removed | ERROR | Function deleted but still has callers |
| E005 | arity_mismatch | ERROR | Parameter count changed, callers pass wrong argument count |
| W001 | placement | WARNING | Function may be better placed in different module |
| W002 | duplicate_name | WARNING | Function with same name exists elsewhere |
| W003 | naming_convention | WARNING | Name doesn't match module naming pattern (Phase 2) |
| W004 | cross_repo_endpoint | WARNING | Changed endpoint consumed by linked repo (Phase 2) |
| S001 | suppressed | INFO | Violation suppressed via inline or config |

**Common fields on all error/warning objects:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `confidence` | float 0.0-1.0 | Always | Resolution confidence. 1.0 = certain. <0.7 = heuristic/ambiguous |
| `resolution_tier` | string enum | Always | `tier1_treesitter`, `tier2_oxc`, `tier2_ty`, `tier2_treesitter_heuristic`, `tier2_rust_analyzer`, `tier3_lsp`, `tier3_scip` |
| `fix_hint` | string | ERROR: always. WARNING: where applicable | Text instruction telling the LLM what to do |

**Clean compile behavior:** Zero errors AND zero warnings = exit 0, empty stdout. `info` block only with `--verbose` or alongside errors/warnings.

**Exit codes:**
- `0` — success, no violations
- `1` — violations found
- `2` — keel internal error

*Extracted from PRD 6, 12.*

---

## Article 7: Testing Standards

**Resolution precision gate:** >85% cross-file resolution precision per language, measured against LSP ground truth on test corpus. This gate blocks M1 advancement.

**Enforcement correctness:**
- True positive rate: >95% (catches known mutations)
- False positive rate: <5% on correctly-placed code

**Test corpus (minimum 10 repos):**

| Language | Repos |
|----------|-------|
| TypeScript | excalidraw (~120k LOC), cal.com (~200k LOC), typescript-eslint (~80k LOC) |
| Python | FastAPI (~30k LOC), httpx (~25k LOC), django-ninja (~15k LOC) |
| Go | cobra (~15k LOC), fiber (~30k LOC) |
| Rust | ripgrep (~25k LOC), axum (~20k LOC) |
| Multi-language | Purpose-built test repo with known cross-file references |

**4 test categories:**
1. Graph correctness (`init` + `map`) — precision/recall vs LSP ground truth
2. Enforcement correctness (`compile`) — mutation testing, circuit breaker, fix hints
3. Performance benchmarks — hard numeric targets (see Article 8)
4. LLM integration (end-to-end) — Claude Code + Cursor + Codex on real repos

*Extracted from PRD 19.*

---

## Article 8: Performance Targets

These are pass/fail gates, not aspirational goals:

| Operation | Target | Context |
|-----------|--------|---------|
| `keel init` | <10s | 50k LOC TypeScript + Python project |
| `keel map` | <5s / <30s | 100k LOC / 500k LOC |
| `keel compile` (single file) | <200ms | Incremental tree-sitter parsing |
| `keel discover` | <50ms | Graph traversal + adjacency lookup |
| `keel explain` | <50ms | Reads from existing resolution data |
| `--llm` map tokens | <5% of 200k context | ~2500 functions (~10k tokens at ~4 tokens/function) |
| `keel serve` memory | ~50-100MB / ~200-400MB | 50k LOC / 200k LOC (full graph in memory) |
| CLI memory | ~20-50MB | Load subgraph from SQLite per call |
| Binary size | 20-35MB | 4 grammars + Oxc + SQLite + resolution engine |

*Extracted from PRD 4.2, 4.3, 4.4, 4.8, 10, 17.*

---

## Article 9: Licensing and Distribution

**License:** FSL (Functional Source License)
- Free for individual developers and internal use
- Free for non-competing commercial use
- Paid license for companies embedding keel in products they sell
- Converts to Apache 2.0 after 2 years

**Distribution:**
- Single binary via `cargo build --release`
- Pre-built binaries: Linux (x86_64, arm64), macOS (arm64, x86_64), Windows (x86_64)
- Install: `curl | sh`, `brew install keel`, `cargo install keel`, `winget install keel`, `scoop install keel`
- No runtime dependencies
- Windows: native binary, no WSL. Platform-native path handling internally, forward slashes in all output.

*Extracted from PRD 10, 15.*

---

## Article 10: Spec Authority

The authority chain is:

```
Constitution (this document)
    ↓ governs
Specs (keel-speckit/*.md)
    ↓ implement articles
Code (Rust source)
    ↓ implements specs
```

- If a spec contradicts this constitution, the constitution wins
- If code contradicts a spec, the spec wins
- Agents read their assigned specs, not the PRD
- PRD section numbers are cited for traceability but are not the authority — the spec content is
- Specs are self-contained: every struct, schema, error code, and performance target relevant to the spec is extracted inline

---

## Related Documents

- [[PRD_1|PRD v2.1]] — The master source document
- [[design-principles|Design Principles]] — Implementation philosophy (the "why")
- [[agent-swarm|Agent Swarm Playbook]] — Runnable playbook
- [[CLAUDE|CLAUDE.md]] — Agent implementation guide
- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — Bedrock data structures
