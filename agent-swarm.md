# Keel Agent Swarm Playbook

```yaml
tags: [keel, implementation, agent-swarm, automation, agent-teams]
status: ready
agents: 15 (1 orchestrator + 3 leads + 11 teammates)
budget_estimate: "Claude Max plan ($200/month) — expect 2-4 months based on Anthropic's C compiler data"
estimated_runtime: "Phase 0: ~12-24 hours (single agent). Phases 1-3: ~2-4 weeks autonomous. Remaining 30-50%: 2-3 weeks human polish."
inspired_by: "Anthropic C Compiler Swarm (16-agent adaptation) + Claude Code Agent Teams"
```

> **What this document is**: A runnable playbook for implementing keel using 15 Claude Code agents organized as 3 nested agent teams + 1 AI orchestrator. Pre-flight checklist, infrastructure setup, test harness, phase-by-phase agent assignments, coordination patterns, and verification checklists.
>
> **What this document is NOT**: A design document. All design decisions are finalized in [[constitution|Constitution]] and 13 specs in [[keel-speckit/]]. This document assumes specs are locked.

---

## 1. Philosophy & Honest Expectations

### Why Agent Swarm Works for Keel

1. **13 self-contained specs with unambiguous GIVEN/WHEN/THEN acceptance criteria** — agents have clear pass/fail signals
2. **Natural three-way split** along the dependency DAG: Foundation (parsing + graph) -> Enforcement (validation + commands) -> Surface (integration + distribution)
3. **Typed contracts everywhere** — Rust traits and structs define interfaces between agents
4. **Resolution engine parallelizes internally** — 4 language resolvers can be developed independently by separate teammates within the Foundation team
5. **~667 pre-written tests** provide continuous feedback signal
6. **15-agent architecture** matches Anthropic's proven C compiler swarm scale (16 agents, 2,000 sessions)

### "One Shot" — What It Actually Means

**One shot does NOT mean perfection on first try.** It means agents handle the grind while you handle the judgment calls.

Based on Anthropic's C compiler data (16 agents, 2,000 sessions) and KolBaer's experience (3 agents, 50-70% first pass):

- **50-70% of acceptance criteria passing** after autonomous run. keel is compiler-adjacent — harder than CRUD apps, similar to Anthropic's C compiler challenge.
- Each agent runs 100-500 autonomous sessions (test -> fix -> test -> repeat via `/ralph-loop`)
- **The remaining 30-50% needs 2-3 weeks of focused human development**: resolution edge cases, false positive tuning, cross-tool integration quirks, performance optimization.

The goal: collapse 8-12 weeks of development into 2-4 weeks autonomous + 2-3 weeks human refinement.

### This Is Harder Than KolBaer

KolBaer was web app CRUD with frontend/backend split. Keel is compiler-adjacent infrastructure with a 4-language resolution engine. The C compiler comparison is apt.

**The resolution engine IS the risk.** 4 languages x different enhancers x 3 tiers of fallback. PRD estimates 10-14 days for M1 alone. Accept this — front-load the test harness so agents can iterate autonomously.

### Critical Risks & Mitigations

