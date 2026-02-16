# Agent Prompt: Bug Finding & Fixing — Round 3

You are the **Bug-fixing agent** for the keel project. O(n^2) perf was fixed in Rounds 1-2. Round 3 focuses on **code quality**, **feature gaps**, and **robustness**.

## Your Mission

1. Fix any currently failing tests
2. Implement features to un-ignore blocked tests
3. Fix `// BUG:` markers left by other agents
4. Keep clippy at 0 warnings
5. Split files approaching 400-line limit

## Setup

Run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 910 tests, **0 failures**, 93 ignored, 0 clippy warnings
**Performance:** O(n^2) compile fixed. SQLite WAL + pre-fetch optimizations applied.

## Priority Order

### P0: Fix any failing tests

Run `cargo test --workspace` and fix any failures immediately.

### P1: Un-ignore tests by implementing features

Run `cargo test --workspace -- --include-ignored` to find ignored tests that fail.
For each: fix the underlying code (not just the test), then remove `#[ignore]`.

Focus areas for feature implementation:
- `ModuleProfile` — missing `insert_module_profile` public API
- Schema v2 migration — `apply_migration_v2()` not implemented
- Module auto-creation — parser doesn't auto-create Module nodes per file

### P2: BUG Markers

Search for `// BUG:` comments in source code (not test annotations):
```bash
grep -rn '// BUG:' crates/ tests/ --include='*.rs' | grep -v '#\[ignore'
```

### P3: Clippy + Code Quality

```bash
cargo clippy --workspace -- -D warnings
find crates/ tests/ -name '*.rs' -exec wc -l {} \; | sort -rn | awk '$1 > 380'
```

### P4: Robustness

- Check error handling paths in `crates/keel-cli/src/`
- Verify graceful degradation when `ty` or `rust-analyzer` are unavailable

## Constraints

- Max 15 files per session
- Commit after each bug fix with descriptive message
- Do NOT modify frozen contracts (LanguageResolver, GraphStore traits)
- Do NOT break existing 910 passing tests
- Performance fixes MUST include before/after measurements in commit message
- Run `cargo test --workspace` after every fix
- If you can't fix a bug safely, add a `// BUG:` comment and move on
