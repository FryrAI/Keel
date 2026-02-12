# Agent Prompt: Test Infrastructure — Round 2

You are the **Test Infrastructure agent** for the keel project. Round 1 wired all test directories and created helpers. Round 2 implements **real assertions** in 187 empty stubs.

## Your Mission

1. Implement real assertions in 179 ignored test stubs (graph, parsing, graph_correctness)
2. Implement 8 empty-body resolution stubs (tests/resolution/typescript/)
3. Decompose 2 files over 400 lines

## Setup

Create an agent team with 3 roles:
- **Coder** — implements test bodies with real Rust assertions
- **Architect** — ensures tests exercise actual keel-core/keel-parsers APIs
- **Devil's advocate** — verifies assertions are meaningful (not tautological)

Then run `/ralph-loop` with test command: `cargo test --workspace`

## Current State

**Passing:** 478 tests across enforcement(154), output(50), server(29), contracts(66), resolution(~50), core(~130)
**Ignored (empty stubs):** 318 tests — all have GIVEN/WHEN/THEN pseudocode but NO actual code

### Breakdown by directory

| Directory | Ignored | Priority |
|-----------|---------|----------|
| tests/graph/ | 70 | P0 |
| tests/parsing/ | 59 | P0 |
| tests/graph_correctness/ | 50 | P1 |
| tests/resolution/typescript/ | 8 empty | P1 |

### Files over 400 lines (decompose)

| File | Lines |
|------|-------|
| tests/contracts/test_json_schema_contract.rs | 467 |
| tests/integration/test_multi_language.rs | 407 |

## How To Implement Tests

### Pattern: Graph Tests (tests/graph/)

These test keel-core types and storage. Read `crates/keel-core/src/lib.rs` exports first.

```rust
#[test]
fn test_create_function_node() {
    // The stub has: GIVEN/WHEN/THEN pseudocode
    // Implement with actual keel_core types:
    use keel_core::types::{GraphNode, NodeKind};

    let node = GraphNode {
        hash: "abc123".into(),
        kind: NodeKind::Function,
        name: "my_func".into(),
        file_path: "src/lib.rs".into(),
        line: 10,
        // ... fill in required fields from the struct definition
    };
    assert_eq!(node.kind, NodeKind::Function);
    assert_eq!(node.name, "my_func");
    assert!(!node.hash.is_empty());
}
```

**Key:** Read `crates/keel-core/src/types.rs` to see the actual struct fields. Don't guess.

### Pattern: Parsing Tests (tests/parsing/)

These test the tree-sitter parsers. Use the LanguageResolver trait.

```rust
#[test]
fn test_typescript_parser_extracts_functions() {
    use keel_parsers::typescript::TsResolver;
    use keel_parsers::resolver::LanguageResolver;
    use std::path::Path;

    let resolver = TsResolver::new();
    let source = "export function greet(name: string): string { return name; }";
    let result = resolver.parse_file(Path::new("test.ts"), source);

    assert!(!result.definitions.is_empty());
    assert!(result.definitions.iter().any(|d| d.name == "greet"));
}
```

### Pattern: Graph Correctness Tests (tests/graph_correctness/)

These validate end-to-end graph building. Use the helpers in `tests/common/mod.rs`.

```rust
#[test]
fn test_typescript_function_definitions_complete() {
    let (dir, _path) = setup_test_project("typescript");
    // Write TypeScript files with known functions
    std::fs::write(dir.path().join("src/utils.ts"), "export function foo(): void {}\nexport function bar(): void {}");
    // Run keel map equivalent
    let store = create_mapped_project(&[("src/utils.ts", "export function foo(): void {}\nexport function bar(): void {}")]);
    // Query the graph and assert completeness
    // ...
}
```

### Pattern: Resolution Stubs (8 empty bodies)

Located in `tests/resolution/typescript/`:
- `test_barrel_files.rs`: `test_barrel_star_export`, `test_barrel_circular_detection`
- `test_re_exports.rs`: `test_star_reexport`, `test_star_reexport_name_collision`, `test_namespace_reexport`, `test_reexport_from_external_package`
- `test_path_aliases.rs`: `test_path_alias_tsconfig_extends`, `test_path_alias_uses_oxc_resolver`

Follow the pattern of adjacent implemented tests in the same file.

## Workflow

1. Read `crates/keel-core/src/lib.rs` and `crates/keel-parsers/src/lib.rs` to understand APIs
2. Read `tests/common/mod.rs` for available helpers
3. Start with tests/graph/ (P0) — implement 5-10 tests, commit, run suite
4. Move to tests/parsing/ (P0) — same pattern
5. Then tests/graph_correctness/ (P1)
6. Then resolution stubs (P1)
7. Decompose the 2 over-400-line files last

## Constraints

- Max 15 files per session
- Remove `#[ignore = "Not yet implemented"]` when implementing a test
- Every test must have at least one `assert!` / `assert_eq!` / `assert_ne!`
- Do NOT modify code in `crates/` — only `tests/`
- If a test reveals a bug, add `// BUG: <description>` and keep test as `#[ignore]`
- Commit after each file of tests is implemented
- Run `cargo test --workspace` after every batch of changes
- Keep files under 400 lines
