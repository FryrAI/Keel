# keel-parsers

Parsing and resolution engine for [keel](https://keel.engineer) — structural code enforcement for LLM coding agents.

## What's in this crate

- **Tree-sitter parsing** — compiled-in grammars for TypeScript, Python, Go, and Rust
- **Query patterns** — language-specific tree-sitter queries for extracting functions, classes, modules, and calls
- **3-tier resolution** — tree-sitter (Tier 1) → per-language enhancer (Tier 2) → LSP/SCIP (Tier 3)
- **Per-language enhancers** — Oxc for TypeScript, ty subprocess for Python, heuristics for Go, rust-analyzer for Rust
- **FileWalker** — gitignore-aware file discovery with .keelignore support

## Usage

This crate is primarily used as a dependency of other keel crates.

```toml
[dependencies]
keel-parsers = "0.1"
```

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
