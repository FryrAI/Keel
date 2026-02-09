# Architecture Synthesis

Sources: [[perplexity_arch]], [[gemini_arch]], [[claude_arch]]

---

## Consensus

**All three sources agree on three foundational points:**

1. **The hybrid approach is the only viable path.** No single engine meets all four requirements simultaneously. The performance targets (<5s/100k LOC, <200ms incremental, <2GB memory) rule out LSP and SCIP as primary engines. The precision target (>90%) rules out tree-sitter heuristics alone.

2. **Tree-sitter is the universal foundation.** Every source picks tree-sitter as the fast parsing layer. It's unconditionally safe — actively maintained, MIT-licensed, battle-tested at GitHub scale. Parsing benchmarks: ~3-6ms/KLOC, incremental reparsing <1ms, memory well under 500MB for 500k LOC.

3. **No universal cross-file resolution engine exists post-stack-graphs.** And probably shouldn't. Each language's module system has fundamentally different resolution semantics. The tools that handle them best are language-specific. keel's architecture should embrace this reality.

---

## Key Discoveries

### stack-graphs is dead
- Archived September 9, 2025. "No longer supported or updated."
- No community fork has emerged with active development despite 159 forks
- The TSG DSL was the wrong abstraction — imperative code for language-specific resolution is more maintainable
- Key lesson: the "universal DSL" model failed. The industry has corrected toward fast, language-specific tooling

### Oxc (TypeScript) — production-ready, native Rust
- The Oxidation Compiler has won the race for high-performance JS/TS tooling
- `oxc_resolver`: Rust port of webpack's enhanced-resolve, **30x faster**, handles tsconfig paths, ESM/CJS, barrel files
- `oxc_semantic`: per-file symbol tables, scope analysis — correctly resolves intra-file scoping
- v0.111+, 1.3M+ downloads, extremely actively maintained (latest release Feb 2, 2026), MIT-licensed
- **Critical limitation**: `oxc_semantic` is strictly per-file — no cross-file type checking. Cross-file resolution must be stitched together with tree-sitter queries
- Used in production by Rolldown and Biome

### Ty (Python) — transformative but beta
- Formerly Red Knot, built by Astral (makers of Ruff/uv)
- Full Rust-native Python type checker using Salsa incremental computation framework
- **4.7ms incremental updates** (80x faster than Pyright), 10-60x faster cold starts than mypy
- Beta as of Feb 2026 (v0.0.15 on PyPI), multiple releases per week
- **Not yet consumable as a Rust library**: crates live inside the ruff monorepo, not published on crates.io, no API stability guarantees
- For Phase 1, best consumed as a subprocess (`ty --output-format json`). Library integration is Phase 2 when API stabilizes.

### TypeScript 7 (Project Corsa) — in Go, not Rust
- Microsoft's rewrite of the TypeScript compiler is in **Go**, not Rust
- Complicates embedding the official TS compiler into a Rust binary
- Reinforces the value of Oxc as a Rust-native alternative for analysis

### Cursor uses embeddings, not code graphs
- AST-aware code chunking via tree-sitter + vector embeddings (likely Voyage AI) + Turbopuffer vector DB + Merkle-tree incremental sync
- Does **not** use LSP or SCIP for codebase understanding — relies on semantic similarity retrieval, not precise reference resolution
- But uses a "Shadow Workspace" (hidden VS Code instance with real LSP) for enforcement/verification
- Suggests the market undervalues precise code graphs — gap keel could fill

---

## Per-Language Recommendations

**Converged table — all three sources agree on the best engine per language:**

| Language | Primary Engine | Precision | Fallback | Notes |
|----------|---------------|-----------|----------|-------|
| **TypeScript** | Oxc (`oxc_resolver` + `oxc_semantic`) + tree-sitter | ~85-93% | LSP (tsserver/vtsls) for method calls on typed objects | Barrel files and re-exports handled by oxc_resolver; type-dependent resolution needs LSP |
| **Python** | Tree-sitter + ty subprocess (Phase 1), ty library (Phase 2) | ~82-99% (ty-dependent) | Pyright LSP if ty unavailable | ty is the single most important emerging tool; precision jumps dramatically with it |
| **Go** | Tree-sitter heuristics alone | ~85-92% | Optional Go subprocess (`go/packages`) | Go is easiest — explicit imports, package scoping, capitalization convention make heuristics work. No Go analysis library exists for Rust (FFI impractical). |
| **Rust** | Tree-sitter + rust-analyzer (`ra_ap_ide` crates) | ~75-99% (r-a dependent) | rust-analyzer has built-in SCIP emission | API explicitly unstable (0.0.x). Startup 60+ seconds for large workspaces. Load lazily. |

