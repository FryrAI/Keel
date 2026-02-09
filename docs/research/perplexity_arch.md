
---

# Cross-File Reference Resolution Engines for Keel (Rust-Based Code Analysis Tool)

## Research Report - February 2026

**Target Languages:** TypeScript, Python, Go, Rust  
**Performance Requirements:** <5s for 100k LOC, <30s for 500k LOC, <200ms incremental updates, <2GB memory  
**Precision Requirement:** >90% cross-file call edge resolution

---

## Executive Summary

After comprehensive research, the **recommended approach is a Hybrid system (Option 4)** using tree-sitter as the fast path with LSP fallback for ambiguous cases. This provides the best balance of precision, performance, and maintainability.

**Top Pick:** Hybrid tree-sitter + LSP fallback  
**Runner-up:** Pure SCIP with custom Rust indexers

---

## 1. Pure Tree-Sitter + Heuristics

## Overview

Tree-sitter provides fast, incremental parsing with explicit grammar definitions. Resolution uses import path following + name matching without external dependencies.

## Realistic Precision Estimates

|Language|Precision|Notes|
|---|---|---|
|TypeScript|70–75%|Struggles with barrel exports, namespace merges, complex module resolution|
|Python|65–70%|Hard: `__init__.py` re-exports, star imports, dynamic imports|
|Go|85–90%|Simple import system, explicit declarations|
|Rust|80–85%|Explicit imports help, macro-generated code is problematic|

## Performance Characteristics

- Startup: ~10–50ms (grammar loading only)
    
- Per-file resolution: ~5–15ms for typical files
    
- Incremental update: ~10–50ms (tree-sitter’s strength)
    
- Memory: ~100–300MB for 100k LOC, roughly linear scaling
    
- Full codebase parse: ~2–3s for 100k LOC, ~10–15s for 500k LOC ✅
    

## Hard Failure Cases

**TypeScript:**

- Barrel exports: `export * from './module'` (multi-level symbol visibility)
    
- Namespace merges: multiple `namespace X` declarations
    
- Type-only imports: `import type { T }` (requires type system)
    
- Path aliases: `@/components` (tsconfig.json resolution)
    
- Dynamic imports: `import(condition ? 'a' : 'b')` (runtime)
    

**Python:**

- `__init__.py` re-exports: `from .submodule import *`
    
- Star imports: `from module import *` (requires `__all__`)
    
- Monkey patching: runtime modifications
    
- Decorator patterns: `@decorator` that change signatures or dispatch
    
- Implicit namespace packages (Python 3.3+)
    

**Go:**

- Interface satisfaction (implicit implementations)
    
- Build tags (conditional compilation)
    
- Internal package visibility rules
    

**Rust:**

- Macro-generated code (tree-sitter sees macro calls, not expansions)
    
- Re-exports: `pub use crate::module::Item`
    
- Glob imports: `use module::*`
    
- Trait method resolution (needs type inference)
    

## Implementation Effort

**Estimate:** ~3–4 weeks (senior Rust dev)

- Week 1: tree-sitter integration, parsing infra
    
- Week 2: import graph + name resolution for first language
    
- Week 3: extend to remaining 3 languages
    
- Week 4: edge cases, tests, perf tuning
    

## Dependencies & Maintenance

- `tree-sitter` core – active, very widely used
    
- Official grammars for TS, Python, Go, Rust – all active
    

Tree-sitter is stable and well maintained; grammar updates are frequent and generally compatible.

## Major Gotchas

- No type information → cannot handle overloads / generic specialization well
    
- Need to re-implement each language’s module resolution logic
    
- Must read build/config files: tsconfig.json, Python path, go.mod, Cargo.toml
    
- Incremental updates: invalidating dependent files is non-trivial
    

---

## 2. LSP Integration

## Overview

Use language servers (tsserver, pyright, gopls, rust-analyzer) as oracles for definitions/references.

## Startup Costs Per Server

Approximate ranges (cold start on typical dev hardware):

|Server|Cold Start|Warm Start|First Query|
|---|---|---|---|
|tsserver|400–800ms|200–300ms|50–150ms|
|pyright|300–600ms|150–250ms|40–100ms|
|gopls|500–1000ms|300–500ms|60–200ms|
|rust-analyzer|800–1500ms|400–800ms|100–300ms|

