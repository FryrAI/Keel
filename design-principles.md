# Keel Design Principles — One-Shot Implementation Philosophy

```yaml
tags: [keel, design-principles, agent-swarm, implementation]
status: governing
purpose: "Why document — every agent reads this before touching code"
```

> **What this document is**: The governing philosophy for implementing keel via 3 Claude Code agents + 1 observer in autonomous ralph loops. These principles define what makes one-shot implementation work, what breaks it, and the non-negotiable rules for this build.
>
> **What this document is NOT**: A design document. All technical design decisions are in [[PRD_1|PRD v2.1]] and extracted into self-contained specs in [[keel-speckit/]]. This document is about *how we build*, not *what we build*.

---

## Principle 1: The Verifier Is King

> *"Claude will solve whatever problem you give it. So it's important that the task verifier is nearly perfect, otherwise Claude will solve the wrong problem."* — Anthropic C compiler team

This is the single most important lesson from Anthropic's C compiler project (16 agents, 2,000 sessions). The quality of autonomous output is bounded by the quality of the test harness, not the quality of the model. If the verifier is wrong, the agent optimizes for the wrong target. If the verifier is incomplete, the agent finds shortcuts that pass tests but don't work.

**For keel, the verifier has 4 oracles:**

1. **LSP ground truth** (graph correctness) — Parse test corpus repos with keel AND with the language's native LSP. Compare call edges. Precision = edges keel found that LSP confirms / total edges keel found. Recall = edges LSP found that keel also found / total edges LSP found. Target: >85% precision per language, >80% recall.

2. **Mutation testing** (enforcement correctness) — Automatically introduce known-breaking changes to test corpus code (rename parameter, change return type, remove function, change arity, add function without type hints). Verify keel catches each mutation. True positive rate target: >95%. False positive rate target: <5%.

3. **Performance benchmarks** (hard numeric targets) — `init` <10s for 50k LOC. `compile` <200ms for single-file change. `discover` <50ms. `explain` <50ms. Clean compile = empty stdout + exit 0. These are not aspirational — they are pass/fail gates.

4. **JSON schema validation** (output contracts) — Every `--json` output must validate against the schemas defined in [[keel-speckit/008-output-formats/spec|Spec 008]]. Schema violations = test failure. No exceptions.

**Phase 0 creates ALL test infrastructure before a single line of product code.** Test files are pre-written with `#[ignore]` annotations. Agents un-ignore tests as they implement features. Progress = passing tests / total tests.

---

## Principle 2: Contracts Before Code

Inter-agent interfaces must be defined and frozen in Phase 0. Without frozen contracts, Agent A returns `FunctionNode { hash: String }` while Agent B expects `GraphNode { content_hash: Vec<u8> }` — and nobody catches it until integration.

**4 critical contracts (frozen in Phase 0):**

1. **`LanguageResolver` trait** (Agent A exposes to Agent B)
   ```
   trait LanguageResolver {
       fn resolve_definitions(file: &Path) -> Vec<Definition>;
       fn resolve_references(file: &Path) -> Vec<Reference>;
       fn resolve_call_edge(call_site: &CallSite) -> Option<ResolvedEdge>;
   }
   ```

2. **`GraphStore` trait** (Agent A exposes to Agents B and C)
   ```
   trait GraphStore {
       fn get_node(hash: &str) -> Option<GraphNode>;
       fn get_edges(node_id: u64, direction: EdgeDirection) -> Vec<GraphEdge>;
       fn get_module_profile(module_id: u64) -> Option<ModuleProfile>;
       fn update_nodes(changes: Vec<NodeChange>) -> Result<()>;
   }
   ```

3. **Result structs** (Agent B exposes to Agent C)
   ```
   struct CompileResult { errors: Vec<Violation>, warnings: Vec<Violation>, info: CompileInfo }
   struct DiscoverResult { target: NodeInfo, upstream: Vec<NodeInfo>, downstream: Vec<NodeInfo>, module_context: ModuleContext }
   struct ExplainResult { error_code: String, hash: String, confidence: f64, resolution_tier: String, chain: Vec<ResolutionStep> }
   ```

4. **JSON output schemas** (Agent B/C expose to external consumers)
   - Compile error JSON schema (PRD 12)
   - Discover JSON schema (PRD 12)
   - Map JSON schema (PRD 12)
   - Explain JSON schema (PRD 12)

