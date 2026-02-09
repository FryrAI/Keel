# Strategic Analysis of Cross-File Reference Resolution Engines for Keel: Architectural Recommendations for 2026

**Date:** February 8, 2026

**To:** Keel Engineering Leadership

**Subject:** Comprehensive Evaluation of Symbol Resolution Architectures for Automated Code Enforcement

## 1. Executive Summary

The mandate for **Keel** is to establish a deterministic enforcement layer for Large Language Model (LLM) coding agents, ensuring that generated code adheres to the strict structural and semantic boundaries of an existing repository. As of February 2026, the static analysis landscape has shifted dramatically following the archival of GitHub's `stack-graphs` framework in late 2025. This event has forced a departure from unified, declarative definitions of scope toward a heterogeneous ecosystem of high-performance, language-specific tools written in systems languages like Rust and Go.

The primary engineering challenge for Keel lies in resolving cross-file references with high precision (>90%) while operating within a constrained resource envelope (2GB RAM for 500k LOC) and delivering sub-second incremental updates. The analysis indicates that no single "silver bullet" library exists to replace `stack-graphs` across TypeScript, Python, Go, and Rust. Instead, the optimal solution requires orchestrating a hybrid architecture that leverages the explosion of Rust-native compiler frontends—specifically the **Oxc** toolchain for JavaScript/TypeScript and **Ty (formerly Red Knot)** for Python—while utilizing the **Simple Code Intelligence Protocol (SCIP)** as a data interchange format for languages where native embedding is prohibitive, particularly Go and complex Rust workspaces.

This report evaluates five distinct architectural approaches. The analysis confirms that a **Pure Tree-sitter** implementation relies too heavily on heuristics, failing the precision requirements for enforcement. Conversely, a full **LSP Integration** strategy, while precise, introduces unacceptable latency and memory overhead, threatening the stability of the enforcement layer on standard developer hardware. The recommended **Hybrid "Compiler-Native" Strategy** balances these competing concerns by statically linking high-performance semantic libraries where possible and ingesting pre-computed indices where necessary. This approach aligns with the internal mechanics of industry-leading tools like Cursor and RepoMapper, offering a path to "ground truth" enforcement without the bloat of a full Integrated Development Environment (IDE).

---

## 2. The Post-Stack-Graphs Ecosystem (2025–2026)

### 2.1 The Archival of Stack-Graphs and the "Zero-Config" Vacuum

For years, GitHub’s `stack-graphs` promised a unified theory of name binding: a declarative, graph-based formalism that could resolve symbols across files for any language without requiring a build step. Its archival in September 2025 marked the end of the "universal engine" era. The project failed not due to a lack of utility, but because of the immense complexity involved in modeling the idiosyncrasies of modern languages—such as TypeScript’s conditional types or Python’s dynamic MRO—within a graph DSL. Maintaining these rulesets became an unsustainable burden that outpaced the evolution of the languages themselves.

While a fork named `metaslang_stack_graphs` exists, maintained by the Nomic Foundation , it has been specialized for the Solidity ecosystem. Adopting this fork for Keel would necessitate that your engineering team assume the primary maintenance burden for TypeScript, Python, Go, and Rust rulesets—an effort equivalent to writing four separate compiler frontends. Consequently, relying on `stack-graphs` or its derivatives represents a significant technical debt risk.

### 2.2 The Rise of Rust-Native Semantic Tooling

The vacuum left by `stack-graphs` has been filled by a new generation of language-specific tools written in Rust, driven by the need for extreme performance in web tooling and CI pipelines.

- **JavaScript/TypeScript:** The **Oxc (Oxidation Compiler)** project has matured into a production-grade toolchain. Its resolver, `oxc_resolver`, is a Rust port of Webpack’s `enhanced-resolve` but operates at 30x the speed, offering precise module resolution compatible with the complex `package.json` exports and `tsconfig` paths found in modern monorepos.
    
- **Python:** Astral, the team behind the `ruff` linter, released **Ty (formerly Red Knot)** in late 2025. Unlike `ruff`, which focused on syntax, `ty` is a full static type checker and semantic analyzer built on the **Salsa** incremental computation framework. It provides deep cross-file understanding with performance metrics 10-60x faster than traditional tools like `mypy` or `pyright`.
    
