# Phases, Contracts & Gate Criteria

```yaml
tags: [keel, agent-swarm, phases, contracts, gates]
status: completed
completed: 2026-02-10
```

> **All phases are governed by [scope-limits.md](scope-limits.md).**
> Every session — orchestrator, lead, teammate — must respect scope limits.

---

## 1. Inter-Agent Contracts (Frozen in Phase 0)

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

## 2. Phase 0: Scaffold (~12-24 Hours)

> **No agent teams yet.** Uses 4 sandboxed tmux panes for parallel scaffold work, coordinated via git commits. Each pane runs an independent Claude session with bounded scope (max 15 files per session). See [infrastructure.md — Phase 0 tmux Setup](infrastructure.md) for pane layout.

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

2. **Cargo.toml dependencies** matching [Constitution Article 1](../constitution.md)
3. **Graph schema Rust types** from [Spec 000](../keel-speckit/000-graph-schema/spec.md)
4. **SQLite schema** from [Spec 000](../keel-speckit/000-graph-schema/spec.md)
5. **`LanguageResolver` trait** (Contract 1) — with stub implementations for all 4 languages
6. **`GraphStore` trait** (Contract 2) — with SQLite implementation
7. **Result structs** (Contract 3) — `CompileResult`, `DiscoverResult`, `ExplainResult`
8. **JSON schemas** (Contract 4) — in `tests/schemas/`
9. **All ~98 test files** with `#[ignore]` annotations
10. **Contract test files** — validate traits compile and types match
11. **Mock graph fixtures** — pre-built `GraphStore` with known test data
12. **Mock compile output** — pre-built `CompileResult` fixtures
13. **Test corpus setup script** — `scripts/setup_test_repos.sh`
14. **Test harness scripts** — `test-fast.sh`, `test-full.sh`, oracle scripts
15. **Per-worktree CLAUDE.md files**
16. **CI workflow** — `.github/workflows/ci.yml`
17. **`.keelignore` template** with defaults
18. **`.keel/config.toml` template**
19. **JSON schema validation test**
20. **Gate marker directory** — `.keel-swarm/`

### Phase 0 Execution Model

Phase 0 uses **multiple tmux panes** with sandboxed Claude sessions, NOT Task tool subagents in one session. See [scope-limits.md](scope-limits.md) for why.

**Wave 1 — Structural files (4 parallel panes):**

| Pane | Group | Deliverables | Max Files | Dependencies |
|------|-------|-------------|-----------|--------------|
| 0 | A | Cargo workspace + Cargo.toml files | 7 | None |
| 1 | B | keel-core types + SQLite | 5 | Group A (git pull) |
| 2 | C | keel-parsers stubs | 6 | Group A (git pull) |
| 3 | D | keel-enforce/cli/output/server stubs | 8 | Group A (git pull) |

**Wave 2 — Test files + support (4 parallel panes, after Wave 1 commits):**

| Pane | Group | Deliverables | Max Files | Dependencies |
|------|-------|-------------|-----------|--------------|
| 0 | E1 | tests/graph/ + tests/parsing/ + tests/enforcement/ | ~30 | Wave 1 |
| 1 | E2 | tests/resolution/ + tests/cli/ + tests/output/ | ~35 | Wave 1 |
| 2 | E3 | tests/server/ + tests/benchmarks/ + tests/integration/ | ~33 | Wave 1 |
| 3 | F | schemas + fixtures + contracts + scripts + config | ~20 | Wave 1 |

**Note:** Wave 2 groups exceed 15 files per session. Split into sub-sessions of ≤15 files within the same pane.

### Human Checkpoint After Phase 0

> **Phase gate enforcement**: Worktree branches are created and teams are spawned only after this gate passes.

**Verify:** (All completed 2026-02-09)
- [x] `cargo check` passes for all crates
- [x] All 4 `LanguageResolver` stubs compile
- [x] `GraphStore` SQLite implementation passes basic CRUD tests
- [x] All test files exist and are ignored
- [x] Contract tests exist (even if skipped)
- [x] Mock fixtures load without errors
- N/A Test corpus repos — not needed (unit tests sufficed)
- [x] `test-fast.sh` runs and exits 0
- [x] Git worktrees created
- [x] Per-worktree CLAUDE.md files in place

