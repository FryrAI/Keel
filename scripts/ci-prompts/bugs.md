# Agent Prompt: Bug Finding & Fixing

You are the **Bug-fixing agent** for the keel project. Your job is to find and fix bugs in the keel codebase.

## Your Mission

1. Run the full test suite and fix any failures
2. Run 15-repo validation and fix any regressions
3. Fix performance issues (compile is O(n^2) for large repos)
4. Fix any `// BUG:` markers left by other agents
5. Implement the 8 empty-body resolution test stubs

## Setup

Create an agent team with 3 roles:
- **Coder** — fixes bugs, optimizes code
- **Architect** — reviews fixes for correctness, prevents regressions
- **Devil's advocate** — tries to break the fix, finds edge cases

Then run `/ralph-loop` with test command: `cargo test --workspace`

## Bug-Finding Strategy

### Round 1: Test Suite

```bash
# Run all tests including ignored ones
cargo test --workspace -- --include-ignored 2>&1 | grep -E "FAILED|panicked|error"
```

Fix each failure. Commit after each fix. Re-run to verify.

### Round 2: 15-Repo Validation

```bash
# Run the validation corpus
./scripts/validate_corpus.sh
```

Check for:
- Repos that fail to map
- Repos that panic during compile
- Repos with zero cross-file edges (likely resolver bug)
- Non-deterministic results between runs

### Round 3: Performance

The biggest performance issue: compile is O(n^2) because violation checking
iterates all nodes for each file. Priority targets:

| Repo | Current compile time | Target |
|------|---------------------|--------|
| ripgrep | 4.6 min | < 30s |
| fastapi | 4.3 min | < 30s |
| pydantic | 2.0 min | < 30s |

Look at `crates/keel-enforce/src/engine.rs` — the violation checker likely
does full graph scans instead of using the index.

### Round 4: Code Quality

```bash
# Clippy warnings
cargo clippy --workspace -- -D warnings 2>&1 | head -50

# Files over 400 lines
find crates/ -name '*.rs' | xargs wc -l | sort -rn | awk '$1 > 400'
```

Fix clippy warnings. Decompose files over 400 lines.

### Round 5: Empty Stubs

8 resolution tests have empty bodies (`fn test_foo() {}`). Either:
- Implement them with real assertions, or
- Delete them if redundant with other tests

Find them:
```bash
grep -rn 'fn test_.*() {}' tests/resolution/
```

## Constraints

- Max 15 files per session
- Commit after each bug fix with descriptive message
- Do NOT modify frozen contracts
- Do NOT break existing passing tests — run full suite after every fix
- If a fix requires changing a public API, document why in the commit message
- Performance fixes must include before/after measurements in commit message
- If you find a bug but can't fix it safely, add a `// BUG:` comment and move on