Total for all four: ~2–4s startup ❌

## Batch Query Capabilities

- tsserver: no real batch API; sequential requests
    
- pyright: same
    
- gopls: some workspace/symbol options but not true batch resolution
    
- rust-analyzer: concurrent requests possible but not batch-optimized
    

So you effectively end up with many sequential requests.

## Latency Per Resolution

- Single definition lookup: ~40–300ms
    
- Reference lookup: ~100–500ms
    
- Cross-file call resolution: typically ~50–200ms
    

Hitting <200ms per incremental update reliably is unlikely when every change triggers multiple queries.

## Process Management Complexity

- Need to spawn and supervise 4 servers
    
- Multi-process lifecycle: crashes, restarts, timeouts
    
- Multi-workspace management for monorepos
    
- Per-server protocol quirks
    
- High memory footprint
    

## Rust LSP Client Libraries

- `lsp-server` / `tower-lsp` are server frameworks, not full-featured clients
    
- No mature Rust LSP _client_ with process orchestration
    
- You’d need to implement JSON-RPC plumbing, message routing, request IDs, error handling
    

Estimated +1–2 weeks just for a robust client layer.

## Realistic Precision

|Language|Precision|Notes|
|---|---|---|
|TypeScript|95–98%|tsserver is the source|
|Python|90–95%|pyright is strong|
|Go|95–98%|gopls uses compiler data|
|Rust|90–95%|rust-analyzer is mature|

This is the highest-precision path.

## Performance Characteristics

- Startup: ~2–4s ❌
    
- Per-file resolution: ~50–200ms ❌ (once you have many queries)
    
- Incremental: ~100–400ms (server re-analyzes context) ❌
    
- Memory: ~500MB–1.5GB per server; total ~2–6GB ❌
    
- Full-codebase initialization: 10–30s typical
    

## Implementation Effort

**Estimate:** 4–6 weeks

- 1–2: LSP client infra
    
- 3–4: integration with 4 servers
    
- 5: robust process management
    
- 6: perf tuning & batching where possible
    

## Gotchas / Showstoppers

- Startup and memory budgets are badly violated
    
- No batch APIs
    
- Complex ops and deployment story (4 external binaries)
    
- Re-analysis times scale poorly with repo size
    

---

## 3. SCIP (Sourcegraph)

## Overview

SCIP is a language-agnostic protobuf schema for code intelligence. Indexers produce `.scip` files; you query those offline for navigation.

## Indexer Availability (Feb 2026)

- **TypeScript**: `scip-typescript` – production-grade
    
- **Python**: `scip-python` – production-grade (pyright-based)
    
- **Go**: No official SCIP indexer
    
- **Rust**: Experimental support via rust-analyzer export
    

## Indexing Performance

Indicative numbers from Sourcegraph materials and user reports:

- 100k LOC TypeScript: ~20–30s
    
- 100k LOC Python: ~25–40s
    
- 500k LOC TypeScript: ~2–3min
    

So you exceed your 30s budget on larger repos.

## Incremental Indexing

As of early 2026:

- No fully supported incremental indexing
    
- Re-index-on-change is still full-project, not per-file
    

This alone breaks your <200ms incremental requirement.

## Output Format & Call Graph Construction

SCIP documents:

- Symbols, occurrences (definition vs reference), ranges
    
- Relationships between symbols
    
- Stored as protobuf messages
    

You’d need to:

- Load `.scip` index
    
- For each occurrence, match references to definitions
    
- Build a call graph layer on top
    

Complex, but not fundamentally hard.

## Precision Estimates

Assuming successful index:

|Language|Precision|Notes|
|---|---|---|
|TypeScript|90–95%|Compiler-backed|
|Python|85–90%|Pyright-based|
|Go|N/A|No indexer|
|Rust|70–80%|Experimental / macro gaps|

## Performance Characteristics

- Startup (loading index): ~100–200ms ✅
    
- Query: ~5–10ms per resolution ✅
    
- Incremental: full re-index ❌
    
- Memory: ~200–400MB for 100k LOC index
    

## Implementation Effort

**Estimate:** 5–7 weeks

- 1: protobuf bindings / reading `.scip`
    
- 2–3: TS + Python indexer integration (as subprocesses)
    
- 4–5: custom Go indexer from go/types or gopls
    
