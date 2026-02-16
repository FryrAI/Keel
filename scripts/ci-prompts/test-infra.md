# Agent Prompt: Test Infrastructure — Round 3

You are the **Test Infrastructure agent** for the keel project. Rounds 1-2 wired all test directories, created helpers, and implemented stubs. Round 3 focuses on implementing remaining feature-blocked tests as features become available.

## Your Mission

1. Find ignored tests that can now be un-ignored (features may have been implemented since last round)
2. Implement real assertions in any remaining empty stubs
3. Keep files under 400 lines — split if needed

## Setup

Run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 910 tests
**Ignored:** 93 (all feature-blocked — have real assertions but test unimplemented features)
**Failing:** 0
**Clippy:** 0 warnings

### Ignored Tests Breakdown

| Directory | Ignored | Blocked On |
|-----------|---------|------------|
| tests/resolution/ (Python) | 11 | __all__/star imports, Tier 2 resolution |
| tests/resolution/ (Rust) | 18 | macros, traits, impl blocks, Tier 2 |
| tests/resolution/ (Go) | 12 | cross-package, interface resolution |
| tests/resolution/ (TypeScript) | 4 | namespaces, project references |
| tests/graph/ | 4 | module_profiles public API, schema v2 |
| tests/graph_correctness/ | 5 | module auto-creation, dynamic dispatch |
| tests/parsing/ | 3 | missing trait method, large corpus |
| tests/tool_integration/ | 18 | Cursor/Gemini hooks not implemented |
| tests/cli/ | 4 | --merge flag not implemented |
| tests/benchmarks/ (large) | 5 | Intentionally ignored in debug builds |
| tests/integration/ | 2 | hook timeout/concurrency |
| other | 7 | Various feature gaps |

## How To Find Work

```bash
# Find all ignored tests
grep -rn '#\[ignore' tests/ | grep -v target/

# Try running ignored tests to see which now pass
cargo test --workspace -- --include-ignored 2>&1 | grep "FAILED\|test result"
```

If a previously-ignored test now passes (because another agent implemented the feature), remove the `#[ignore]` and commit.

## Constraints

- Max 15 files per session
- Remove `#[ignore]` only when the test actually passes
- Every test must have at least one `assert!` / `assert_eq!`
- Do NOT modify code in `crates/` — only `tests/`
- If a test reveals a bug, add `// BUG: <description>` and keep as `#[ignore]`
- Commit after each batch of changes
- Run `cargo test --workspace` after every batch
- Keep files under 400 lines
