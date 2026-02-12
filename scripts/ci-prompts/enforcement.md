# Agent Prompt: Enforcement & CLI Tests — Round 2

You are the **Enforcement agent** for the keel project. Round 1 implemented 154 enforcement tests + 29 server tests + 50 output tests. Round 2 implements **real assertions** in 133 remaining empty stubs.

## Your Mission

1. Implement real assertions in 53 CLI command test stubs
2. Implement real assertions in 49 tool integration test stubs
3. Convert 31 benchmark stubs to real benchmarks

## Setup

Create an agent team with 3 roles:
- **Coder** — implements test bodies with real assertions
- **Architect** — ensures CLI tests exercise actual binary behavior
- **Devil's advocate** — verifies tests actually detect regressions

Then run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 478 tests (enforcement, output, server, contracts, core, resolution)
**Your targets (all `#[ignore = "Not yet implemented"]` with empty bodies):**

| Directory | Ignored | What they test |
|-----------|---------|---------------|
| tests/cli/ | 53 | CLI commands: init, map, compile, discover, where, explain, stats, deinit, exit codes |
| tests/tool_integration/ | 49 | IDE hooks: Cursor, Claude Code, Git, Gemini, instruction files, hook execution |
| tests/benchmarks/ | 31 | Performance: hash, sqlite, compile, parsing, map, discover |

## How To Implement Tests

### Pattern: CLI Tests (tests/cli/)

CLI tests invoke the keel binary and check stdout/stderr/exit codes. Use helpers from `tests/common/mod.rs`.

```rust
use std::process::Command;

#[test]
fn test_compile_all_changed() {
    let (dir, _) = setup_test_project("typescript");
    // Write files
    std::fs::write(dir.path().join("src/a.ts"), "export function foo(): void {}");

    // Run keel init + map first
    let bin = keel_bin();
    Command::new(&bin).args(["init"]).current_dir(dir.path()).output().unwrap();
    Command::new(&bin).args(["map"]).current_dir(dir.path()).output().unwrap();

    // Modify a file
    std::fs::write(dir.path().join("src/a.ts"), "export function foo(x: number): void {}");

    // Run compile
    let output = Command::new(&bin).args(["compile"]).current_dir(dir.path()).output().unwrap();

    // Assert behavior
    assert!(output.status.success() || output.status.code() == Some(1));
    // Exit 0 = clean, Exit 1 = violations found
}
```

**Available CLI commands to test:** init, map, compile, discover, where, explain, stats, deinit

**Exit codes:** 0 = success, 1 = violations, 2 = internal error

### Pattern: Tool Integration Tests (tests/tool_integration/)

These test hook/config generation for IDE integrations.

```rust
#[test]
fn test_cursor_hooks_json_generation() {
    let (dir, _) = setup_test_project("typescript");
    let bin = keel_bin();

    // Initialize the project
    Command::new(&bin).args(["init"]).current_dir(dir.path()).output().unwrap();

    // Check that integration files are generated
    let hooks_path = dir.path().join(".cursor/hooks.json");
    // If keel init doesn't generate cursor hooks, test the generation command
    // Read the spec at keel-speckit/009-tool-integration/spec.md for details
}
```

**Key integrations:**
- Cursor: `.cursor/hooks.json` + `.cursor/rules/keel.mdc`
- Claude Code: `.claude/CLAUDE.md` rules
- Git: `.git/hooks/pre-commit`
- Gemini: `.gemini/settings.json`

### Pattern: Benchmark Tests (tests/benchmarks/)

Convert from `#[ignore]` stubs to real timing assertions:

```rust
#[test]
fn bench_hash_computation_under_1ms() {
    use std::time::Instant;

    let source = "fn example(x: i32) -> i32 { x + 1 }";
    let start = Instant::now();
    for _ in 0..1000 {
        // Compute hash using keel's hash function
        let _hash = keel_core::hash::compute_hash(source);
    }
    let elapsed = start.elapsed();

    // 1000 hashes should complete in under 100ms (0.1ms per hash)
    assert!(elapsed.as_millis() < 100, "hash too slow: {:?}", elapsed);
}
```

## Workflow

1. Read `tests/common/mod.rs` helpers (especially `keel_bin()` and `setup_test_project()`)
2. Read `crates/keel-cli/src/` to understand command implementations
3. Start with tests/cli/ (P0) — implement by command: init, then map, then compile, etc.
4. Move to tests/tool_integration/ (P1) — check keel-speckit/009 for integration specs
5. Then tests/benchmarks/ (P2) — convert to real timing tests
6. Check keel-speckit/007-cli-commands/spec.md for expected behaviors

## Constraints

- Max 15 files per session
- Remove `#[ignore = "Not yet implemented"]` when implementing a test
- Every test must have at least one `assert!` / `assert_eq!`
- Do NOT modify code in `crates/` — only `tests/`
- If a test reveals a bug, add `// BUG: <description>` and keep test as `#[ignore]`
- Commit after each command's tests are implemented
- Run `cargo test --workspace` after every batch of changes
- CLI tests must build keel first: use `keel_bin()` helper
- Keep files under 400 lines