### Where sources disagree on per-language approach

- **Perplexity** favors tree-sitter + LSP fallback for all languages (generic hybrid)
- **Gemini** emphasizes compiler frontends as libraries (Oxc, Ty) as the primary approach ("The Top Recommendation")
- **Claude** is closest to Gemini but more conservative on ty (subprocess first, library later) and more optimistic about Go heuristics

---

## 3-Tier Architecture

All sources converge on a layered design, though they name the tiers differently:

### Tier 1 — Universal fast path (tree-sitter)
- Parse all files with tree-sitter
- Extract definitions, call sites, imports via query patterns (leveraging `tags.scm`)
- Build file-level index with incremental updates in <1ms
- Use rayon for parallel parsing
- **Resolves ~75-92% of cross-file references** depending on language
- Go is nearly complete at this tier; other languages need enhancement

### Tier 2 — Language-specific enhancers (native Rust where available)
- **TypeScript**: `oxc_resolver` for import-to-file-path + `oxc_semantic` for per-file symbol tables
- **Python**: ty subprocess (Phase 1) → ty library (Phase 2)
- **Go**: Tree-sitter heuristics sufficient (optional Go subprocess for remaining cases)
- **Rust**: Lazy-load rust-analyzer via `ra_ap_ide` crates
- Lifts precision to **~85-95%** per language