**Contract tests run on EVERY cycle.** Failing contract test = immediate stop. The agent must fix the contract violation before continuing any other work. This is the same pattern as KolBaer's `packages/shared-types/` — but for Rust traits and structs instead of TypeScript types.

---

## Principle 3: Decompose by Dependency DAG, Not Feature

KolBaer decomposed by technology layer (frontend / backend / engine). That worked because web app layers have natural isolation — the frontend doesn't import backend code.

**Keel's decomposition follows the dependency chain:**

```
Foundation (parsing + graph)
    ↓
Enforcement (validation + commands)
    ↓
Surface (integration + distribution)
```

Each layer can start against mocks/fixtures, then integrate with the real layer below at phase gates. This means:

- **Agent A (Foundation)** builds the parser and graph. Everything else depends on this.
- **Agent B (Enforcement)** builds validation logic and CLI commands. Depends on Agent A's graph, but can start with mock graph fixtures.
- **Agent C (Surface)** builds tool integration, MCP/HTTP server, VS Code extension, distribution. Depends on Agent B's command output, but can start with mock compile output.

**Why not feature decomposition?** A "compile command" feature touches parsing (Agent A), validation logic (Agent B), CLI output (Agent B), JSON formatting (Agent C), and hook integration (Agent C). Feature decomposition creates cross-agent dependencies on every task. Dependency-chain decomposition creates clean layer boundaries with mock-able interfaces.

---

## Principle 4: The Resolution Engine Is the Long Pole — Accept It

4 languages x different enhancers (Oxc, ty subprocess, tree-sitter heuristics, rust-analyzer lazy-load) = highest complexity in the system. This is compiler-adjacent work.

**The numbers from PRD 18:**
- M1 (core parser + resolution engine): 10-14 days estimated
- 4 languages, each with a different Tier 2 enhancer
- Precision target: >85% cross-file resolution per language
- Three independent research sources agree: 8-10 weeks for the engine alone

**Accept this reality. Don't try to shortcut it.**

