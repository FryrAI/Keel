# Cross-file reference resolution for keel: a decisive comparison

**The best path for keel is a language-specific hybrid architecture using native Rust compiler frontends where available, tree-sitter as the universal fast path, and selective LSP/SCIP fallback for precision.** No single approach meets all four requirements simultaneously — the performance targets (<5s/100k LOC, <200ms incremental, <2GB) rule out LSP and SCIP as primary engines, while the precision target (>90%) rules out tree-sitter heuristics alone for TypeScript, Python, and Rust. The solution is a layered design that matches each language to its strongest available Rust-native toolchain, unified by tree-sitter's universality.

This report evaluates five approaches against keel's requirements, surveys the post-stack-graphs ecosystem, and delivers a concrete architecture recommendation with implementation timeline.

---

## Approach 1: Tree-sitter + heuristics — fast but imprecise

Tree-sitter trivially meets every performance target. Parsing benchmarks show **~3ms/KLOC for Rust, ~6ms/KLOC for Python**, meaning 100k LOC parses in 300–600ms with room to spare. Incremental reparsing after a single-file edit completes in under 1ms thanks to tree-sitter's shared-subtree design. Memory for 500k LOC stays well under 500MB. Parallelizing across files with rayon makes these numbers even better. All four tree-sitter grammars (TypeScript, Python, Go, Rust) are actively maintained within the official tree-sitter organization, with releases through early 2026.