| # | Risk | Impact | Mitigation |
|---|------|--------|------------|
| 1 | **Resolution engine complexity** — 4 languages, each with different Tier 2 enhancer | Long M1 phase, possible <85% precision | Per-language resolvers parallelize as separate teammates within Foundation team. Gate on precision before advancing. |
| 2 | **Oxc API surface** — `oxc_semantic` is per-file only | Cross-file stitching needed | Use tree-sitter queries (Spec 001) for cross-file, Oxc for per-file enhancement |
| 3 | **ty (Python) is beta** — v0.0.15, API not stable | Subprocess may change behavior | Use as subprocess only (not library). Fallback to tree-sitter heuristics + Pyright LSP. |
| 4 | **rust-analyzer lazy-load** — 60s+ startup | Performance impact on Rust projects | Lazy-loaded, not always-on. Only triggered when tree-sitter heuristics fail. |
| 5 | **tree-sitter grammar versions** — may differ from installed | Parse failures on edge cases | Pin grammar versions in Cargo.toml. Test against corpus. |
| 6 | **Agent spinning** — same test failure loops indefinitely | Wasted budget, zero progress | Error fingerprinting via `TeammateIdle` hooks: 5=hint, 8=force-skip, 15=cooldown (see [[design-principles#Principle 6|Principle 6]]) |
| 7 | **Inter-agent contract drift** — Foundation's types != Enforcement's expectations | Integration failures at phase gates | Frozen contracts in Phase 0. Contract tests on every cycle. |
| 8 | **Performance benchmarks fail on first pass** — <200ms compile not trivially achievable | Blocks M2 gate | Profile early. Use criterion benchmarks. Optimize hot paths (tree-sitter incremental, SQLite queries). |

---

## 2. Pre-Flight Checklist

Complete ALL items before launching agents.

### Specs & Documents

- [ ] All 13 specs hardened with unambiguous acceptance criteria
  - [ ] [[keel-speckit/000-graph-schema/spec|000 Graph Schema]]
  - [ ] [[keel-speckit/001-treesitter-foundation/spec|001 Tree-sitter Foundation]]
  - [ ] [[keel-speckit/002-typescript-resolution/spec|002 TypeScript Resolution]]
  - [ ] [[keel-speckit/003-python-resolution/spec|003 Python Resolution]]
  - [ ] [[keel-speckit/004-go-resolution/spec|004 Go Resolution]]
  - [ ] [[keel-speckit/005-rust-resolution/spec|005 Rust Resolution]]
  - [ ] [[keel-speckit/006-enforcement-engine/spec|006 Enforcement Engine]]
  - [ ] [[keel-speckit/007-cli-commands/spec|007 CLI Commands]]
  - [ ] [[keel-speckit/008-output-formats/spec|008 Output Formats]]
  - [ ] [[keel-speckit/009-tool-integration/spec|009 Tool Integration]]
  - [ ] [[keel-speckit/010-mcp-http-server/spec|010 MCP/HTTP Server]]
  - [ ] [[keel-speckit/011-vscode-extension/spec|011 VS Code Extension]]
  - [ ] [[keel-speckit/012-distribution/spec|012 Distribution]]
- [ ] [[constitution|Constitution]] reviewed — all 10 articles satisfied by spec coverage
- [ ] [[design-principles|Design Principles]] reviewed — all 10 principles understood
- [ ] [[keel-speckit/test-harness/strategy|Test Harness Strategy]] reviewed — 4 oracles defined, corpus listed

### Test Corpus

- [ ] Test corpus repos cloned and pinned to specific commits
- [ ] Purpose-built test repo (#11) created with known cross-file references
- [ ] LSP ground truth data generated for all repos (TypeScript/tsserver, Python/pyright, Go/gopls, Rust/rust-analyzer)

### External Dependencies Verified

- [ ] `tree-sitter` crate with 4 language grammars compiles
- [ ] `oxc_resolver` + `oxc_semantic` crate compiles (v0.111+)
- [ ] `ty` CLI installed and `ty --output-format json` works on Python test corpus
- [ ] `ra_ap_ide` crate compiles (note: 0.0.x unstable API)
- [ ] `rusqlite` with `bundled` feature compiles

### Agent Teams Prerequisites

- [ ] Claude Code installed with experimental agent teams enabled
- [ ] Environment variable set: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- [ ] Claude Code `settings.json` configured (see [[#13. Agent Teams Configuration]])
- [ ] tmux installed and available
- [ ] `/ralph-loop` and `/tmux-observe` skills installed in Claude Code
- [ ] `bubblewrap` and `socat` installed (Linux: `sudo apt install bubblewrap socat`)
- [ ] Sandbox verified working: `claude --sandbox --print "echo hello"`

---

## 3. Infrastructure Setup

### Architecture Overview

```
tmux session "keel-swarm" (4 panes)

Pane 0: ORCHESTRATOR (Claude Code session, root worktree)
  - AI-driven, runs /ralph-loop
  - Uses /tmux-observe to monitor panes 1-3
  - Checks test results via git across worktrees
  - Enforces phase gates (creates gate marker files)
  - No agent team — standalone session focused on coordination

Pane 1: FOUNDATION (Claude Code + agent team "keel-foundation")
  Lead A (delegate mode) — coordinates Foundation work
  +-- Teammate: ts-resolver     — Spec 002 (TypeScript/Oxc)
  +-- Teammate: py-resolver     — Spec 003 (Python/ty)
  +-- Teammate: go-resolver     — Spec 004 (Go/heuristics)
  +-- Teammate: rust-resolver   — Spec 005 (Rust/rust-analyzer)
  (Lead A handles Specs 000, 001 via subagents or directly)

Pane 2: ENFORCEMENT (Claude Code + agent team "keel-enforcement")
  Lead B (delegate mode) — coordinates Enforcement work
  +-- Teammate: enforcement-engine — Spec 006
  +-- Teammate: cli-commands      — Spec 007
  +-- Teammate: output-formats    — Spec 008

Pane 3: SURFACE (Claude Code + agent team "keel-surface")
  Lead C (delegate mode) — coordinates Surface work
  +-- Teammate: tool-integration — Spec 009
  +-- Teammate: mcp-server       — Spec 010
  +-- Teammate: vscode-ext       — Spec 011
  +-- Teammate: distribution     — Spec 012
```

**Total: 1 orchestrator + 3 leads + 11 teammates = 15 agents** (comparable to Anthropic's 16-agent C compiler swarm)

### Human Role: Manage Only the Orchestrator

The human interacts ONLY with the orchestrator (pane 0). Everything else is autonomous:
- Human launches Phase 0 (single-agent scaffold)
- Human starts the tmux session and kicks off the orchestrator
- Orchestrator manages teams, enforces gates, handles escalation
- Human intervenes only when orchestrator flags 15-repeat escalation or a gate decision needs judgment

### Git Worktrees Setup

Git worktrees provide file isolation between teams — each session has its own working directory while sharing a single git repo.

```bash
# Create the main repo
mkdir -p keel-swarm && cd keel-swarm
git init keel
cd keel

# Initial scaffold (done in Phase 0, before teams spawn)
echo "# keel" > README.md
git add README.md && git commit -m "Initial commit"

# Create worktrees for each team (after Phase 0 completes)
git worktree add ../worktree-a -b foundation
git worktree add ../worktree-b -b enforcement
git worktree add ../worktree-c -b surface

# Create shared directories
mkdir -p .keel-swarm  # Gate marker files go here
mkdir -p results      # Oracle test results per team
```

**Resulting folder structure:**

```
keel-swarm/
+-- keel/                  # Root worktree (orchestrator)
+-- worktree-a/            # Foundation team
+-- worktree-b/            # Enforcement team
+-- worktree-c/            # Surface team
+-- test-corpus/           # Cloned test repos (shared, read-only)
|   +-- excalidraw/        # TypeScript ~120k LOC
|   +-- cal-com/           # TypeScript ~200k LOC
|   +-- typescript-eslint/ # TypeScript ~80k LOC
|   +-- fastapi/           # Python ~30k LOC
|   +-- httpx/             # Python ~25k LOC
|   +-- django-ninja/      # Python ~15k LOC
|   +-- cobra/             # Go ~15k LOC
|   +-- fiber/             # Go ~30k LOC
|   +-- ripgrep/           # Rust ~25k LOC
|   +-- axum/              # Rust ~20k LOC
|   +-- keel-test-repo/    # Multi-language ~5k LOC (purpose-built)
+-- specs/                 # Symlink to keel-speckit/ for agent context
```

### tmux Session Setup

```bash
#!/bin/bash
# Launch the 4-pane keel swarm session
SESSION="keel-swarm"

tmux new-session -d -s $SESSION -n "swarm"

# Pane 0 (top-left): Orchestrator — root worktree
tmux send-keys -t $SESSION "cd keel-swarm/keel && claude --sandbox --dangerously-skip-permissions" C-m

# Pane 1 (top-right): Foundation team — worktree-a
tmux split-window -h -t $SESSION
tmux send-keys -t $SESSION "cd keel-swarm/worktree-a && claude --sandbox --dangerously-skip-permissions" C-m

# Pane 2 (bottom-left): Enforcement team — worktree-b
tmux split-window -v -t $SESSION:0.0
tmux send-keys -t $SESSION "cd keel-swarm/worktree-b && claude --sandbox --dangerously-skip-permissions" C-m

# Pane 3 (bottom-right): Surface team — worktree-c
tmux split-window -v -t $SESSION:0.1
tmux send-keys -t $SESSION "cd keel-swarm/worktree-c && claude --sandbox --dangerously-skip-permissions" C-m

tmux attach -t $SESSION
```

Once each Claude Code session is running:
- **Pane 0 (Orchestrator):** Tell it to run `/ralph-loop` with the orchestrator CLAUDE.md instructions (see [[#14. Orchestrator Design]])
- **Panes 1-3 (Team Leads):** Each creates its agent team and spawns teammates (see [[#6. Agent Assignments]])

---

## 4. Inter-Agent Contracts (Frozen in Phase 0)

These 4 contracts are the seams between agents. They are frozen before Phase 1 starts. Contract tests run on every cycle — failing contract = immediate stop.

### Contract 1: `LanguageResolver` trait (Foundation -> Enforcement)

```rust
pub trait LanguageResolver {
    fn language(&self) -> &str;
    fn parse_file(&self, path: &Path, content: &str) -> ParseResult;
    fn resolve_definitions(&self, file: &Path) -> Vec<Definition>;
    fn resolve_references(&self, file: &Path) -> Vec<Reference>;
    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge>;
}
```

### Contract 2: `GraphStore` trait (Foundation -> Enforcement, Surface)

```rust
pub trait GraphStore {
    fn get_node(&self, hash: &str) -> Option<GraphNode>;
    fn get_node_by_id(&self, id: u64) -> Option<GraphNode>;
    fn get_edges(&self, node_id: u64, direction: EdgeDirection) -> Vec<GraphEdge>;
    fn get_module_profile(&self, module_id: u64) -> Option<ModuleProfile>;
    fn get_nodes_in_file(&self, file_path: &str) -> Vec<GraphNode>;
    fn get_all_modules(&self) -> Vec<GraphNode>;
    fn update_nodes(&mut self, changes: Vec<NodeChange>) -> Result<(), GraphError>;
    fn update_edges(&mut self, changes: Vec<EdgeChange>) -> Result<(), GraphError>;
    fn get_previous_hashes(&self, node_id: u64) -> Vec<String>;
}
```

### Contract 3: Result structs (Enforcement -> Surface)

```rust
pub struct CompileResult {
    pub version: String,
    pub command: String,
    pub status: String,
    pub files_analyzed: Vec<String>,
    pub errors: Vec<Violation>,
    pub warnings: Vec<Violation>,
    pub info: CompileInfo,
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
```

### Contract 4: JSON output schemas (Enforcement, Surface -> external consumers)

All `--json` outputs must validate against schemas in `tests/schemas/`:
- `compile_output.schema.json`
- `discover_output.schema.json`
- `map_output.schema.json`
- `explain_output.schema.json`

---

## 5. Phase 0: Scaffold (Single Agent, ~12-24 Hours)

> **One agent only.** No teams yet. Phase 0 uses the root worktree. Two agents scaffolding = conflict on every file.

### Phase 0 Deliverables (~20 items)

1. **Cargo workspace structure**
   ```
   keel/
   +-- Cargo.toml              # Workspace root
   +-- crates/
   |   +-- keel-core/          # Graph schema, storage, resolution engine
   |   +-- keel-parsers/       # tree-sitter + per-language resolvers
   |   +-- keel-enforce/       # Compile validation, enforcement logic
   |   +-- keel-cli/           # clap CLI, command routing
   |   +-- keel-server/        # MCP + HTTP server (keel serve)
   |   +-- keel-output/        # JSON, LLM, human output formatters
   +-- tests/                  # Integration tests, benchmarks
   +-- .keel/                  # Generated for dogfooding
   +-- extensions/
       +-- vscode/             # VS Code extension (TypeScript)
   ```

2. **Cargo.toml dependencies** matching [[constitution#Article 1 Technology Stack|Constitution Article 1]]

3. **Graph schema Rust types** from [[keel-speckit/000-graph-schema/spec|Spec 000]] — `GraphNode`, `GraphEdge`, `NodeKind`, `EdgeKind`, `ModuleProfile`, `ExternalEndpoint`

4. **SQLite schema** from [[keel-speckit/000-graph-schema/spec|Spec 000]] — all tables, indexes, triggers

5. **`LanguageResolver` trait** (Contract 1) — with stub implementations for all 4 languages

6. **`GraphStore` trait** (Contract 2) — with SQLite implementation

7. **Result structs** (Contract 3) — `CompileResult`, `DiscoverResult`, `ExplainResult`

8. **JSON schemas** (Contract 4) — in `tests/schemas/`

9. **All ~98 test files** with `#[ignore]` annotations — from [[keel-speckit/test-harness/strategy|Test Harness Strategy]]

10. **Contract test files** — validate traits compile and types match

11. **Mock graph fixtures** — pre-built `GraphStore` with known test data for Enforcement team

12. **Mock compile output** — pre-built `CompileResult` fixtures for Surface team

13. **Test corpus setup script** — `scripts/setup_test_repos.sh`

14. **Test harness scripts** — `test-fast.sh`, `test-full.sh`, oracle scripts

15. **Per-worktree CLAUDE.md files** — one per worktree with team-specific instructions (teammates inherit this automatically)

16. **CI workflow** — `.github/workflows/ci.yml` (cargo check, cargo test, clippy)

17. **`.keelignore` template** with defaults

18. **`.keel/config.toml` template** from [[constitution#Article 6 Output Contracts|Constitution]]

19. **JSON schema validation test** — validates fixture outputs against schemas

20. **Gate marker directory** — `.keel-swarm/` for cross-team gate enforcement

### Human Checkpoint After Phase 0

> **Phase gate enforcement**: Worktree branches are created and teams are spawned only after this gate passes.

**Verify:**
- [ ] `cargo check` passes for all crates
- [ ] All 4 `LanguageResolver` stubs compile
- [ ] `GraphStore` SQLite implementation passes basic CRUD tests
- [ ] All ~98 test files exist and are ignored
- [ ] Contract tests exist (even if skipped)
- [ ] Mock fixtures load without errors
- [ ] Test corpus repos cloned and pinned
- [ ] `test-fast.sh` runs and exits 0 (all tests skipped)
- [ ] Git worktrees created (`worktree-a`, `worktree-b`, `worktree-c`)
- [ ] Per-worktree CLAUDE.md files in place

---

## 6. Agent Assignments

### Team Architecture: 3 Nested Agent Teams

Each team is a Claude Code agent team with a lead in **delegate mode** (can't edit code, only coordinates) and 3-4 teammates who do the actual implementation. Each teammate runs `/ralph-loop` for autonomous test-fix-test cycles within their crate scope.

**Why 3 teams of 3-4, not 1 flat team of 11?** A single team with 11 teammates creates a coordination bottleneck at the lead. Three teams of 3-4 teammates each keeps coordination manageable and matches the natural Foundation -> Enforcement -> Surface dependency chain.

### Foundation Team — `keel-foundation` (Pane 1, worktree-a)

**Lead A** (delegate mode) — coordinates Foundation work. Handles Specs 000, 001 via subagents or directly before spawning language-specific teammates.

| Teammate | Spec | Crate | Files Owned |
|----------|------|-------|-------------|
| `ts-resolver` | [[keel-speckit/002-typescript-resolution/spec\|002]] | `keel-parsers/src/typescript/` | All TypeScript resolver files |
| `py-resolver` | [[keel-speckit/003-python-resolution/spec\|003]] | `keel-parsers/src/python/` | All Python resolver files |
| `go-resolver` | [[keel-speckit/004-go-resolution/spec\|004]] | `keel-parsers/src/go/` | All Go resolver files |
| `rust-resolver` | [[keel-speckit/005-rust-resolution/spec\|005]] | `keel-parsers/src/rust/` | All Rust resolver files |

**Gate M1:** Resolution precision >85% per language measured against LSP ground truth.

**Lead A creates the team:**
```
Create team "keel-foundation"
Create tasks for Specs 000, 001, 002, 003, 004, 005
Spawn teammates: ts-resolver, py-resolver, go-resolver, rust-resolver
Handle Specs 000-001 first (they unblock the resolvers)
```

### Enforcement Team — `keel-enforcement` (Pane 2, worktree-b)

**Lead B** (delegate mode) — coordinates Enforcement work.

| Teammate | Spec | Crate | Files Owned |
|----------|------|-------|-------------|
| `enforcement-engine` | [[keel-speckit/006-enforcement-engine/spec\|006]] | `keel-enforce/` | Enforcement logic, circuit breaker, placement scoring |
| `cli-commands` | [[keel-speckit/007-cli-commands/spec\|007]] | `keel-cli/` | clap CLI, command routing |
| `output-formats` | [[keel-speckit/008-output-formats/spec\|008]] | `keel-output/` | JSON/LLM/human formatters |

**Starts Phase 1 against mock graph fixtures.** Builds enforcement logic, CLI skeleton, output formatters — all testable against mock data. Real graph integration at M1 gate.

**Gate M2:** All CLI commands functional, enforcement catches known mutations (>95% TP rate).

### Surface Team — `keel-surface` (Pane 3, worktree-c)

**Lead C** (delegate mode) — coordinates Surface work.

| Teammate | Spec | Crate | Files Owned |
|----------|------|-------|-------------|
| `tool-integration` | [[keel-speckit/009-tool-integration/spec\|009]] | tool config generation, hook scripts | All tool integration files |
| `mcp-server` | [[keel-speckit/010-mcp-http-server/spec\|010]] | `keel-server/` | MCP + HTTP server |
| `vscode-ext` | [[keel-speckit/011-vscode-extension/spec\|011]] | `extensions/vscode/` | VS Code extension (TypeScript) |
| `distribution` | [[keel-speckit/012-distribution/spec\|012]] | CI/CD, install scripts | Build + distribution |

**Starts Phase 1 with template generation and tool detection logic.** Builds hook configs, instruction templates, MCP server skeleton — all testable against mock data. Real compile integration at M2 gate.

**Gate M3:** End-to-end with Claude Code + Cursor on real repos.

### Orchestrator (Pane 0, root worktree)

**Not part of any team.** A standalone Claude Code session focused on cross-team coordination.

**Responsibilities:**
- Run `/ralph-loop` to continuously monitor and enforce
- Use `/tmux-observe` to read output from panes 1-3
- Check test results across worktrees via git operations
- Write gate marker files (`.keel-swarm/gate-m1-passed`, etc.)
- Track cross-team error patterns
- Write `swarm-status.md` by reading state from all 3 worktrees
- Trigger human review at 15-repeat escalation threshold

**Does NOT:** Write product code, modify Cargo.toml, edit test files, push to any worktree.

---

## 7. Phase Sequencing

```
Phase 0 (Single Agent):  Cargo scaffold, test harness, mock fixtures, contracts, pre-written tests
    -- HUMAN CHECKPOINT --

Phase 1 (3 Teams in Parallel):
  Foundation team = tree-sitter parsing + per-language resolution (specs 000-005)
    Lead A coordinates, 4 resolver teammates work in parallel
  Enforcement team = enforcement engine against mock graph (spec 006) + CLI skeleton (spec 007)
    Lead B coordinates, 3 component teammates work in parallel
  Surface team = tool templates + detection logic (spec 009) + MCP skeleton (spec 010)
    Lead C coordinates, 4 component teammates work in parallel
    -- GATE M1: Resolution >85% precision per language --

Phase 2 (3 Teams in Parallel):
  Foundation team = performance tuning + edge cases + Tier 3 fallback stubs
  Enforcement team = CLI wiring with real graph + output formatters (spec 008) + circuit breaker
  Surface team = hook flow with real compile + instruction files + CI templates
    -- GATE M2: All commands work, enforcement catches mutations (>95% TP) --

Phase 3 (3 Teams in Parallel):
  Foundation team = resolution edge cases + cross-language endpoint detection
  Enforcement team = batch mode + suppress mechanism + progressive adoption polish
  Surface team = VS Code extension (spec 011) + distribution (spec 012) + install scripts
    -- GATE M3: E2E with Claude Code + Cursor on real repos --

Phase 4 (All):  Dogfooding — use keel to develop keel
```

### Phase Gate Enforcement

**The orchestrator creates gate marker files only after gate criteria are met.** Teams check for gate markers before advancing to next-phase work. Orchestrator uses `/tmux-observe` and git to verify gate criteria across all 3 worktrees.

**Cross-team coordination mechanism:**
1. Each team lead pushes results to their worktree branch
2. Orchestrator pulls all branches, runs cross-team checks
3. When gate criteria pass, orchestrator writes marker: `.keel-swarm/gate-m1-passed`
4. Team leads check for gate markers and advance to next phase

**Cross-team integration at gates:**
- **M1 gate:** Orchestrator merges `foundation` branch into `enforcement` branch so Enforcement gets the real graph
- **M2 gate:** Orchestrator merges `enforcement` branch into `surface` branch so Surface gets real compile output

**Gate M1 criteria (binary):**
- [ ] TypeScript resolution precision >85% on excalidraw, typescript-eslint
- [ ] Python resolution precision >82% on FastAPI, httpx
- [ ] Go resolution precision >85% on cobra, fiber
- [ ] Rust resolution precision >75% on ripgrep, axum
- [ ] All 4 languages parse without panic on full test corpus
- [ ] `keel init` <10s on 50k LOC test repo
- [ ] Graph node/edge counts within 10% of LSP baseline

**Gate M2 criteria (binary):**
- [ ] All CLI commands return valid output (init, map, discover, compile, where, explain)
- [ ] Mutation test: >95% true positive rate
- [ ] Mutation test: <5% false positive rate
- [ ] Circuit breaker: 3-attempt escalation verified
- [ ] Clean compile: empty stdout + exit 0
- [ ] `compile` <200ms on single-file change
- [ ] JSON output validates against all schemas

**Gate M3 criteria (binary):**
- [ ] Claude Code e2e: hooks fire, errors shown, LLM fixes violations
- [ ] Cursor e2e: hooks fire (including v2.0 workaround)
- [ ] `keel init` generates correct configs for all detected tools
- [ ] MCP server: all 5 tools respond correctly
- [ ] VS Code extension: status bar, diagnostics, CodeLens work
- [ ] Builds on Linux x86_64, macOS arm64, Windows x86_64

---

## 8. Spawn Prompts

Each team lead spawns teammates with detailed prompts. Teammates load the worktree's project CLAUDE.md automatically, so spawn prompts focus on spec-specific scope and crate ownership.

### Foundation Team Spawn Prompts

**Lead A initial prompt (given by human or orchestrator):**
```
You are Lead A of the Foundation team. You are in delegate mode — coordinate,
don't code. Create team "keel-foundation".

First, handle Specs 000 and 001 yourself via subagents:
- Spec 000 (Graph Schema): crates/keel-core/ — bedrock types + SQLite storage
- Spec 001 (Tree-sitter Foundation): crates/keel-parsers/src/lib.rs — universal Tier 1

Then spawn 4 resolver teammates and assign their tasks.

Gate M1 target: Resolution precision >85% per language against LSP ground truth.
Run: ./scripts/test_graph_correctness.sh

Use plan approval — review each teammate's plan before they implement.
```

**ts-resolver spawn prompt:**
```
You are ts-resolver. Your sole focus is Spec 002 (TypeScript Resolution).

Spec file: specs/002-typescript-resolution/spec.md — read this FIRST.
Your crate: crates/keel-parsers/src/typescript/
Test command: cargo test -p keel-parsers -- typescript
Contract: LanguageResolver trait (frozen — do NOT change the signature)

Target: TypeScript resolution precision >85% on excalidraw and typescript-eslint
test corpus repos.

Rules:
- Oxc (oxc_resolver + oxc_semantic) is your Tier 2 enhancer
- tree-sitter is Tier 1 (already implemented by Lead A in Spec 001)
- Pure Rust. No FFI in hot path.
- Run cargo test after EVERY change
- Run /ralph-loop for autonomous test-fix-test cycles
```

**py-resolver spawn prompt:**
```
You are py-resolver. Your sole focus is Spec 003 (Python Resolution).

Spec file: specs/003-python-resolution/spec.md — read this FIRST.
Your crate: crates/keel-parsers/src/python/
Test command: cargo test -p keel-parsers -- python
Contract: LanguageResolver trait (frozen — do NOT change the signature)

Target: Python resolution precision >82% on FastAPI and httpx test corpus repos.

Rules:
- ty is your Tier 2 enhancer — subprocess only (ty --output-format json)
- ty is beta (v0.0.15) — handle subprocess failures gracefully
- Fallback to tree-sitter heuristics if ty unavailable
- Run cargo test after EVERY change
- Run /ralph-loop for autonomous test-fix-test cycles
```

**go-resolver spawn prompt:**
```
You are go-resolver. Your sole focus is Spec 004 (Go Resolution).

Spec file: specs/004-go-resolution/spec.md — read this FIRST.
Your crate: crates/keel-parsers/src/go/
Test command: cargo test -p keel-parsers -- go
Contract: LanguageResolver trait (frozen — do NOT change the signature)

Target: Go resolution precision >85% on cobra and fiber test corpus repos.

Rules:
- Go is simplest — tree-sitter heuristics alone achieve ~85-92%
- No external enhancer needed (Tier 2 = enhanced heuristics)
- Run cargo test after EVERY change
- Run /ralph-loop for autonomous test-fix-test cycles
```

**rust-resolver spawn prompt:**
```
You are rust-resolver. Your sole focus is Spec 005 (Rust Resolution).

Spec file: specs/005-rust-resolution/spec.md — read this FIRST.
Your crate: crates/keel-parsers/src/rust/
Test command: cargo test -p keel-parsers -- rust
Contract: LanguageResolver trait (frozen — do NOT change the signature)

Target: Rust resolution precision >75% on ripgrep and axum test corpus repos.

Rules:
- rust-analyzer (ra_ap_ide) is your Tier 2 enhancer — lazy-loaded (60s+ startup)
- Only trigger rust-analyzer when tree-sitter heuristics fail
- Rust is the hardest language — 75% precision is the minimum gate
- Run cargo test after EVERY change
- Run /ralph-loop for autonomous test-fix-test cycles
```

### Enforcement Team Spawn Prompts

**Lead B initial prompt:**
```
You are Lead B of the Enforcement team. You are in delegate mode — coordinate,
don't code. Create team "keel-enforcement".

Spawn 3 teammates: enforcement-engine, cli-commands, output-formats.
All work against mock graph fixtures until Gate M1 passes.

Gate M2 target: All commands work, enforcement catches >95% of mutations.

Use plan approval — review each teammate's plan before they implement.
```

**enforcement-engine spawn prompt:**
```
You are enforcement-engine. Your sole focus is Spec 006 (Enforcement Engine).

Spec file: specs/006-enforcement-engine/spec.md — read this FIRST.
Your crate: crates/keel-enforce/
Test command: cargo test -p keel-enforce
Contracts you depend on: GraphStore trait (use mock fixtures until M1 gate)
Contracts you own: CompileResult struct (frozen)

Rules:
- Use mock graph fixtures in Phase 1. Real graph after M1 gate.
- Every ERROR must have non-empty fix_hint
- Clean compile = empty stdout + exit 0
- Implement circuit breaker: 3-attempt escalation
- Run /ralph-loop for autonomous test-fix-test cycles
```

**cli-commands spawn prompt:**
```
You are cli-commands. Your sole focus is Spec 007 (CLI Commands).

Spec file: specs/007-cli-commands/spec.md — read this FIRST.
Your crate: crates/keel-cli/
Test command: cargo test -p keel-cli
Contracts you depend on: CompileResult, DiscoverResult, ExplainResult (use mocks)

Rules:
- All commands: init, map, discover, compile, where, explain, serve, deinit, stats
- Use clap for argument parsing
- Forward slashes in all output, NO_COLOR respected
- Run /ralph-loop for autonomous test-fix-test cycles
```

**output-formats spawn prompt:**
```
You are output-formats. Your sole focus is Spec 008 (Output Formats).

Spec file: specs/008-output-formats/spec.md — read this FIRST.
Your crate: crates/keel-output/
Test command: cargo test -p keel-output
Contracts you own: JSON output schemas in tests/schemas/ (frozen)

Rules:
- Three output modes: JSON (--json), LLM (--llm), human (default)
- JSON output must validate against schemas
- LLM output optimized for agent consumption
- Run /ralph-loop for autonomous test-fix-test cycles
```

### Surface Team Spawn Prompts

**Lead C initial prompt:**
```
You are Lead C of the Surface team. You are in delegate mode — coordinate,
don't code. Create team "keel-surface".

Spawn 4 teammates: tool-integration, mcp-server, vscode-ext, distribution.
All work against mock compile output until Gate M2 passes.

Gate M3 target: E2E with Claude Code + Cursor on real repos.

Use plan approval — review each teammate's plan before they implement.
```

**tool-integration spawn prompt:**
```
You are tool-integration. Your sole focus is Spec 009 (Tool Integration).

Spec file: specs/009-tool-integration/spec.md — read this FIRST.
Your files: tool config templates, hook scripts, CI templates
Test command: cargo test -- tool_integration
Contracts you depend on: CLI commands from Enforcement (use mock CLI output)

Rules:
- 9+ tool configs: Claude Code, Cursor, Windsurf, Copilot, Aider, etc.
- Hook scripts must validate file paths (reject metacharacters)
- Use mock compile output in Phase 1. Real compile after M2 gate.
- Run /ralph-loop for autonomous test-fix-test cycles
```

**mcp-server spawn prompt:**
```
You are mcp-server. Your sole focus is Spec 010 (MCP/HTTP Server).

Spec file: specs/010-mcp-http-server/spec.md — read this FIRST.
Your crate: crates/keel-server/
Test command: cargo test -p keel-server
Contracts you depend on: Core library (use mock KeelCore)

Rules:
- MCP server is thin wrapper (~300-500 lines) — no new logic
- 5 MCP tools: compile, discover, map, where, explain
- HTTP mode for VS Code extension
- Run /ralph-loop for autonomous test-fix-test cycles
```

**vscode-ext spawn prompt:**
```
You are vscode-ext. Your sole focus is Spec 011 (VS Code Extension).

Spec file: specs/011-vscode-extension/spec.md — read this FIRST.
Your directory: extensions/vscode/
Test command: cd extensions/vscode && npm test
Contracts you depend on: keel serve --http (use mock HTTP server)

Rules:
- Display layer only (~500 lines TypeScript)
- Status bar, inline diagnostics, CodeLens
- No new logic — surfaces keel serve output
- Run /ralph-loop for autonomous test-fix-test cycles
```

**distribution spawn prompt:**
```
You are distribution. Your sole focus is Spec 012 (Distribution).

Spec file: specs/012-distribution/spec.md — read this FIRST.
Your files: CI/CD workflows, install scripts, Dockerfile
Test command: cargo build --release

Rules:
- Cross-platform: Linux x86_64, macOS arm64, Windows x86_64
- Single binary, zero runtime dependencies
- Install methods: curl | sh, brew, cargo install, winget, scoop
- Binary size budget: 20-35MB (flag if >40MB)
- Run /ralph-loop for autonomous test-fix-test cycles
```

---

## 9. Ralph Loop

Each participant in the swarm runs `/ralph-loop` — Claude Code's autonomous test-fix-test skill. No custom loop scripts needed.

### How `/ralph-loop` Works

`/ralph-loop` is a Claude Code skill that puts the agent into a continuous cycle:
1. Run tests for the agent's scope
2. Analyze failures
3. Fix the code
4. Run tests again
5. Repeat until tests pass or escalation triggers

### Who Runs `/ralph-loop`

| Agent | Scope | Test Command |
|-------|-------|-------------|
| Orchestrator | Cross-team gate checks | `./scripts/test-full.sh` across worktrees |
| Lead A | Foundation team progress | `cargo test -p keel-core -p keel-parsers` |
| Lead B | Enforcement team progress | `cargo test -p keel-enforce -p keel-cli -p keel-output` |
| Lead C | Surface team progress | `cargo test -p keel-server && cd extensions/vscode && npm test` |
| ts-resolver | TypeScript resolver | `cargo test -p keel-parsers -- typescript` |
| py-resolver | Python resolver | `cargo test -p keel-parsers -- python` |
| go-resolver | Go resolver | `cargo test -p keel-parsers -- go` |
| rust-resolver | Rust resolver | `cargo test -p keel-parsers -- rust` |
| enforcement-engine | Enforcement logic | `cargo test -p keel-enforce` |
| cli-commands | CLI commands | `cargo test -p keel-cli` |
| output-formats | Output formatters | `cargo test -p keel-output` |
| tool-integration | Tool configs | `cargo test -- tool_integration` |
| mcp-server | MCP/HTTP server | `cargo test -p keel-server` |
| vscode-ext | VS Code extension | `cd extensions/vscode && npm test` |
| distribution | Build + distribution | `cargo build --release` |

### Leads vs Teammates

- **Teammates** run `/ralph-loop` to autonomously fix code within their crate scope
- **Leads** run `/ralph-loop` to monitor teammate progress, redistribute work, and escalate blockers
- **Orchestrator** runs `/ralph-loop` to monitor all 3 teams, enforce gates, and coordinate cross-team integration

---

## 10. Cross-Team Coordination

Since agent teams can't message across teams (one team per Claude Code session), coordination uses filesystem and git:

### Git Push/Pull

Each worktree pushes results to its branch. Other worktrees pull when needed.

```bash
# From orchestrator (root worktree), check all branches:
git fetch --all
git log --oneline foundation..HEAD
git log --oneline enforcement..HEAD
git log --oneline surface..HEAD
```

### Gate Marker Files

The orchestrator writes gate markers to the shared `.keel-swarm/` directory:

```
.keel-swarm/
+-- gate-m1-passed          # Written when M1 criteria verified
+-- gate-m2-passed          # Written when M2 criteria verified
+-- gate-m3-passed          # Written when M3 criteria verified
+-- status.md               # Current swarm status dashboard
```

**Marker file format:**
```
gate: M1
passed: 2026-03-15T14:30:00Z
criteria:
  ts_precision: 87%
  py_precision: 84%
  go_precision: 91%
  rust_precision: 78%
  no_panics: true
  init_time: 8.2s
  graph_delta: 6%
```

### Cross-Team Integration Merge

When a gate passes, the orchestrator merges the upstream branch into the downstream worktree:

```bash
# M1 gate passed — give Enforcement the real graph
cd worktree-b
git merge foundation --no-edit

# M2 gate passed — give Surface the real compile output
cd worktree-c
git merge enforcement --no-edit
```

### Shared Test Results

Each team writes oracle results to `results/` in the repo:

```
results/
+-- foundation/
|   +-- lsp-ground-truth-ts.json
|   +-- lsp-ground-truth-py.json
|   +-- lsp-ground-truth-go.json
|   +-- lsp-ground-truth-rust.json
+-- enforcement/
|   +-- mutation-test-results.json
|   +-- performance-benchmarks.json
+-- surface/
    +-- e2e-claude-code.json
    +-- e2e-cursor.json
```

---

## 11. Error Fingerprinting & Escalation

Error fingerprinting prevents agent spinning. Within each team, `TeammateIdle` hooks implement escalation natively. The orchestrator tracks cross-team patterns via `/tmux-observe`.

### Intra-Team Escalation (via `TeammateIdle` hooks)

When a teammate goes idle (stops making progress), the team lead receives a `TeammateIdle` notification. The lead tracks failure counts per teammate and escalates:

**Escalation thresholds** (from [[design-principles#Principle 6|Principle 6]]):
- **5 consecutive failures:** Lead sends teammate a message with a targeted hint: "This error has occurred 5 times. Try a different approach. Consider: [specific suggestion based on error pattern]"
- **8 consecutive failures:** Lead reassigns the task to a different teammate or handles it via subagent. Logs for human review.
- **15 consecutive failures:** Lead flags to orchestrator via commit message convention (`ESCALATE: [description]`). Orchestrator triggers human review.

### Cross-Team Escalation (via orchestrator)

The orchestrator uses `/tmux-observe` to read output from all 3 team panes and detect:
- Teams stuck on the same error across multiple cycles
- Gate criteria not progressing
- Build failures affecting multiple teams

### Error Fingerprint Format

Each team lead tracks fingerprints in their task descriptions:

```
Error fingerprint: hash(test_name + error_pattern + file_path)
- Groups identical failures
- Allows different manifestations of same root cause to escalate together
- Reset on new error: if the fix changes the error, counter resets
- Only identical consecutive failures escalate
```

---

## 12. Task Dependency Graph Per Team

Each team lead creates tasks with dependencies using `TaskCreate` and `TaskUpdate` with `addBlockedBy`/`addBlocks`.

### Foundation Team Tasks

```
Task: "Implement Graph Schema (Spec 000)"
  -> blocks: all other Foundation tasks

Task: "Implement Tree-sitter Foundation (Spec 001)"
  -> blockedBy: Spec 000
  -> blocks: all resolver tasks

Task: "Implement TypeScript Resolution (Spec 002)"  [ts-resolver]
  -> blockedBy: Spec 001

Task: "Implement Python Resolution (Spec 003)"  [py-resolver]
  -> blockedBy: Spec 001

Task: "Implement Go Resolution (Spec 004)"  [go-resolver]
  -> blockedBy: Spec 001

Task: "Implement Rust Resolution (Spec 005)"  [rust-resolver]
  -> blockedBy: Spec 001
```

Specs 002-005 run **in parallel** once Spec 001 completes.

### Enforcement Team Tasks

```
Task: "Set up mock graph fixtures"
  -> blocks: all Enforcement tasks

Task: "Implement Enforcement Engine (Spec 006)"  [enforcement-engine]
  -> blockedBy: mock fixtures

Task: "Implement CLI Commands (Spec 007)"  [cli-commands]
  -> blockedBy: mock fixtures

Task: "Implement Output Formats (Spec 008)"  [output-formats]
  -> blockedBy: mock fixtures
```

Specs 006-008 run **in parallel** once mock fixtures are ready.

### Surface Team Tasks

```
Task: "Set up mock compile output fixtures"
  -> blocks: all Surface tasks

Task: "Implement Tool Integration (Spec 009)"  [tool-integration]
  -> blockedBy: mock fixtures

Task: "Implement MCP/HTTP Server (Spec 010)"  [mcp-server]
  -> blockedBy: mock fixtures

Task: "Implement VS Code Extension (Spec 011)"  [vscode-ext]
  -> blockedBy: mock fixtures (Phase 1), Spec 010 (Phase 3)

Task: "Implement Distribution (Spec 012)"  [distribution]
  -> blockedBy: mock fixtures (Phase 1)
```

### Cross-Team Gates (Enforced by Orchestrator)

```
Gate M1: Foundation must achieve >85% precision per language
  -> Triggers: merge foundation into enforcement branch
  -> Enforcement team switches from mock to real graph

Gate M2: Enforcement must achieve >95% TP mutation rate
  -> Triggers: merge enforcement into surface branch
  -> Surface team switches from mock to real compile output

Gate M3: Surface must achieve E2E on real repos
  -> Triggers: all-team dogfooding phase
```

---

## 13. Agent Teams Configuration

### Claude Code Settings

Add to Claude Code `settings.json`:

```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  },
  "teammateMode": "tmux",
  "sandbox": {
    "enabled": true,
    "autoAllowBashIfSandboxed": true,
    "allowUnsandboxedCommands": false,
    "excludedCommands": ["docker"]
  },
  "permissions": {
    "allow": [
      "Bash(cargo test*)",
      "Bash(cargo build*)",
      "Bash(cargo check*)",
      "Bash(cargo clippy*)",
      "Bash(cargo fmt*)",
      "Bash(cargo install*)",
      "Bash(cargo run*)",
      "Bash(./scripts/*)",
      "Bash(git *)",
      "Bash(gh *)",
      "Bash(npm *)",
      "Bash(npx *)",
      "Bash(ty *)",
      "Bash(rustup *)",
      "Bash(tmux *)",
      "Bash(curl *)",
      "Bash(wget *)",
      "Bash(mkdir *)",
      "Bash(cp *)",
      "Bash(mv *)",
      "Bash(rm *)",
      "Bash(ls *)",
      "Bash(cat *)",
      "Bash(cd *)",
      "Read",
      "Write",
      "Edit",
      "Glob",
      "Grep",
      "Skill",
      "Task",
      "SendMessage",
      "TaskCreate",
      "TaskUpdate",
      "TaskList",
      "TaskGet"
    ],
    "deny": [
      "Read(~/.ssh/**)",
      "Read(~/.aws/**)",
      "Read(~/.kube/**)",
      "Read(~/.gnupg/**)",
      "Read(/etc/shadow)"
    ]
  }
}
```

### Permission Pre-Approval

To reduce friction across all 15 agents, pre-approve common operations. With `autoAllowBashIfSandboxed: true`, all bash commands are auto-approved inside the sandbox anyway — these explicit entries serve as documentation and fallback if sandbox is ever disabled:

- **Cargo ecosystem** — `cargo test/build/check/clippy/fmt/install/run` + `rustup`
- **Node ecosystem** — `npm`, `npx` (for VS Code extension)
- **Git + GitHub** — `git`, `gh` (worktrees handle isolation)
- **Language tools** — `ty` (Python type checker subprocess)
- **Infrastructure** — `tmux` (orchestrator), `curl`/`wget`, basic file ops (`mkdir`, `cp`, `mv`, `rm`, `ls`, `cat`, `cd`)
- **Scripts** — `./scripts/*` (test harness, setup)
- **File tools** — `Read`, `Write`, `Edit`, `Glob`, `Grep` — always allowed (crate ownership prevents conflicts)
- **Agent teams plumbing** — `Skill`, `Task`, `SendMessage`, `TaskCreate`, `TaskUpdate`, `TaskList`, `TaskGet` — required for `/ralph-loop`, `/tmux-observe`, teammate coordination, and subagent spawning

### Sandbox Configuration

All agents launch with `--sandbox --dangerously-skip-permissions`. The sandbox block above enables:

- **`autoAllowBashIfSandboxed: true`** — bash commands auto-approved when sandboxed (safe because bubblewrap restricts the blast radius)
- **`allowUnsandboxedCommands: false`** — prevents agents from escaping the sandbox via unsandboxed fallback
- **`excludedCommands: ["docker"]`** — docker doesn't work inside bubblewrap; excluding it prevents hang/error loops
- **No network restriction** — agents have full internet access (useful for searching docs, crate registries, Stack Overflow, etc.). The sandbox protects the *filesystem*, not the network.
- **`permissions.deny`** — explicitly blocks reading SSH keys, AWS credentials, kube configs, GPG keys, and `/etc/shadow` even if an agent tries

The shared test corpus can be mounted read-only via `--add-dir` if it lives outside the worktree:

```bash
claude --sandbox --dangerously-skip-permissions --add-dir ../test-corpus:ro
```

### `TeammateIdle` Hook Configuration

Create `.claude/hooks.json` in each worktree:

```json
{
  "hooks": {
    "TeammateIdle": [
      {
        "command": "echo 'TEAMMATE_IDLE: {{teammate_name}} has gone idle after {{idle_reason}}'",
        "description": "Log teammate idle events for escalation tracking"
      }
    ],
    "TaskCompleted": [
      {
        "command": "echo 'TASK_COMPLETED: {{task_id}} by {{teammate_name}}'",
        "description": "Log task completions for progress tracking"
      }
    ]
  }
}
```

The team lead reads these hook outputs to track escalation counters and gate progress.

### Delegate Mode for Leads

All 3 team leads run in delegate mode. When spawning teammates, the lead uses:

```
Task tool with mode: "delegate"
```

This means:
- Lead **cannot** edit files directly
- Lead coordinates via messages, task assignments, and plan approvals
- Lead reviews teammate plans before they implement (plan approval)
- This prevents the lead from introducing its own bugs (same principle as [[design-principles#Principle 7|Principle 7]])

---

## 14. Orchestrator Design

### Orchestrator CLAUDE.md

Place this in the root worktree's CLAUDE.md (or include in the orchestrator's initial prompt):

```markdown
# Keel Orchestrator — Cross-Team Coordinator

You are the orchestrator for the keel agent swarm. You are NOT part of any team.
You monitor 3 teams across 3 worktrees and enforce phase gates.

## Your Tools
- /tmux-observe — read output from panes 1-3 (Foundation, Enforcement, Surface)
- /ralph-loop — continuous monitoring cycle
- git operations — check test results, merge branches at gates

## Your Responsibilities
1. Monitor all 3 teams via /tmux-observe
2. Check test results: pull each branch, run test scripts, compare against gate criteria
3. Write gate markers when criteria pass: .keel-swarm/gate-m1-passed, etc.
4. Merge branches at gate transitions (foundation -> enforcement at M1, enforcement -> surface at M2)
5. Detect cross-team patterns (same error in multiple teams = systemic issue)
6. Write swarm-status.md with current state of all teams
7. Flag human review when 15-repeat escalation fires

## Gate Check Procedure
1. git fetch --all
2. For each worktree: checkout branch, run test scripts, parse results
3. Compare results against gate criteria (see agent-swarm.md Section 7)
4. If ALL criteria pass: write gate marker, perform cross-team merge
5. If criteria don't pass: log which criteria are failing, check progress trend

## You Do NOT
- Write Rust code
- Modify Cargo.toml
- Edit test files
- Push to any worktree branch
- Make architectural decisions
- Modify any team's tasks

## Monitoring Cycle
Run /ralph-loop with this cycle:
1. /tmux-observe pane 1 — check Foundation progress
2. /tmux-observe pane 2 — check Enforcement progress
3. /tmux-observe pane 3 — check Surface progress
4. git fetch --all — get latest from all branches
5. Check gate criteria for current phase
6. Update swarm-status.md
7. If gate passes: write marker, merge branches, notify (commit message)
8. If 15-repeat escalation detected: flag for human
```

### Swarm Status Dashboard

The orchestrator maintains `swarm-status.md` in the root worktree:

```markdown
# Keel Swarm Status
Updated: [timestamp]

## Current Phase: 1

## Teams
| Team | Lead | Teammates | Active Tasks | Completed Tasks |
|------|------|-----------|-------------|----------------|
| Foundation | Lead A | ts/py/go/rust-resolver | 4 | 2 |
| Enforcement | Lead B | engine/cli/output | 3 | 1 |
| Surface | Lead C | tools/mcp/vscode/dist | 2 | 2 |

## Gate Progress
| Gate | Status | Blocking Criteria |
|------|--------|-------------------|
| M1 | IN_PROGRESS | TS: 81% (need 85%), Rust: 72% (need 75%) |
| M2 | WAITING | Blocked by M1 |
| M3 | WAITING | Blocked by M2 |

## Escalations
- [none]
```

---

## 15. Agent Teams Limitations

Document these limitations so agents don't waste time trying unsupported operations:

### No Cross-Team Messaging

Teams can't message other teams directly. Agent Teams messaging is intra-team only. Cross-team coordination uses:
- Git push/pull between worktree branches
- Gate marker files in `.keel-swarm/`
- Shared test results in `results/`
- Orchestrator reading all panes via `/tmux-observe`

### No Nested Teams

Teammates can't spawn their own teams. However:
- Leads CAN have teams (that's the whole architecture)
- Teammates CAN use the Task tool to spawn subagents for complex subtasks
- Subagents are not teammates — they're ephemeral helpers

### No Session Resumption

If a teammate crashes, the lead spawns a replacement:
1. Lead detects crash via `TeammateIdle` notification or missing progress
2. Lead creates a new teammate with the same spawn prompt
3. New teammate picks up from the last committed state in git
4. Task is reassigned to the new teammate

### One Team Per Session

Each Claude Code session (worktree) gets exactly one team. This is why the architecture uses 4 separate sessions in tmux panes.

### File Conflicts Within a Team

Teammates sharing a worktree must own non-overlapping files. Keel's crate structure provides natural isolation:
- Each resolver owns `keel-parsers/src/<language>/`
- Each enforcement component owns its crate
- VS Code extension is isolated in `extensions/vscode/`

If two teammates need to edit the same file, the lead must serialize the work (one teammate at a time) or restructure the file ownership.

---

## 16. Sandbox Hardening

### Why Sandbox?

All 15 agents run with `--dangerously-skip-permissions` for extended unsupervised periods (days/weeks). Without sandboxing, a confused agent could write outside its worktree, exfiltrate secrets, or reach arbitrary network endpoints. OS-level sandboxing makes `--dangerously-skip-permissions` safe by restricting what "permission" actually grants.

### bubblewrap (Linux) / Seatbelt (macOS)

Claude Code's native `--sandbox` flag uses **bubblewrap** on Linux and **Seatbelt** on macOS — kernel-level isolation, not containerization. This is better than Docker for agent swarms:

- **No Docker-in-Docker complexity** — each agent is a simple sandboxed process
- **Lower overhead** — no container image layers, no daemon
- **Built-in** — ships with Claude Code, no external orchestration
- **Per-session isolation** — each session is sandboxed to its worktree directory

### 3-Layer Isolation Model

```
Layer 1: Sandbox (OS-level) — bubblewrap restricts filesystem writes to CWD
         (network unrestricted — agents can search docs freely)
         ┌─────────────────────────────────────────────────┐
         │                                                 │
Layer 2: │ Git worktrees (logical) — each team confined    │
         │ to its own worktree directory                   │
         │  ┌───────────┐ ┌───────────┐ ┌───────────┐     │
         │  │worktree-a │ │worktree-b │ │worktree-c │     │
         │  │Foundation │ │Enforcement│ │  Surface  │     │
         │  │           │ │           │ │           │     │
Layer 3: │  │ts/ py/ go/│ │enforce/   │ │tools/ mcp/│     │
         │  │rust/      │ │cli/ out/  │ │vscode/dist│     │
         │  │(crate own)│ │(crate own)│ │(crate own)│     │
         │  └───────────┘ └───────────┘ └───────────┘     │
         └─────────────────────────────────────────────────┘
```

1. **Sandbox (OS-level)**: bubblewrap restricts writes to CWD (network unrestricted)
2. **Git worktrees (logical)**: each team confined to its worktree directory — sandbox CWD = worktree root
3. **Crate ownership (convention)**: teammates own non-overlapping files within their worktree

### Authoritative Sandbox Configuration

This is the complete `settings.json` sandbox block (also shown in [[#13. Agent Teams Configuration]]):

```json
{
  "sandbox": {
    "enabled": true,
    "autoAllowBashIfSandboxed": true,
    "allowUnsandboxedCommands": false,
    "excludedCommands": ["docker"]
  },
  "permissions": {
    "deny": [
      "Read(~/.ssh/**)",
      "Read(~/.aws/**)",
      "Read(~/.kube/**)",
      "Read(~/.gnupg/**)",
      "Read(/etc/shadow)"
    ]
  }
}
```

### What Sandbox Prevents

| Threat | Mitigation |
|--------|-----------|
| Agent writes outside worktree (e.g., `rm -rf /`) | bubblewrap restricts writes to CWD (worktree root) |
| Agent reads SSH keys / AWS creds / kube config | `permissions.deny` blocks + bubblewrap filesystem isolation |
| Agent runs Docker (hangs inside bubblewrap) | `excludedCommands: ["docker"]` prevents the attempt |
| Agent escapes sandbox via unsandboxed fallback | `allowUnsandboxedCommands: false` — no escape hatch |

### What Sandbox Does NOT Prevent

| Risk | Why Sandbox Can't Help | Mitigation |
|------|----------------------|------------|
| Agent overwrites another teammate's files within same worktree | Sandbox CWD = worktree root; all teammates share that directory | **Crate ownership** — each teammate owns non-overlapping file paths (Layer 3) |
| Agent makes bad git commits | Git operations are allowed (needed for workflow) | **Code review at gates** — orchestrator reviews before merging at M1/M2/M3 |
| Agent pushes bad code to remote | Git push is allowed (network unrestricted) | **Branch protection** — worktree branches are not `main`; gate merges are reviewed |

### Crash Recovery

If bubblewrap kills an agent process (OOM, segfault, resource limit):

1. Team lead detects via `TeammateIdle` notification or missing progress
2. Lead spawns a replacement teammate with the same spawn prompt
3. New teammate picks up from the last committed state in git
4. Task is reassigned to the new teammate via `TaskUpdate`

This is identical to the existing crash recovery in [[#15. Agent Teams Limitations]] — sandbox crashes are indistinguishable from other crashes.

### Pre-Flight Sandbox Verification

Run these before launching the swarm:

```bash
# Verify bubblewrap is installed
bwrap --version

# Verify socat is installed (used by tmux teammate mode)
socat -V | head -1

# Test sandbox restricts writes outside CWD
claude --sandbox --print "touch /tmp/should-fail.txt"
# Expected: permission denied

# Test sandbox allows writes inside CWD
cd /tmp/test-sandbox && claude --sandbox --print "touch should-work.txt"
# Expected: success
```

---

## 17. Agent Audit Trail — Post-Mortem Breadcrumbs

Every agent leaves a structured trace. When things go sideways (and they will), you need to reconstruct exactly what happened, when, and why — without relying on agent memory or context windows.

### Log Directory Structure

Each worktree gets its own log directory:

```
.keel-swarm/logs/
├── agents/                    # Per-agent JSONL audit logs
│   ├── ts-resolver.jsonl
│   ├── py-resolver.jsonl
│   ├── go-resolver.jsonl
│   ├── rust-resolver.jsonl
│   ├── enforcement-engine.jsonl
│   ├── cli-commands.jsonl
│   ├── output-formats.jsonl
│   ├── tool-integration.jsonl
│   ├── mcp-server.jsonl
│   ├── vscode-ext.jsonl
│   ├── distribution.jsonl
│   ├── lead-a.jsonl
│   ├── lead-b.jsonl
│   ├── lead-c.jsonl
│   └── orchestrator.jsonl
├── escalations/               # Escalation events (5/8/15 threshold)
│   └── YYYY-MM-DD.jsonl
├── gates/                     # Gate check attempts and results
│   └── YYYY-MM-DD.jsonl
└── aggregated/                # Orchestrator-produced summaries
    └── daily-YYYY-MM-DD.json
```

### Hook-Driven Logging (Automatic)

Claude Code's hook system fires on every tool use. This gives you the breadcrumb trace without agents having to manually log anything.

**`.claude/hooks.json` — audit hooks (add to each worktree):**

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"pre_tool\",\"agent\":\"'$CLAUDE_AGENT_NAME'\",\"tool\":\"'$TOOL_NAME'\",\"args_summary\":\"'$(echo $TOOL_ARGS | head -c 200)'\"}' >> .keel-swarm/logs/agents/${CLAUDE_AGENT_NAME:-unknown}.jsonl",
        "description": "Audit log: pre-tool breadcrumb"
      }
    ],
    "PostToolUse": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"post_tool\",\"agent\":\"'$CLAUDE_AGENT_NAME'\",\"tool\":\"'$TOOL_NAME'\",\"exit_code\":'${EXIT_CODE:-0}',\"duration_ms\":'${DURATION_MS:-0}'}' >> .keel-swarm/logs/agents/${CLAUDE_AGENT_NAME:-unknown}.jsonl",
        "description": "Audit log: post-tool result"
      }
    ],
    "TeammateIdle": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"teammate_idle\",\"agent\":\"'$TEAMMATE_NAME'\",\"reason\":\"'$IDLE_REASON'\"}' >> .keel-swarm/logs/agents/${TEAMMATE_NAME:-unknown}.jsonl",
        "description": "Audit log: teammate idle event for escalation tracking"
      }
    ],
    "TaskCompleted": [
      {
        "command": "echo '{\"ts\":\"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'\",\"event\":\"task_completed\",\"agent\":\"'$TEAMMATE_NAME'\",\"task_id\":\"'$TASK_ID'\"}' >> .keel-swarm/logs/agents/${TEAMMATE_NAME:-unknown}.jsonl",
        "description": "Audit log: task completion event"
      }
    ]
  }
}
```

### Git-Driven Logging (Convention)

Every commit message follows a structured convention for grep-able history:

```
[agent-name][spec-NNN] action: description

Examples:
[ts-resolver][spec-002] feat: implement Oxc barrel file resolution
[enforcement-engine][spec-006] fix: circuit breaker counter not resetting on success
[lead-a][gate] check: M1 TypeScript precision at 83% (target 85%)
[orchestrator][gate] pass: M1 all criteria met, merging foundation -> enforcement
[py-resolver][spec-003] ESCALATE: ty subprocess returns invalid JSON on FastAPI test
```

**Convention rules:**
- `[agent-name]` — always first bracket
- `[spec-NNN]` or `[gate]` or `[infra]` — scope
- Action prefix: `feat:`, `fix:`, `test:`, `refactor:`, `check:`, `pass:`, `fail:`, `ESCALATE:`
- `ESCALATE:` in caps = human attention needed (grep for these first in post-mortem)

### Orchestrator Aggregation

During each `/ralph-loop` cycle, the orchestrator:

1. Reads `.keel-swarm/logs/agents/*.jsonl` from all 3 worktrees
2. Counts events per agent: tool calls, failures, idle events, task completions
3. Detects patterns: agent with >5 consecutive `exit_code != 0`, idle events without task completions, escalation-worthy stalls
4. Writes daily summary to `.keel-swarm/logs/aggregated/daily-YYYY-MM-DD.json`:

```json
{
  "date": "2026-03-15",
  "phase": 1,
  "agents": {
    "ts-resolver": {
      "tool_calls": 847,
      "failures": 23,
      "tasks_completed": 3,
      "idle_events": 12,
      "consecutive_failures_max": 4,
      "last_activity": "2026-03-15T18:42:00Z"
    }
  },
  "escalations": [],
  "gate_checks": [
    {"gate": "M1", "result": "fail", "blocking": "rust_precision: 72%"}
  ]
}
```

### Post-Mortem Analysis

When debugging what went wrong, use these patterns:

```bash
# What was agent X doing in the last hour?
jq 'select(.ts > "2026-03-15T17:00:00Z")' .keel-swarm/logs/agents/ts-resolver.jsonl

# Find all escalation events across all agents
grep '"ESCALATE"' .keel-swarm/logs/agents/*.jsonl

# Find all gate check failures
jq 'select(.event == "gate_check" and .result == "fail")' .keel-swarm/logs/gates/*.jsonl

# Git log filtered by agent
git log --oneline --grep='\[ts-resolver\]'

# Git log filtered by escalations
git log --oneline --grep='ESCALATE'

# Find the moment an agent started spinning (consecutive failures)
jq 'select(.event == "post_tool" and .exit_code != 0)' .keel-swarm/logs/agents/rust-resolver.jsonl | head -20

# Daily summary: which agents made progress?
jq '.agents | to_entries[] | select(.value.tasks_completed > 0) | .key' .keel-swarm/logs/aggregated/daily-*.json

# Count tool calls per agent (who's burning the most budget?)
wc -l .keel-swarm/logs/agents/*.jsonl | sort -rn
```

### Log Rotation

JSONL files grow indefinitely. At each phase gate:
1. Orchestrator compresses current logs: `gzip .keel-swarm/logs/agents/*.jsonl`
2. Archives to `.keel-swarm/logs/archive/phase-N/`
3. Fresh JSONL files start for next phase

This keeps per-phase logs manageable and provides clean breakpoints for post-mortem analysis.

---

## 18. Verification Checklist (Post-Build)

After all phases complete, verify:

- [ ] Every spec's acceptance criteria passes
- [ ] Resolution precision >85% per language (Gate M1)
- [ ] Mutation testing >95% true positive, <5% false positive (Gate M2)
- [ ] All performance benchmarks pass (Gate M2)
- [ ] JSON schemas validate on all outputs (Gate M2)
- [ ] E2E with Claude Code + Cursor works (Gate M3)
- [ ] Cross-platform builds succeed (Gate M3)
- [ ] `keel init` on real repos generates correct multi-tool configs
- [ ] `keel deinit` cleanly removes all generated files
- [ ] Binary size <40MB
- [ ] No runtime dependencies (single binary works on fresh system)

---

## Related Documents

- [[design-principles|Design Principles]] — the "why" document
- [[constitution|Constitution]] — non-negotiable articles
- [[keel-speckit/test-harness/strategy|Test Harness Strategy]] — oracle definitions and corpus
- [[CLAUDE|CLAUDE.md]] — agent implementation guide
- [[PRD_1|PRD v2.1]] — master source document (agents should NOT read this — use specs instead)
