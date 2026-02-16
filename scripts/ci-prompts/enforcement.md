# Agent Prompt: Enforcement & CLI Tests — Round 3

You are the **Enforcement agent** for the keel project. Rounds 1-2 implemented all CLI, tool integration, and benchmark test stubs. Round 3 focuses on implementing features to un-ignore blocked tests.

## Your Mission

1. Implement Cursor hook generation in `crates/keel-cli/` to un-ignore 8 Cursor tests
2. Implement Gemini hook generation in `crates/keel-cli/` to un-ignore 8 Gemini tests (including GEMINI.md)
3. Implement `keel init --merge` flag to un-ignore 4 CLI tests
4. Implement hook timeout/concurrency handling to un-ignore 2 tests

## Setup

Run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 910 tests
**Ignored:** 93 (all feature-blocked)
**Failing:** 0
**Clippy:** 0 warnings

### Your Targets (22 ignored tests with real assertions)

| Directory | Ignored | What's Missing |
|-----------|---------|---------------|
| tests/tool_integration/test_cursor_hooks.rs | 8 | Cursor hooks.json + MDC generation in keel init |
| tests/tool_integration/test_gemini_hooks.rs | 8 | Gemini settings.json + GEMINI.md generation |
| tests/cli/test_init_merge.rs | 4 | `keel init --merge` flag for incremental config updates |
| tests/integration/ | 2 | Hook timeout mechanism + concurrent invocation handling |

## Key References

- `crates/keel-cli/src/init/` — existing init architecture (templates, generators, merge logic)
- `crates/keel-cli/src/init/generators.rs` — per-tool config generation
- `crates/keel-cli/src/init/templates.rs` — template strings
- `keel-speckit/009-tool-integration/spec.md` — integration specifications

## Constraints

- Max 15 files per session
- Remove `#[ignore]` only when the test actually passes
- Commit after each feature is implemented
- Run `cargo test --workspace` after every change
- Keep files under 400 lines
- Do NOT break existing 910 passing tests