### Tier 3 — On-demand fallback (LSP/SCIP)
- For references flagged as ambiguous by Tier 1+2:
  - Multiple candidate definitions
  - Method calls on unresolved types (`foo.bar()` where foo's type unknown)
  - Star imports, dynamic imports, conditional imports
  - Trait/interface dispatch
- Query LSP server or pre-built SCIP index
- Cache results aggressively
- **Not always-on** — a precision knob the user can enable
- Lifts precision to **>95%** where needed

---

## Performance Estimates

**All requirements met by the hybrid architecture:**

| Requirement | Target | Estimated | Source Consensus |
|-------------|--------|-----------|-----------------|
| Full parse 100k LOC | <5s | 1-5s | All agree ✅ |
| Full parse 500k LOC | <30s | 5-25s | All agree ✅ |
| Incremental update | <200ms | 10-80ms | All agree ✅ (tree-sitter <1ms + index update ~10-50ms) |
| Memory 500k LOC | <2GB | 200MB-1.5GB | All agree ✅ (in-process Rust, no external server overhead) |
| Precision >90% | >90% | ~87-95% | Varies by language; achievable with Tier 2 enhancers |

### Performance comparison of rejected approaches

| Approach | 100k LOC Parse | Incremental | Memory | Precision | Verdict |
|----------|---------------|-------------|--------|-----------|---------|
| Pure tree-sitter | ✅ 2-3s | ✅ 10-50ms | ✅ ~300MB | ❌ 65-85% | Too imprecise for enforcement |
| Pure LSP | ❌ 4-6s startup per server | ❌ 100-400ms | ❌ 5-13GB (4 servers) | ✅ 95-100% | Memory/startup impossible |
| Pure SCIP | ❌ 20-100s indexing | ❌ No incremental | ✅ ~400MB | ✅ 95-98% | Too slow, no incrementality |
| Hybrid (recommended) | ✅ 1-5s | ✅ 10-80ms | ✅ 200MB-1.5GB | ✅ ~87-95% | Only viable path |
| Compiler frontends | ✅ <5s (Oxc/ty) | ✅ <5ms (ty) | ✅ Controllable | ✅ >95% (where available) | Best building blocks, not standalone |

---

## Timeline

Sources disagree on timeline, reflecting different assumptions about scope:

| Source | Estimate | Scope | Build Order |
|--------|----------|-------|-------------|
| **Perplexity** | 6-8 weeks | Tree-sitter + slim LSP client for all 4 languages | Weeks 1-2: tree-sitter infra. 3-4: LSP client. 5: hybrid routing. 6: caching. 7-8: multi-language polish. |
| **Gemini** | Phase 1 (no timeline given) | Oxc + SCIP + ty subprocess. Focus on TS first. | TS first → SCIP for Go/Rust → Python with ty → benchmarks |
| **Claude** | 8-12 weeks | Most conservative. Per-language sequential. | Weeks 1-2: tree-sitter foundation. 3-4: Go (easiest). 5-6: TypeScript (Oxc). 7-8: Python (ty subprocess). 9-10: Rust (r-a library). 11-12: optimization. |

**Realistic convergence: 8-10 weeks** for a senior Rust developer covering all four languages with the hybrid architecture. Go is easiest (tree-sitter heuristics work well), TypeScript next (Oxc is production-ready), Python depends on ty's stability, Rust depends on willingness to accept lazy-loaded rust-analyzer.

---

## Disagreements Between Sources

| Topic | Perplexity | Gemini | Claude |
|-------|-----------|--------|--------|
| **Tree-sitter precision for TS** | 70-75% | 55-65% (lowest estimate) | 75-85% |
| **Tree-sitter precision for Python** | 65-70% | 55-65% | 65-75% |
| **Hybrid precision for TS** | 88-93% | ~85-92% | ~87-90% (without LSP fallback) |
| **Primary vs fallback emphasis** | LSP as primary fallback ("Hybrid tree-sitter + LSP") | Compiler frontends as primary ("Polyglot Compiler" with Oxc/Ty) | Language-specific layering with tree-sitter always first |
| **Oxc emphasis** | Mentioned but not central | Central — "Oxc has effectively won the race" | Central — "fastest in class," "production-quality" |
| **Ty emphasis** | Mentioned as subprocess option | Central — calls it "transformative" | Important but cautious — "beta," "not yet consumable as library" |
| **Go strategy** | LSP fallback via gopls | SCIP ingestion via `scip-go` | Tree-sitter heuristics alone (Go is easy enough) |
| **Runner-up pick** | SCIP (if incremental indexing lands) | LSP-Bridge Daemon (managed LSP lifecycle) | SCIP background indexing + tree-sitter hot path |
| **Implementation weeks** | 6-8 | Not explicitly stated (phase-based) | 8-12 |
| **SCIP viability** | "Re-evaluate if incremental indexing becomes available" | "Essential for baseline state" for Go/Rust | "Batch-only, 4-20x too slow" — but good for background indexing |
| **rust-analyzer as library** | Possible but heavy, unstable API | Risks noted (startup time, memory 2-4GB) | "Works" but load lazily, keep as optional enhancer |

### The key strategic disagreement

**Perplexity** treats the hybrid as "tree-sitter with LSP fallback" — a generic two-layer system. **Gemini** and **Claude** both argue for a more nuanced per-language strategy where the "enhancer" layer is different for each language (Oxc for TS, ty for Python, tree-sitter-only for Go, rust-analyzer for Rust). This per-language approach is more complex to implement but produces better precision and avoids the LSP memory/startup problems.

---

## Ecosystem Signals

### Tools that validate the architecture

| Tool/Project | What It Shows |
|-------------|---------------|
| **Cursor's Shadow Workspace** | Even "fuzzy" retrieval tools use "precise" methods (real LSP) for enforcement — validates the hybrid approach |
| **Aider's repo map** | Tree-sitter + PageRank works well enough for LLM context selection, but not for enforcement precision |
| **CKB/CodeMCP** | Most feature-complete code intelligence MCP server: orchestrates SCIP + LSP + tree-sitter (in Go, not Rust) — validates the 3-tier approach |
| **Tethys** (Rust crate, v0.1.0) | Implements hybrid tree-sitter/LSP cache pattern with `UnresolvedReference` types — validates the ambiguity-detection pattern |
| **Codanna** (Rust crate, v0.5.26) | Tree-sitter code intelligence with embeddings, 75,000+ symbols/second — validates tree-sitter performance claims |
| **RepoMapper MCP** | Tree-sitter + PageRank for "important" code identification — validates tree-sitter as universal fast path |
| **mcp-server-tree-sitter** | Explicitly states "does not handle cross-file references" — confirms tree-sitter alone is insufficient |

### Technologies to monitor

| Technology | Status | Role for keel |
|-----------|--------|---------------|
| **Oxc** | Production-ready, v0.111+ | TS/JS resolution engine — ready now |
| **Ty** | Beta v0.0.15, rapid releases | Python resolution — subprocess now, library later |
| **Salsa** | Mature (core of ty and rust-analyzer) | Incremental computation framework — enables on-demand analysis |
| **SCIP** | Stable standard, but Sourcegraph pivoting to AI products | Data interchange format for Go/Rust baseline. Watch for maintenance risk. |
| **tree-sitter** | Mature, universally adopted | Universal fast path — unconditionally safe dependency |
| **rust-analyzer `ra_ap_*`** | Unstable API (0.0.x) but architecturally mature | Rust precision enhancer — load lazily |