- **TypeScript 7 (Project Corsa):** Microsoft’s rewrite of the TypeScript compiler in **Go** (not Rust) creates a divergence in the ecosystem. While this improves `tsc` performance, it complicates embedding the official compiler into a Rust application like Keel, reinforcing the value of Oxc as a Rust-native alternative for analysis.
    

This shift from "universal graphs" to "fast native libraries" defines the architectural constraints for Keel. The goal is no longer to find one library to rule them all, but to efficiently orchestrate the best-in-class native libraries for each target language.

---

## 3. Comparative Evaluation of Resolution Architectures

The selection of a resolution engine dictates the fundamental performance and reliability characteristics of Keel. The following sections analyze five distinct approaches against the stated requirements of >90% precision, <200ms latency, and strict memory limits.

### 3.1 Approach 1: Pure Tree-sitter + Heuristics

This architecture relies exclusively on the **Tree-sitter** parsing library to generate Concrete Syntax Trees (CSTs) and uses custom S-expression queries to extract symbol information. Cross-file resolution is performed via string matching and file system heuristics (e.g., assuming `import utils` maps to `./utils.ts`).

**Performance Characteristics:** The performance profile of Tree-sitter is exceptional. Because it was designed for real-time syntax highlighting in editors like Atom and Neovim, it handles incremental updates in the microsecond range. A full parse of 100k LOC typically completes in under 5 seconds on modern hardware. Memory usage is minimal, as the system only needs to retain the parse trees for active files, easily staying under 500MB even for large repositories.

**Precision Estimate: 55-65%**

Despite its speed, this approach fails the >90% precision requirement. Tree-sitter is a syntactic parser; it lacks semantic understanding of the code.

- **Ambiguity:** In a statement like `x.y()`, Tree-sitter cannot determine if `y` is a method on a class defined in another file, a property of a localized object, or a dynamic import.
    
- **Star Imports:** Constructs like `from module import *` in Python or `export * from './utils'` in TypeScript blind heuristic resolvers. Without resolving the _target_ module and indexing its exports, the resolver cannot know what symbols are introduced into the current scope.
    
- **Shadowing:** Correctly handling variable shadowing (where a local variable allows a name from an outer scope) requires a reimplementation of lexical scoping logic, which is error-prone and redundant.
    

**Verdict:**

**Unsuitable for Enforcement.** A code graph enforcement layer must function as a source of truth. Relying on heuristics introduces a high rate of false positives (flagging valid code as broken) and false negatives (missing invalid references), which undermines trust in the agent's constraints.

### 3.2 Approach 2: LSP Integration

In this model, Keel acts as a Language Server Protocol (LSP) client, spawning and managing separate processes for `typescript-language-server`, `gopls`, `basedpyright`, and `rust-analyzer`. It queries these servers for definitions and references via JSON-RPC.

**Performance Characteristics:**

- **Startup Latency:** Severe. `rust-analyzer` performs a `cargo check` on startup, which can take minutes for a 500k LOC repository. `gopls` and `tsserver` also have initialization phases that block queries.
    
- **Incremental Updates:** Fast (<50ms) once the server is warm, as the LSPs maintain their own state.
    
- **Memory:** **Critical Failure Point.** LSPs are notoriously memory-hungry because they maintain full semantic graphs, type tables, and caches for the entire workspace. `rust-analyzer` alone has been documented to consume 10GB-20GB of RAM on large monorepos. Running four concurrent LSPs would immediately breach the 2GB constraint.
    

**Precision Estimate: 98-100%**

This approach offers the highest possible precision. The LSPs use the official compiler APIs (or close approximations) and represent the definitive interpretation of the code.

**Implementation Effort:** High. The LSP specification is often treated as a guideline rather than a strict standard. Keel would need to handle process lifecycle management, distinct initialization options per server, and robust error handling for crashed or hung servers (`gopls` and `tsserver` are prone to hanging on large edits). Libraries like `tower-lsp` or `lsp-bridge` can mitigate some boilerplate, but the operational complexity remains high.

**Verdict:**