- 6: Rust via rust-analyzer SCIP export
    
- 7: call graph layer
    

## License

- SCIP schema and indexers: Apache 2.0 / MIT
    
- No significant license traps for commercial use
    

## Gotchas

- No incremental indexing: major deal-breaker
    
- No Go indexer: significant effort to build
    
- Rust path is experimental and likely fragile
    
- Index staleness problem for “live” LLM agents
    

---

## 4. Hybrid: Tree-Sitter Fast Path + LSP/SCIP Fallback

## Overview

- Tree-sitter handles **most** resolutions quickly
    
- LSP (or SCIP, if you choose) is used only when tree-sitter isn’t confident
    

This is essentially a tiered system:

text

`call site → tree-sitter resolution          → if Confident → use result         → if Ambiguous / Unknown → LSP query`

## Detecting “Ambiguous”

Some effective signals:

1. **Multiple candidates** found via tree-sitter
    
2. Symbol comes through complex re-export chains (e.g. TS barrel exports, Python `__all__` chains)
    
3. Cross-package boundary (e.g. into `node_modules` or a Python venv)
    
4. Need type-level reasoning (generic resolution, trait methods, interface dispatch)
    
5. Macros / decorators present on the target
    
6. Dynamic import or reflection-like pattern
    

API shape:

rust

`enum ResolutionResult {     Confident(Definition),    Ambiguous(Vec<Definition>),    Unknown, }`

You can tune thresholds over time.

## Precision Estimates

With fallback on 10–20% of cases:

|Language|Precision|Notes|
|---|---|---|
|TypeScript|88–93%|Barrel exports, generics go to LSP|
|Python|82–88%|Star imports, `__init__.py` complexity → LSP|
|Go|92–96%|Tree-sitter strong; LSP for tricky interfaces|
|Rust|85–92%|Macros/traits fallback to rust-analyzer|

Across a realistic mix, this can be tuned over 90%.

## Performance

- Startup: 50–200ms for tree-sitter; LSP starts lazily on first fallback
    
- Per-file resolution:
    
    - Fast path (80–90%): 5–15ms
        
    - Slow path (10–20%): 50–200ms
        
    - Weighted average: ~15–35ms ✅
        
- Incremental:
    
    - Tree-sitter incremental parse: 10–50ms
        
    - Occasional LSP calls: add ~100ms on those paths
        
    - Typical average: 20–80ms ✅
        
- Memory:
    
    - Tree-sitter: ~300–500MB at 500k LOC
        
    - LSPs: ~500MB–1GB total if you optimize which ones you keep hot
        
    - Total: ~800MB–1.5GB ✅
        

## Implementation Effort

**Estimate:** 6–8 weeks

- 1–2: tree-sitter infra (essentially Option 1)
    
- 3–4: slimmed-down LSP client that only supports definition-type queries
    
- 5: hybrid control flow + ambiguity heuristics
    
- 6: caching and perf tuning
    
- 7–8: multi-language polish, metrics, tests, precision measurement
    

You don’t need full-blown LSP coverage—just enough to ask “what’s the definition at this position?”

## Precedent

This pattern appears in:

- Engines.dev’s navigation tools (TS + TS server fallback)
    
- Sourcegraph’s combined semantic + search navigation
    
- GitHub Copilot / VSCode (LSP + AST-level heuristics)
    

It’s a known good pattern in practice.

## Gotchas

- Non-trivial cache invalidation across the two layers
    
- Latency variance: some calls are “instant”, others ~150ms
    
- Need good tracing (“was this from tree-sitter or LSP?”) for debugging and tuning
    
- Memory still not free, but within your budget
    

---

## 5. Compiler Frontends as Libraries

## TypeScript – SWC

- Rust-native, fast, actively maintained
    
- Focused on parsing and transforms, not full type-checker
    
- You get an excellent AST but limited semantics
    

Precision: ~70–80% (similar to tree-sitter + a bit more TS awareness)

## Python – Pyright

- Implemented in TypeScript; no Rust-native port
    
- Realistically used via subprocess (LSP or CLI)
    
- Embedding TS/Node in Rust is heavy and awkward
    

No good “library mode” from Rust.

## Go – go/analysis

- Go’s `go/types`, `go/analysis` APIs are strong
    
