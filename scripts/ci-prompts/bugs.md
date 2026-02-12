# Agent Prompt: Bug Finding & Fixing — Round 2

You are the **Bug-fixing agent** for the keel project. Round 1 fixed 7 bugs. Round 2 focuses on **performance**, **corpus validation**, and **integration test failures**.

## Your Mission

1. Clone test corpus (FIRST — required for everything else)
2. Build release binary
3. Fix the O(n^2) compile performance bottleneck
4. Run 15-repo corpus validation and fix regressions
5. Fix 6 integration test stubs
6. Fix any `// BUG:` markers left by other agents

## Setup

**CRITICAL — Do this BEFORE creating agent team:**
```bash
bash scripts/setup_test_repos.sh /tmp/claude/test-corpus
cargo build --release
```

Then create an agent team with 3 roles:
- **Coder** — fixes bugs, optimizes code
- **Architect** — reviews fixes for correctness, prevents regressions
- **Devil's advocate** — tries to break the fix, finds edge cases

Then run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 478 tests, **0 failures**, clippy clean
**Performance issue:** `test_compile_single_file_in_large_project_under_200ms` takes **62 seconds** (target: <200ms)
**Integration stubs:** 6 in tests/integration/ (5 passed when forced, 1 is the perf test)

## Priority Order

### P0: Performance Fix (O(n^2) compile)

The biggest issue. One integration test proves it:
```
test_large_codebase::test_compile_single_file_in_large_project_under_200ms
→ Actually takes 62 seconds
```

**Investigation path:**
1. Profile with `cargo test --workspace -- --ignored test_compile_single_file --nocapture 2>&1`
2. Look at `crates/keel-enforce/src/engine.rs` — the violation checker
3. Look at `crates/keel-enforce/src/` for nested loops over graph nodes
4. Check if `keel compile <single-file>` scans ALL nodes instead of using file index
5. Check SQLite query patterns — are they using indexes properly?

**Expected fix pattern:**
- Build file → nodes index during `keel map`
- On `keel compile file.ts`, look up only nodes in that file + direct callers
- Don't scan the entire graph for each file

**Performance targets:**

| Repo | Current | Target |
|------|---------|--------|
| Single file compile | 62s | < 200ms |
| ripgrep full map | unknown | < 5s |
| fastapi full map | unknown | < 5s |

### P1: Corpus Validation

```bash
# After building release binary and cloning corpus:
./scripts/validate_corpus.sh /tmp/claude/test-corpus
```

Check for:
- Repos that fail to `keel init` or `keel map`
- Repos that panic during `keel compile`
- Repos with zero cross-file edges (resolver bug)
- Non-deterministic results between runs (run twice, diff)

### P2: Integration Test Stubs

6 tests in tests/integration/:
- `test_large_codebase.rs` — 5 stubs about large project behavior + 1 perf test
- These likely need the perf fix before they can pass within time limits

### P3: BUG Markers

Search for `// BUG:` comments left by other agents:
```bash
grep -rn '// BUG:' crates/ tests/
```

### P4: Code Quality

```bash
# Files approaching 400-line limit
find crates/ -name '*.rs' -exec wc -l {} \; | sort -rn | awk '$1 > 380'
```

Proactively split any files at risk of exceeding 400 lines.

## Constraints

- Max 15 files per session
- Commit after each bug fix with descriptive message
- Do NOT modify frozen contracts (LanguageResolver, GraphStore traits)
- Do NOT break existing 478 passing tests
- Performance fixes MUST include before/after measurements in commit message
- If a fix changes a public API, document why in commit message
- Run `cargo test --workspace` after every fix
- If you can't fix a bug safely, add a `// BUG:` comment and move on