**Viable Only for Verification, Not Discovery.** Due to the memory footprint and startup latency, this approach cannot be the primary engine for a lightweight agent enforcement layer. It works best as a "oracle" for occasional verification rather than the backbone of the system.

### 3.3 Approach 3: SCIP (Sourcegraph Code Intelligence Protocol)

This architecture decouples analysis from query time. Keel would utilize external indexers (CLIs like `scip-typescript` or `scip-go`) to generate a static index (`index.scip`) of the codebase. This index, based on a standardized Protobuf schema, contains all definitions, references, and documentation.

**Performance Characteristics:**

- **Query Performance:** Instant. Finding a reference is a simple lookup in the loaded index (essentially a hash map).
    
- **Memory:** Highly Efficient. The index can be streamed or partially loaded. A SCIP index for a large repo is typically compact, easily fitting within 2GB.
    
- **Indexing Latency:** Poor. SCIP indexers generally perform a full compilation or analysis pass. Generating a fresh index for a 500k LOC repository takes 30s to several minutes. This violates the <200ms requirement for incremental updates.
    

**Precision Estimate: 95-98%**

SCIP indexers are typically built on top of the official compiler APIs (e.g., `scip-typescript` uses the TS compiler API), ensuring high fidelity. The only precision loss comes from the "staleness" of the index between updates.

**Verdict:**

**Essential for Baseline State.** SCIP provides a unified, highly efficient way to represent the "baseline" state of a repository. While it cannot handle real-time edits from an agent, it is the perfect mechanism for initializing the graph for Go and Rust, where native library embedding is difficult.

### 3.4 Approach 4: Hybrid (Tree-sitter Fast Path + Fallback)

This is the architecture pioneered by **Cursor** and **RepoMapper**. It combines a static baseline (like a vector DB or SCIP index) with a fast, approximate parser (Tree-sitter) for the files currently being edited.

**Mechanism:**

1. **Baseline:** Load the project's dependency graph from a pre-computed index.
    
2. **Dirty Path:** When the agent edits File A, Keel re-parses only File A using Tree-sitter.
    
3. **Reconciliation:** Keel attempts to resolve imports in the dirty File A against the known exports in the static index.
    

**Performance Characteristics:**

- **Incremental Update:** Extremely fast (<50ms). Only the dirty file is re-processed.
    
- **Precision:** Variable (~85-92%). This approach struggles with "ripple effects." If File A changes a type definition that File B depends on, the hybrid model might not detect the break in File B until File B is also re-parsed. However, for _resolving_ where a function call points, it is generally sufficient.
    

**Implementation Effort:**

Very High. This requires building a custom reconciliation engine that can merge "fuzzy" data from Tree-sitter with "precise" data from the index. You must implement logic to map Tree-sitter nodes to the symbol IDs used in the SCIP index.

**Verdict:**

**The Reference Architecture.** This approach balances performance and precision most effectively for interactive applications. It is the closest match to how modern "AI IDEs" function.

### 3.5 Approach 5: Compiler Frontends as Libraries

This approach involves statically linking the internal crates of Rust-written tools (`oxc`, `ty`, `rust-analyzer` internals) directly into the Keel binary. Instead of spawning processes, Keel calls functions like `resolver.resolve(import_path)` directly.

**Performance Characteristics:**

- **Speed:** Unmatched. In-process calls allow for nanosecond-level interaction. `Ty` can recompute diagnostics for a file in <5ms. `Oxc` parses and resolves modules orders of magnitude faster than Node.js-based tools.
    
- **Memory:** Controllable. Because you control the data structures, you can discard ASTs for files that are not relevant to the current query, keeping memory usage strictly within bounds. You avoid the overhead of the Electron/Node runtime required by `tsserver` or `pyright`.
    

**Precision Estimate: >95%**

- **TypeScript:** `oxc_resolver` is fully compliant with the Node.js resolution algorithm. `oxc_semantic` handles scope correctly.
    
- **Python:** `ty` uses the Salsa framework to perform correct type inference and MRO resolution, far exceeding regex capabilities.
    

**Major Gotchas:**