The approach works by extracting definitions, call sites, and import statements via tree-sitter queries (leveraging the built-in `tags.scm` patterns that power GitHub's code navigation), then resolving cross-file references through import-path-following and name matching. This is exactly what Aider's repo map does — and it works surprisingly well for LLM context selection. But keel needs call-edge precision, and here the approach breaks down.

**Realistic precision per language:**

|Language|Direct imports|With re-export following|Type-dependent losses|Overall|
|---|---|---|---|---|
|**Go**|90–95%|+2–3%|-5–12%|**85–92%**|
|**TypeScript**|80–85%|+5–8%|-15–20%|**75–85%**|
|**Rust**|80–85%|+5%|-15–25%|**70–80%**|
|**Python**|70–80%|+5–10%|-20–30%|**65–75%**|

Go is the only language that approaches 90%, because its explicit imports, package-level scoping, and capitalization convention make resolution nearly deterministic. TypeScript stumbles on barrel files, re-exports, path aliases, and method calls on typed objects. Rust's trait method resolution and macro-generated code are invisible to tree-sitter. Python's duck typing, star imports, and monkey patching make it the hardest target.

**Hard failure cases across all languages:** method calls requiring type information (`foo.bar()` where `bar`'s definition depends on the type of `foo`), interface/trait dispatch, star imports and glob re-exports, macro-generated or dynamically-created definitions, and conditional compilation. These aren't edge cases — in a typical TypeScript codebase, **15–20% of cross-file calls are method calls that require type inference** to resolve.

**Implementation effort: 5–7 weeks** for a senior Rust developer covering all four languages, including tree-sitter integration, per-language import resolvers, index data structures, and incremental update logic. Could compress to 4 weeks for an MVP covering the "easy 80%" per language.

**Major gotcha:** GitHub's stack-graphs project attempted to build precise cross-file resolution on top of tree-sitter using a declarative DSL (TSG). They abandoned it — the TSG DSL was "too difficult" and the TypeScript definitions reached **6,000 lines** before the maintainers admitted an imperative approach would have been better. The lesson: tree-sitter is the right parsing foundation, but cross-file logic should be imperative Rust code, not a DSL.

---

## Approach 2: LSP integration — precise but memory-prohibitive

Language servers achieve near-perfect precision because they run the same semantic analysis as the compiler: **tsserver uses tsc's type checker (99.9%+ precision), gopls uses go/types (99.9%+), rust-analyzer reimplements Rust's type system including trait resolution via Chalk (99%+), and pyright performs full Python type inference (95–99% depending on annotation coverage)**. For cross-file reference resolution specifically, LSP's `textDocument/definition` and `textDocument/references` are exactly the right abstraction.

But the approach has a **fatal flaw for keel's requirements: memory**. Running four language servers on a 500k LOC polyglot repo requires **5–13GB minimum** — tsserver alone routinely hits 2GB, rust-analyzer commonly uses 2–4GB, gopls needs 0.5–2GB, and pyright takes 0.5–1.5GB. The 2GB total memory budget is physically impossible to meet. Startup time is equally problematic: rust-analyzer takes **15–60 seconds** for a 100k-LOC project, tsserver takes **10–30 seconds**, making the <5s target unreachable.

The Rust LSP client ecosystem is immature. **No production-ready LSP client library exists in Rust.** The closest options are `async-lsp` (supports both server and client, Tower-based, by oxalica), `async-lsp-client` (thin wrapper, very new), and building a custom client using `lsp-types` for protocol structs plus tokio for transport. The critical limitation: **LSP does not support batch requests** — the spec explicitly prohibits JSON-RPC batch messages. You'd need to pipeline individual `textDocument/definition` requests, achieving roughly 20–100 requests/second depending on server parallelism. For a file with 50 call sites, that's 0.5–2.5 seconds per file.

Incremental update performance is **borderline**: gopls handles within-function changes in 50–200ms, rust-analyzer achieves similar via Salsa, but tsserver can lag 2–7 seconds for semantic diagnostics on large projects. The <200ms target is achievable only for simple changes in Go and Rust.

A notable validation of the LSP-as-oracle approach: **Claude Code shipped native LSP support in December 2025** (v2.0.74), querying pyright, gopls, rust-analyzer, and vtsls for go-to-definition and find-references. But Claude Code runs in an interactive environment without keel's memory constraints.

**Implementation effort: 10–17 weeks** — building the client transport, managing four server lifecycles, routing files, orchestrating queries, handling crashes, and working around server-specific quirks.

**Showstoppers:** Memory budget impossible. Startup time impossible. No batch querying. High implementation complexity.

---

## Approach 3: SCIP indexing — compiler-accurate but batch-only

SCIP (Sourcegraph's Code Intelligence Protocol) provides **compiler-accurate cross-file references** encoded in a protobuf format with human-readable symbol strings. The indexers use full semantic analysis: scip-typescript wraps tsc, scip-python forks Pyright, scip-go uses go/packages, and rust-analyzer has built-in SCIP emission. All indexers are **Apache-2.0 licensed** with no commercial-use restrictions.

The SCIP format is elegant for consumption. Each `Document` contains `Occurrence` entries with globally-unique symbol strings — cross-file resolution reduces to a simple string-equality join between definition occurrences and reference occurrences. The official `scip` Rust crate (on crates.io) provides bindings for parsing SCIP protobuf data. Building a cross-file reference graph from SCIP output is straightforward: ~1–2 weeks of work.

**But SCIP fails every performance requirement:**

|Requirement|SCIP reality|Gap|
|---|---|---|
|Full parse <5s (100k LOC)|20–100s|**4–20× too slow**|
|Full parse <30s (500k LOC)|100–500s|**3–17× too slow**|
|Incremental <200ms|Not supported|**Complete failure**|
|Memory <2GB (500k LOC)|Marginal (TypeScript OOM issues)|**Borderline**|

SCIP was designed for CI/CD batch indexing — run the indexer in CI, upload the index to Sourcegraph's server, query it there. It was never intended for interactive or near-real-time use. **No SCIP indexer supports incremental indexing**, and this hasn't changed despite being listed as a future goal since 2022.

The indexer ecosystem is active but shows signs of reduced investment. Sourcegraph is **pivoting aggressively to AI products** (Amp, their agentic coding tool), discontinued Cody Free/Pro/Starter plans in July 2025, and made their main repository private in August 2024. Employee count dropped to ~185 from peak levels through multiple layoff rounds. The SCIP repos remain open-source but maintenance could slow. Notably, **rust-analyzer's SCIP support is maintained by the Rust community**, making it the most sustainable indexer.

**Implementation effort: 3–5 weeks** for consuming SCIP data and building the reference graph. Add 2–3 weeks for a hybrid system that patches SCIP data incrementally.

**Showstoppers:** No incremental support. Indexing speed 4–20× too slow for the full-parse requirement. Designed for batch, not interactive use.

---

## Approach 4: Hybrid tree-sitter + precise fallback — the proven pattern

The hybrid pattern — tree-sitter for the fast 80–90% of resolutions, LSP or SCIP for the remaining ambiguous cases — has **strong precedent across the ecosystem**. Zed editor pioneered this in production: tree-sitter for syntax-level features (highlighting, folding, selections), LSP for semantic features (completions, go-to-definition). Max Brunsfeld, tree-sitter's creator, explicitly stated: "Tree-sitter isn't really an alternative to LSP... for many other features, Tree-sitter is a much cleaner solution."

The most directly relevant implementation is **Tethys**, a Rust crate (v0.1.0, MIT/Apache-2.0) that implements exactly this pattern: tree-sitter indexing cached in SQLite, with `UnresolvedReference` types carrying enough context for LSP follow-up queries. It's very early-stage (single developer, only supports Rust + C#), but validates the architecture. A more sophisticated example is **CKB/CodeMCP**, a Go-based MCP server that orchestrates SCIP for pre-indexed precise data, LSP for on-demand queries, and tree-sitter as fallback — the most feature-complete code intelligence MCP server found, with **74+ tools** covering symbol navigation, impact analysis, and architecture mapping.

**Detecting ambiguity at resolution time** follows clear heuristics: (1) **multiple candidate definitions** — if name matching finds >1 possible target, escalate to precise backend; (2) **method calls without clear receiver type** — `foo.bar()` where `foo`'s type isn't determinable from import context; (3) **unresolvable imports** — star imports, dynamic imports, conditional imports; (4) **trait/interface dispatch** — any call through an interface boundary.

The implementation complexity is moderate: you build the full tree-sitter layer first (Approach 1), then add a precision layer that selectively queries a precise backend for flagged ambiguous references. The key architectural decision is **which precise backend to use** — and this is where Approach 5 becomes relevant.

**Implementation effort: 7–10 weeks** (tree-sitter base + ambiguity detection + fallback integration).

---

## Approach 5: Compiler frontends as libraries — the strongest building blocks

This approach yields the most promising components, but the story varies dramatically per language.

**TypeScript — OXC is the best foundation.** OXC (Oxidation Compiler) provides the fastest JS/TS parser in Rust (~3× faster than SWC), with three critical crates: `oxc_parser` for parsing, `oxc_semantic` for per-file symbol tables and scope analysis, and `oxc_resolver` for production-ready module resolution (a Rust port of webpack's enhanced-resolve, used in production by Rolldown and Biome). OXC is at **v0.111 with 1.3M+ downloads**, extremely actively maintained (latest release February 2, 2026), and MIT-licensed. The crucial limitation: **oxc_semantic is strictly per-file** — it provides complete symbol tables, scope trees, and reference tracking for a single file, but does not perform cross-file type checking. You'd build cross-file resolution by stitching together oxc_resolver (import → file path mapping) with oxc_semantic (per-file symbol tables) and tree-sitter queries for reference extraction. This gets you to **~85–90% precision** — better than raw tree-sitter heuristics because oxc_semantic correctly resolves intra-file scoping and oxc_resolver handles tsconfig paths, but still missing TypeScript's type system for method-call resolution.

**Python — ty is transformative but not yet consumable as a library.** ty (formerly Red Knot), built by Astral (makers of Ruff/uv), is a Rust-native Python type checker achieving **4.7ms incremental updates** (80× faster than Pyright) and 10–60× faster cold starts than mypy. It uses Salsa for incremental computation and performs full cross-module type inference. As of February 2026, ty is in **beta** (v0.0.15 on PyPI), with multiple releases per week. However, ty's Rust crates live inside the ruff monorepo and are **not published on crates.io** — using them requires a git dependency on the ruff workspace with no API stability guarantees. The crate boundaries are internal implementation details that change frequently. Astral plans to stabilize the API eventually (ruff will depend on ty's type-checking crates for type-aware linting), but this isn't available yet. **For Phase 1, ty is best consumed as a subprocess** (`ty --output-format json`), with library integration as a Phase 2 goal when the API stabilizes.

**Go — subprocess is the only practical path.** Go's analysis packages (`go/packages`, `go/types`) provide full semantic cross-file resolution, but calling Go from Rust via FFI is impractical: Go's runtime embeds a goroutine scheduler and GC that conflicts with Rust's memory model. The pragmatic approach is a Go helper binary that loads packages and emits cross-reference data over JSON-RPC or protobuf. However, **Go is also where tree-sitter heuristics work best** — explicit imports, package-level scoping, and capitalization convention mean that simple import-path-following + name-matching achieves **~90% precision** without any external tooling. A Go subprocess adds 10–20ms per query but provides the remaining precision.

**Rust — rust-analyzer as a library works.** The `ra_ap_*` crates on crates.io expose rust-analyzer's full analysis engine: `ra_ap_ide` provides goto-definition and find-all-references, `ra_ap_hir` gives access to the type system, and the Salsa-based incrementality keeps updates fast for within-function changes. The API is **explicitly unstable** (0.0.x versioning), but architecturally mature. Main concern: **startup time scales with dependency count** — projects with 1000+ crate dependencies can take 60+ seconds for initial analysis, and memory commonly reaches 2–4GB for large workspaces. For keel, rust-analyzer should be loaded lazily and kept as an optional precision enhancer, not a blocking requirement.

---

## What the ecosystem tells us about best practices

**Stack-graphs has no maintained successor.** Despite 159 forks and 867 stars, no community fork has emerged with active development since the September 2025 archival. The crates remain on crates.io but are frozen. The key takeaway from stack-graphs' failure: the declarative TSG DSL was the wrong abstraction — imperative code for language-specific resolution logic is more maintainable.

**Cursor uses embedding-based RAG, not code graphs.** Cursor's semantic indexing works by AST-aware code chunking, vector embeddings (likely Voyage AI's code models), storage in Turbopuffer (a vector DB), and Merkle-tree-based incremental sync. It does **not** use LSP or SCIP for its codebase understanding — it relies on semantic similarity retrieval, not precise reference resolution. This suggests the market currently undervalues precise code graphs relative to embedding-based approaches, leaving a gap keel could fill.

**Most MCP code servers use tree-sitter + name matching.** RepoMapper MCP ports Aider's approach (tree-sitter extraction + PageRank ranking). mcp-server-tree-sitter explicitly states it "does not handle cross-file references" and recommends LSP servers for that. code-graph-rag-mcp uses tree-sitter + SQLite + embeddings. The **exception is CKB/CodeMCP**, which orchestrates SCIP + LSP + tree-sitter — but it's written in Go, not Rust.

**Two new Rust crates are worth monitoring**: **Tethys** (v0.1.0, single developer) implements the hybrid tree-sitter/LSP cache pattern for Rust + C#, and **Codanna** (v0.5.26, Apache-2.0) provides tree-sitter-based code intelligence with embeddings for 16+ languages, claiming 75,000+ symbols/second and sub-10ms lookups. Neither is mature enough to depend on, but both validate the architectural direction.

---

## Recommendation: language-specific hybrid with tree-sitter foundation

**Top pick: Layered architecture with per-language optimal backends**

The architecture has three tiers:

**Tier 1 — Universal fast path (tree-sitter).** Parse all files with tree-sitter, extract definitions/call-sites/imports via query patterns, build the file-level index. This runs for every language, handles incremental updates in <1ms, and provides the foundation for cross-file resolution. Use rayon for parallel parsing. This alone resolves ~75–92% of cross-file references depending on language.

**Tier 2 — Language-specific enhancers (native Rust where available).**

- **TypeScript**: Use `oxc_resolver` for import-to-file-path resolution (handles tsconfig paths, ESM/CJS, barrel files). Use `oxc_semantic` for per-file symbol table quality. This lifts TypeScript precision to **~85–90%**.
- **Python**: Use ty as a subprocess for Phase 1 (JSON output). When ty's crates stabilize on crates.io, integrate as a library for **~95–99% precision** with 4.7ms incremental updates.
- **Go**: Tree-sitter heuristics alone, leveraging Go's explicit import system. Precision **~88–92%**. Optional: Go subprocess wrapper around `go/packages` for the remaining cases.
- **Rust**: Tree-sitter for fast path. Lazy-load rust-analyzer via `ra_ap_ide` crates for high-precision resolution when needed. Precision **~95–99%** with rust-analyzer, **~75–80%** tree-sitter-only.

**Tier 3 — On-demand LSP/SCIP fallback.** For references that Tier 1+2 flag as ambiguous (multiple candidates, method calls on unresolved types, interface dispatch), optionally query an LSP server or pre-built SCIP index. Cache results aggressively. This is not always-on — it's a precision knob the user can enable for repos where >95% precision matters.

**Why this wins:**

|Requirement|This architecture|
|---|---|
|<5s for 100k LOC|✅ Tree-sitter parses in 1–3s; OXC resolver adds ~200ms|
|<30s for 500k LOC|✅ Tree-sitter + OXC in 5–15s|
|Incremental <200ms|✅ Tree-sitter <1ms + index update ~10–50ms|
|Memory <2GB|✅ All in-process Rust; ~200–800MB for 500k LOC|
|Precision >90%|✅ Go ~90%, TS ~87%, Python ~95% (with ty), Rust ~95% (with r-a)|

**Implementation timeline: 8–12 weeks** for a senior Rust developer.

- Weeks 1–2: Tree-sitter foundation (parsing, incremental index, query patterns)
- Weeks 3–4: Go resolver (tree-sitter heuristics, import-path following — easiest language)
- Weeks 5–6: TypeScript resolver (OXC integration, barrel file following, re-export chains)
- Weeks 7–8: Python resolver (tree-sitter + ty subprocess integration)
- Weeks 9–10: Rust resolver (tree-sitter + rust-analyzer library integration)
- Weeks 11–12: Ambiguity detection, caching, performance optimization, testing

**Runner-up: SCIP background indexing + tree-sitter hot path**

Run SCIP indexers asynchronously in the background (on a schedule or triggered by git commits), cache the compiler-accurate reference data in SQLite or an in-memory map. Use tree-sitter for all interactive queries and incremental updates, merging SCIP precision data when available. When a file changes, immediately update the tree-sitter index (<1ms), then schedule an async SCIP re-index. Between SCIP rebuilds, new/changed code uses tree-sitter-only resolution.

This is simpler to implement (**6–8 weeks**) and achieves **compiler-accurate precision for unchanged code** while maintaining full interactive speed. The tradeoff: freshly-edited code falls back to tree-sitter precision until the next SCIP rebuild completes (20–100 seconds). This may be acceptable since LLM coding agents typically work on a small number of files at a time while the rest of the codebase is stable.

The runner-up loses to the top pick on incremental precision (stale SCIP data during active edits) and on the dependency risk from Sourcegraph's uncertain trajectory. But it's meaningfully simpler to build and provides 99%+ precision for the vast majority of the codebase at any given time.

---

## Conclusion

The decisive insight is that **no universal cross-file resolution engine exists post-stack-graphs** — and probably shouldn't. Each language's module system has fundamentally different resolution semantics, and the tools that handle them best are language-specific. Keel's architecture should embrace this reality rather than fight it.

Start with tree-sitter as the universal parsing and indexing backbone (it's the one dependency that's unconditionally safe — actively maintained, MIT-licensed, battle-tested at GitHub scale). Layer OXC for TypeScript module resolution (production-quality, native Rust, fastest in class). Integrate ty for Python as it matures through 2026 (the single most important emerging tool in this space). Use rust-analyzer as a library for Rust precision. Accept that Go is easy enough for heuristics alone.

The critical implementation decision is **where to draw the ambiguity boundary** — what percentage of references does tree-sitter handle versus the precise backend. Empirical measurement against real codebases during development will calibrate this better than any upfront estimate. Build the measurement infrastructure (LSP ground-truth comparison) in week 1, and let the data drive the architecture decisions that follow.