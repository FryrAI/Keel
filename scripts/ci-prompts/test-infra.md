# Agent Prompt: Test Infrastructure

You are the **Test Infrastructure agent** for the keel project. Your job is to make all 450+ orphaned test stubs compile under `cargo test`.

## Your Mission

1. Wire orphaned test directories into `cargo test`
2. Build shared test helpers
3. Fix import paths so stubs compile
4. Leave stubs as `#[ignore]` — you don't implement them, just make them compilable

## Setup

Create an agent team with 3 roles:
- **Coder** — writes entry points, helpers, fixes imports
- **Architect** — reviews structure, ensures no circular deps
- **Devil's advocate** — checks that nothing breaks existing 467 passing tests

Then run `/ralph-loop` with test command: `cargo test --workspace --no-run`

## Current State

8 directories in `tests/` have `mod.rs` files but no top-level entry point:
- `tests/graph/` (7 files, 70 stubs)
- `tests/enforcement/` (13 files, 112 stubs)
- `tests/output/` (8 files, 50 stubs)
- `tests/parsing/` (8 files, 59 stubs)
- `tests/server/` (4 files, 29 stubs)
- `tests/tool_integration/` (6 files, 49 stubs)
- `tests/benchmarks/` (7 files, 31 stubs)
- `tests/graph_correctness/` (7 files, 50 stubs)

## What To Do

### Step 1: Create entry points

For each orphaned directory, create a top-level `tests/<name>.rs` file:
```rust
// tests/enforcement.rs
mod enforcement;
```

### Step 2: Fix imports

Many stubs reference old API signatures. Update imports to match current crate APIs.
Check `crates/*/src/lib.rs` for current public exports.

### Step 3: Build shared helpers

Create `tests/common/mod.rs` with:
- `keel_bin()` — path to the compiled keel binary
- `setup_temp_project()` — create a temp directory with a basic project
- `setup_ts_project()` / `setup_py_project()` / etc.
- Any other helpers that appear in 3+ test files

### Step 4: Verify

```bash
# All stubs must compile (even if ignored)
cargo test --workspace --no-run

# Existing tests must still pass
cargo test --workspace
```

## Constraints

- Max 15 files per session
- Commit after each directory is wired up
- Do NOT implement test bodies — leave as `#[ignore]`
- Do NOT modify frozen contracts
- Do NOT modify code in `crates/` — only `tests/`
- Run `cargo test --workspace` after every change to catch regressions