- **API Stability:** Internal compiler APIs (especially `rust-analyzer`'s `ra_ap_*` crates) are not stable. They change frequently, requiring lock-step updates.
    
- **Language Support:** This works exceptionally well for TS and Python (thanks to Oxc and Ty). However, there is no comparable "Rust-native library" for Go. Go analysis generally requires the `go/analysis` framework, which is written in Go.
    

**Verdict:**

**The Top Recommendation.** This approach maximizes the unique advantage of the 2026 ecosystem: the availability of high-performance Rust libraries for dynamic languages. It solves the precision issues of Tree-sitter without the resource costs of LSP.

---

## 4. Deep Research Findings and Implications

### 4.1 New Rust-Native Code Analysis Libraries (2025-2026)

The maturation of the Rust web ecosystem has produced two critical libraries for Keel:

- **Oxc (The Oxidation Compiler):** Oxc has effectively won the race for high-performance JS/TS tooling. Its `oxc_resolver` crate is a standalone, rigorously tested implementation of module resolution. It supports the myriad complexity of the JS ecosystem (`exports` fields, `alias` fields, `tsconfig` paths) out of the box. Benchmarks show it is 30x faster than the industry standard `enhanced-resolve`. For Keel, this means accurate resolution of `import` statements is a solved problem.
    
- **Ty (Astral):** Formerly Red Knot, `ty` is a type checker built for the `uv` and `ruff` ecosystem. Critically, it uses **Salsa**, the same incremental computation library used by `rust-analyzer`. Salsa allows `ty` to function as a responsive database: Keel can update a single file's text and query for specific symbol information without triggering a full re-check. This "query-based" architecture matches Keel's needs perfectly.
    

### 4.2 Cursor’s Semantic Indexing Internals

Cursor's ability to maintain context over large codebases relies on a sophisticated indexing pipeline that Keel can emulate :

- **Merkle Tree Sync:** Cursor uses a Merkle tree to track file states, allowing it to sync only changed regions to its backend. This minimizes I/O.
    
- **Semantic Chunking:** Instead of splitting code by lines, Cursor uses Tree-sitter to split code by logical boundaries (functions, classes). This ensures that embeddings represent complete semantic units.
    
- **Shadow Workspace:** For verification, Cursor runs a hidden instance of VS Code (the "Shadow Workspace"). When code is generated, it is injected into this shadow instance where the real LSP verifies it. This confirms that while "fuzzy" methods are used for retrieval, "precise" methods are used for enforcement—validating the Hybrid approach.
    

### 4.3 MCP Code-Graph Server Approaches

The Model Context Protocol (MCP) ecosystem offers prior art for graph servers:

- **RepoMapper:** Uses a PageRank algorithm on the call graph to identify "important" code. This is a useful heuristic for prioritization but lacks the rigorous symbol resolution needed for enforcement.
    
- **CodeGraphRAG:** Integrates `tree-sitter` with vector databases (`sqlite-vec`). This highlights the trend toward combining syntactic structure with semantic embeddings.
    

---

## 5. Recommendation: The "Polyglot Compiler" Architecture

Based on the evaluation, we recommend a **Hybrid Compiler-Native Architecture** that statically links Rust-native analyzers for dynamic languages and utilizes SCIP for static languages.

### 5.1 The Architecture

Keel should be built as a single Rust binary containing four distinct resolution strategies orchestrated by a central **Graph Core**.

**1. The TypeScript Engine: Oxc**

- **Integration:** Link `oxc_parser`, `oxc_semantic`, and `oxc_resolver`.
    
- **Workflow:** Use `oxc_resolver` to resolve import paths to files on disk. Use `oxc_semantic` to build a symbol table for each file, resolving identifiers to declarations.
    
- **Why:** It offers compiler-level precision with zero-copy overhead and minimal memory usage compared to `tsserver`.
    

**2. The Python Engine: Ty (via Library)**

- **Integration:** Link the `ty` (Astral) crates.
    
- **Workflow:** Initialize a `ty` database for the project. Feed file updates into the database. Query the database for the definition sites of symbols.
    
- **Why:** It provides the only viable path to incremental, accurate Python resolution without running a Python process.
    

**3. The Static Language Engine: SCIP Ingestion (Go & Rust)**

- **Integration:** Use the `scip` Rust crate to read `index.scip` files.
    
- **Workflow:** On startup, run `scip-go` or `rust-analyzer scip` (as a CLI command) to generate a baseline index. Ingest this index into Keel's in-memory graph.
    
- **Incrementality:** For small edits, use Tree-sitter to parse the _changed_ file and attempt to match symbols against the _baseline_ SCIP index. If a major refactor occurs, trigger a background re-index.
    
- **Why:** Native library support for Go in Rust is non-existent. `rust-analyzer` as a library is too heavy. SCIP provides a compact, accurate bridge.
    

**4. The Graph Core:**

- A `petgraph` or `sqlite` database that stores the unified dependency graph.
    
- **Nodes:** Files, Functions, Classes, Structs.
    
- **Edges:** `Imports`, `Calls`, `Inherits`.
    

### 5.2 Implementation Roadmap (Phase 1)

1. **Dependency Setup:** Add `oxc`, `tree-sitter`, and `scip` crates to `Cargo.toml`.
    
2. **TS Implementation:** Implement the `Resolver` trait using `oxc_resolver`. Verify it passes the `enhanced-resolve` test suite (provided in Oxc).
    
3. **SCIP Ingestion:** Build a fast SCIP deserializer. Test it against `scip-go` output on a standard Go repo (e.g., `kubernetes`).
    
4. **Python Prototype:** Experiment with linking `ty`. If the API is too unstable (it is Beta), fallback to `ruff`'s AST parser combined with a simplified import resolver (handling standard `from x import y` logic) and wait for `ty` stabilization.
    
5. **Benchmarks:** Establish a regression test using a 500k LOC synthetic monorepo (mixed TS/Go). Measure RAM usage; if >2GB, implement LRU eviction for the Oxc/Tree-sitter parse trees.
    

---

## 6. Runner-Up: The "LSP-Bridge" Daemon

If the integration complexity of internal compiler APIs proves too high (particularly for `ty` or unstable `rust-analyzer` crates), the fallback strategy is the **Managed LSP Daemon**.

- **Concept:** Keel runs as a lightweight proxy (like `lsp-bridge` or `multi-lsp-proxy` ).
    
- **Optimization:** Instead of letting LSPs run wild, Keel strictly manages their lifecycle. It starts the LSP, requests a specific resolution, and then _pauses_ or _kills_ the process if memory pressure rises.
    
- **Configuration:** It runs LSPs in "single file mode" or with "workspace loading" disabled where possible to reduce the memory footprint.
    
- **Trade-off:** This sacrifices some latency (process startup/shutdown) for implementation simplicity (standard JSON-RPC) and guarantees of correctness (official LSPs). However, avoiding the memory limit on large repos remains a significant risk with this approach.
    

---

## 7. Conclusions

The failure of `stack-graphs` was a failure of the "universal DSL" model. The industry has corrected course toward high-performance, language-specific tooling written in systems languages.

For Keel in 2026, **Rust is the platform.** By leveraging **Oxc** and **Ty**, Keel can achieve an unprecedented combination of speed and precision for dynamic languages. For static languages, **SCIP** provides the necessary bridge to existing, mature toolchains. This hybrid architecture avoids the "uncanny valley" of heuristics while sidestepping the resource demands of full IDEs, perfectly positioning Keel as a lightweight, reliable enforcement layer for the age of AI coding agents.

### Summary of Key Technologies

|**Technology**|**Role in Keel**|**Status (2026)**|**Performance Note**|
|---|---|---|---|
|**Oxc**|TS/JS Resolution|Production Ready|30x faster than webpack resolver|
|**Ty**|Python Resolution|Beta/Stable|Incremental updates in <5ms|
|**SCIP**|Go/Rust Baseline|Stable Standard|Instant query, slow update|
|**Tree-sitter**|Fast Parse / Fallback|Mature Standard|<10ms incremental parse|
|**Salsa**|Incremental Computation|Core of Ty/RA|Enables on-demand analysis|

This report strongly advises proceeding with the **Hybrid Compiler-Native** architecture. It aligns with the trajectory of the ecosystem and offers the most robust foundation for future scalability.