- Gate M1 on resolution precision: >85% per language measured against LSP ground truth. Do not proceed to enforcement until this gate passes.
- Per-language resolvers parallelize *within* Agent A — TypeScript (Oxc), Python (ty), Go (heuristics), and Rust (rust-analyzer) can be developed and tested independently.
- Tree-sitter Tier 1 is the universal fast path and should be built first. Tier 2 enhancers build on top of Tier 1 results.
- Go is simplest (tree-sitter heuristics alone achieve ~85-92%). TypeScript next (Oxc is production-ready). Python depends on ty stability. Rust is hardest (rust-analyzer's 60s+ startup requires lazy-loading).

---

## Principle 5: Progressive Gates, Not Waterfall

Phase gates with hard quality criteria prevent premature advancement. The observer/orchestrator enforces gates — it does NOT create next-phase tasks until the gate passes.

**Gate M1: Resolution**
- Resolution precision >85% per language (measured against LSP ground truth on test corpus)
- All 4 languages parse without panic on test corpus repos
- Graph schema populated correctly (node count, edge count within 10% of LSP baseline)
- Performance: `init` <10s for 50k LOC test repo

**Gate M2: Enforcement**
- All CLI commands functional (`init`, `map`, `discover`, `compile`, `where`, `explain`)
- Enforcement catches all mutation test cases (>95% true positive rate)
- Circuit breaker escalation works (3-attempt sequence verified)
- Clean compile produces empty stdout + exit 0
- `compile` <200ms for single-file change
- JSON output validates against schema

**Gate M3: Integration**
- End-to-end with Claude Code on test repo: hooks fire, errors shown, LLM fixes
- End-to-end with Cursor on test repo: hooks fire (including v2.0 workaround)
- `keel init` generates correct configs for all detected tools
- MCP server responds to all 5 tool calls
- VS Code extension shows status bar, inline diagnostics, CodeLens
- Cross-platform: builds on Linux x86_64, macOS arm64, Windows x86_64

**Gate criteria are binary — pass or fail.** Partial credit doesn't exist. If resolution precision is 84% on TypeScript, the gate fails. Fix it before moving on.

---

## Principle 6: Error Fingerprinting and Escalation

Without escalation, agents enter retry loops — same fix, same failure, same fix, forever. The error fingerprint system converts loops into progressive strategies.

**Escalation tiers:**

| Consecutive Failures | Action |
|---------------------|--------|
| 5 | Inject hint into agent prompt: "This error has occurred 5 times. Try a different approach. Consider: [specific suggestion based on error pattern]" |
| 8 | Force-skip task, reassign to different teammate. Log: "Task force-skipped after 8 attempts. Error fingerprint: [hash]. Needs human review or different agent." |
| 15 | 30-minute cooldown for that error fingerprint. Human review flag raised via orchestrator escalation. |

**Error fingerprint = hash of (test name + error message pattern + file path).** This groups identical failures while allowing different manifestations of the same root cause to escalate together.

**Reset on new error:** If the agent's fix changes the error (different test fails, different error message), the counter resets. Only *identical* consecutive failures escalate.

**Native implementation via `TeammateIdle` hooks:** Within each agent team, `TeammateIdle` notifications alert the team lead when a teammate stops making progress. The lead tracks failure counts per teammate and applies the escalation tiers above. At the 15-repeat threshold, the lead flags the orchestrator via commit message convention (`ESCALATE: [description]`), and the orchestrator triggers human review. This replaces custom error fingerprinting scripts with built-in agent team primitives.

---

## Principle 7: The Observer Sees Everything, Writes Nothing

Observation and coordination are separated from implementation at two layers:

1. **Team leads** (delegate mode) observe and coordinate within their team — they can't edit code, only manage tasks, review plans, and message teammates. This prevents the coordinator from introducing its own bugs within each team.
2. **The orchestrator** observes and coordinates across all 3 teams — it uses `/tmux-observe` to read team output and git to check test results across worktrees. It enforces phase gates and manages cross-team integration.

This is a **2-layer observer pattern**: orchestrator observes teams, leads observe teammates.

**Orchestrator responsibilities:**
- Monitor test pass rates across 3 worktrees via git and `/tmux-observe`
- Enforce phase gates (write gate marker files only after criteria met)
- Merge branches at gate transitions (Foundation -> Enforcement at M1, etc.)
- Detect cross-team patterns (same error in multiple teams = systemic issue)
- Maintain `swarm-status.md` dashboard (updated every cycle via `/ralph-loop`)
- Trigger human intervention when escalation fires (15-repeat threshold)

**Team lead responsibilities (delegate mode):**
- Manage task list within their team (`TaskCreate`/`TaskUpdate`/`TaskList`)
- Review teammate plans before implementation (plan approval)
- Track teammate progress via `TeammateIdle` and `TaskCompleted` notifications
- Apply escalation tiers (5=hint, 8=reassign, 15=flag orchestrator)
- Redistribute work when teammates are blocked or complete early

**Neither orchestrator nor leads:**
- Write Rust code
- Modify `Cargo.toml`
- Edit test files
- Make architectural decisions

The observer pattern at both layers ensures coordination agents never introduce their own bugs, while keeping each agent's context window focused on its role — coordination or implementation, never both.

---

## Principle 8: Mock Everything at Boundaries

Agents must never block on another agent's output. Every inter-agent dependency has a mock that enables parallel development.

**Mock strategy:**

| Consumer | Dependency | Mock |
|----------|-----------|------|
| Agent B (enforcement) | Agent A's real graph | Mock graph fixtures: pre-built `GraphStore` with known nodes, edges, modules from test corpus |
| Agent C (tool integration) | Agent B's compile output | Mock `CompileResult` structs with known errors, warnings, fix hints |
| Agent C (VS Code extension) | `keel serve --http` | Mock HTTP server returning canned responses for all endpoints |
| Agent C (MCP server) | Core library | Mock `KeelCore` trait returning pre-defined results |
| All agents (test harness) | Test corpus repos | Pre-cloned, cached, pinned to specific commits |

**Real integration happens only at phase gates.** Between gates, agents develop against mocks. At the gate, mocks are replaced with real implementations and integration tests run.

---

## Principle 9: Self-Contained Specs — Agents Read Their Spec, Not the PRD

Each spec in [[keel-speckit/]] fully restates ALL relevant PRD content. Agents never need to read [[PRD_1|PRD v2.1]] — their spec file IS their complete reference.

**What "self-contained" means:**
- Rust struct/enum definitions copied in full from PRD 11
- JSON output schemas copied in full from PRD 12
- Error codes and severity levels copied from PRD 12
- Configuration format sections copied from PRD 13
- Performance targets copied from PRD 17-18
- API contracts, CLI behavior, output format examples — all inline
- PRD section numbers cited for traceability, but the content is extracted in full

**Why this costs more upfront but pays off:**
- No context-switching: agent reads one file, implements everything in it
- No ambiguity: every struct field, every JSON property, every error code is right there
- No stale references: if the spec says `confidence: f64`, that's what gets implemented — not a paraphrase of "there's a confidence field somewhere in section 12"
- Faster iteration: agent doesn't burn tokens loading a 2000+ line PRD to find the one section it needs

**Traceability:** Every spec cites `PRD section numbers` (e.g., "Extracted from PRD 5, 10.3, 11, 23") so humans can verify the extraction is correct and complete.

---

## Principle 10: One Binary, Zero Runtime Dependencies

keel is a single Rust binary with no runtime dependencies. This is a hard constraint, not a preference.

**What "zero runtime dependencies" means:**
- tree-sitter grammars for all 4 languages compiled into the binary
- SQLite statically linked via `rusqlite` with `bundled` feature
- No FFI calls in the hot path
- No Node.js, Python, or Go runtime required to run keel
- `ty` is the one exception: subprocess call for Python Tier 2 enhancement (optional, degrades gracefully if `ty` is not installed)
- `rust-analyzer` is lazy-loaded via `ra_ap_ide` crates (Rust library, not subprocess)

**Cross-platform from day 1:**
- Linux x86_64 and arm64
- macOS arm64 and x86_64
- Windows x86_64
- Platform-native path handling internally, forward slashes in all output
- Respects `NO_COLOR` and `TERM=dumb` for terminal output

**Binary size budget:** 20-35MB expected (4 language grammars + Oxc + SQLite + resolution engine). Acceptable for developer tooling. Investigate LTO + stripping if >40MB.

**Install methods:** `curl -fsSL https://keel.engineer/install.sh | sh`, `brew install keel`, `cargo install keel`, `winget install keel`, `scoop install keel`.

---

## How These Principles Interact

The principles form a reinforcing system:

1. **Verifier Is King** (P1) demands that test infrastructure is built first
2. **Contracts Before Code** (P2) freezes the interfaces those tests verify
3. **Dependency DAG Decomposition** (P3) determines which agent owns which contract
4. **Resolution Is the Long Pole** (P4) sets expectations for the hardest subsystem
5. **Progressive Gates** (P5) prevents advancing until quality criteria are met
6. **Error Fingerprinting** (P6) prevents agents from spinning on the same failure
7. **Observer Sees Everything** (P7) enforces gates and coordinates without writing code
8. **Mock Everything** (P8) enables parallel work despite sequential dependencies
9. **Self-Contained Specs** (P9) gives each agent everything it needs in one file
10. **One Binary** (P10) constrains the technical decisions throughout

**If any principle is violated, the build degrades predictably:**
- Skip P1 (verifier) → agents optimize for wrong targets, output looks correct but isn't
- Skip P2 (contracts) → integration fails at phase gates, 2-3 days of rework
- Skip P3 (decomposition) → cross-agent dependencies create blocking chains
- Skip P4 (long pole) → premature advancement to enforcement with <85% resolution → false positive cascade
- Skip P5 (gates) → bad code propagates, downstream agents build on broken foundation
- Skip P6 (fingerprinting) → agents burn budget on retry loops
- Skip P7 (observer) → coordination agent introduces its own bugs
- Skip P8 (mocks) → agents block on each other, parallelism collapses
- Skip P9 (self-contained) → agents burn context window loading the full PRD
- Skip P10 (one binary) → distribution complexity explodes, cross-platform breaks

---

## Related Documents

- [[PRD_1|PRD v2.1]] — The master source document (2000+ lines)
- [[constitution|Constitution]] — Non-negotiable articles extracted from PRD
- [[agent-swarm|Agent Swarm Playbook]] — Runnable playbook for 3 agents + observer
- [[CLAUDE|CLAUDE.md]] — Agent implementation guide
- [[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]] — Bedrock data structures
- [[keel-speckit/test-harness/strategy|Test Harness Strategy]] — Oracle definitions and corpus