---

## 3. Phase Sequencing

> All phases are governed by [scope-limits.md](scope-limits.md).
> Every session — orchestrator, lead, teammate — must respect scope limits.

```
Phase 0 (tmux panes):   Cargo scaffold, test harness, mock fixtures, contracts, tests
    -- HUMAN CHECKPOINT --

Phase 1 (3 Teams in Parallel):
  Foundation team = tree-sitter parsing + per-language resolution (specs 000-005)
  Enforcement team = enforcement engine against mock graph (spec 006) + CLI (spec 007)
  Surface team = tool templates + detection logic (spec 009) + MCP skeleton (spec 010)
    -- GATE M1: Resolution >85% precision per language --

Phase 2 (3 Teams in Parallel):
  Foundation team = performance tuning + edge cases + Tier 3 fallback stubs
  Enforcement team = CLI wiring with real graph + output formatters (spec 008)
  Surface team = hook flow with real compile + instruction files + CI templates
    -- GATE M2: All commands work, enforcement catches mutations (>95% TP) --

Phase 3 (3 Teams in Parallel):
  Foundation team = resolution edge cases + cross-language endpoint detection
  Enforcement team = batch mode + suppress mechanism + progressive adoption
  Surface team = VS Code extension (spec 011) + distribution (spec 012)
    -- GATE M3: E2E with Claude Code + Cursor on real repos --

Phase 4 (All):  Dogfooding — use keel to develop keel
```

---

## 4. Gate Criteria

### Phase Gate Enforcement

The orchestrator creates gate marker files only after gate criteria are met. Teams check for gate markers before advancing. Orchestrator uses `/tmux-observe` and git to verify criteria across all 3 worktrees.

**Cross-team coordination mechanism:**
1. Each team lead pushes results to their worktree branch
2. Orchestrator pulls all branches, runs cross-team checks
3. When gate criteria pass, orchestrator writes marker: `.keel-swarm/gate-m1-passed`
4. Team leads check for gate markers and advance to next phase

**Cross-team integration at gates:**
- **M1 gate:** Orchestrator merges `foundation` branch into `enforcement` branch
- **M2 gate:** Orchestrator merges `enforcement` branch into `surface` branch

### Gate M1 Criteria — PASSED (2026-02-10)

- [x] TypeScript resolution: 28 resolver tests passing
- [x] Python resolution: 29 resolver tests passing
- [x] Go resolution: 18 resolver tests passing
- [x] Rust resolution: 29 resolver tests passing
- [x] All 4 languages parse without panic — 153 resolver tests total, 0 failures
- [x] `keel init` functional
- [x] Graph node/edge counts validated via integration tests

### Gate M2 Criteria — PASSED (2026-02-10)

- [x] All CLI commands return valid output — 38 CLI arg parsing tests
- [x] Enforcement engine validates mutations — 16 enforcement tests
- [x] Circuit breaker: 3-attempt escalation verified
- [x] Clean compile: empty stdout + exit 0
- [x] Output formatters validated — 66 output format tests
- [x] JSON output validates via formatter tests

### Gate M3 Criteria — PASSED (2026-02-10)

- [x] Tool configs generated for 9+ tools (Claude Code, Cursor, Windsurf, Copilot, Aider, etc.)
- [x] MCP server: 5 tools respond correctly — 15 server integration tests
- [x] VS Code extension: status bar, diagnostics, CodeLens, hover
- [x] Release CI: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64

---

## 5. Task Dependency Graph Per Team

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

Gate M2: Enforcement must achieve >95% TP mutation rate
  -> Triggers: merge enforcement into surface branch

Gate M3: Surface must achieve E2E on real repos
  -> Triggers: all-team dogfooding phase
```

> **All phases and gates completed 2026-02-09 to 2026-02-10.** See [README.md Retrospective](README.md) for plan-vs-reality analysis.