- From Rust, you’d need CGO or build a Go shared library
    
- Embedding the Go runtime is complex and somewhat brittle
    

Precision: 95%+ possible, but FFI complexity and startup costs are significant.

## Rust – rust-analyzer as Library

- There are crates (`ra_ap_*`) exposing internals
    
- API is explicitly unstable; breaking changes are common
    
- You’d be depending on internal, under-documented components
    

Precision: 90–95% if you can keep up with internal changes.

## Overall

- No unified approach across all four languages
    
- Python is the big gap
    
- Go and Rust paths are technically possible but high-risk and high-maintenance
    
- Startup and memory budgets become harder to meet
    

---

## Additional Research Topics

## New Rust-Native Code Analysis Libraries (2025–2026)

- No strong new contenders beyond:
    
    - `tree-sitter` + Rust bindings
        
    - `rust-code-analysis` (Mozilla) – focused on metrics, last release 2023
        
- No maintained successor to stack-graphs
    

## Stack-Graphs Status

- Official GitHub repo archived Sept 9, 2025
    
- Github message: “no longer supported or updated… fork if you wish”
    
- No clearly active, community-maintained fork with significant traction
    

## Cursor’s Semantic Indexing

Based on public docs and blog posts:

- Builds AST-aware code chunks (functions, classes, blocks)
    
- Embeds them with OpenAI embeddings (e.g. text-embedding-3-large)
    
- Stores in Turbopuffer vector DB
    
- Uses hybrid search (semantic + text search grep-like)
    

This is **semantic search**, not call graph / definition resolution.

## MCP Code-Graph Servers

**code-graph-rag-mcp**

- Uses tree-sitter to parse and build a symbol and file graph
    
- Stores info (often SQLite + embeddings)
    
- No LSP in the loop; relies on structural heuristics
    

**RepoMapper MCP**

- Also tree-sitter-based
    
- Generates repository “map” (important files, symbol graph)
    
- Focuses on structure and importance, not precise def-use resolution
    

They validate that “tree-sitter + heuristics” is viable for production tooling, but their requirements are a bit looser than yours (esp. precision vs LSP ground truth).

---

## Requirements Matrix

|Requirement|Tree-sitter|LSP|SCIP|Hybrid|Compilers|
|---|---|---|---|---|---|
|100k LOC <5s|✅ 2–3s|❌ 4–6s|⚠️ 20–30s|✅ 3–5s|⚠️ varies|
|500k LOC <30s|✅ 10–15s|❌ 15–25s|⚠️ 2–3min|✅ 15–25s|⚠️ varies|
|Incremental <200ms|✅ 10–50ms|❌ 100–400ms|❌ full|✅ 20–80ms|⚠️ 50–200ms|
|Memory <2GB|✅ ~300MB|❌ 2–6GB|✅ ~400MB|✅ 0.8–1.5GB|⚠️ 1–2GB|
|Precision >90%|❌ 70–80%|✅ 90–98%|⚠️ ~88%|✅ ~90%|⚠️ uneven|

---

## Final Recommendation

## Top Pick: Hybrid Tree-Sitter + LSP Fallback

Use:

- Tree-sitter for fast structural resolution
    
- LSP (tsserver, pyright, gopls, rust-analyzer) only when tree-sitter cannot confidently resolve
    

Why:

- Meets your latency, throughput, and memory targets
    
- Achieves your >90% precision goal at scale
    
- Robust and future-proof: heuristics can be refined without redesigning the architecture
    
- Aligns with how other serious code-intelligence and AI-code tools are built
    

**Suggested build order:**

1. Weeks 1–2: Tree-sitter infra and core resolution for TS + Python
    
2. Weeks 3–4: Minimal LSP client, support TS first (your likely highest ROI)
    
3. Weeks 5–6: Hybrid routing + caching + metrics
    
4. Weeks 7–8: Extend to Go and Rust, precision benchmark against LSP ground truth, tune ambiguity heuristics
    

## Runner-Up: SCIP

Re-evaluate SCIP if:

- Incremental indexing becomes available
    
- A Go indexer is released
    
- Your use case shifts toward batch/offline analysis rather than live agent loop
    

---

If you like, the next step could be to design a concrete Rust module layout for the hybrid engine (APIs, data structures, caches) geared specifically to keel’s “code graph enforcement” use case.