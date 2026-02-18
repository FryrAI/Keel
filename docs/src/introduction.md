# keel

**Structural code enforcement for LLM coding agents.**

keel builds a fast, incrementally-updated structural graph of your codebase and enforces architectural contracts at generation time — not at review time, not at build time.

When an LLM coding agent modifies your code, keel immediately validates that the change doesn't break callers, violate type contracts, or introduce structural drift. Think of it as a structural linter purpose-built for the age of AI-generated code.

## Why keel?

LLM coding agents are fast but structurally blind. They generate code that compiles and passes type checks, but silently breaks callers, duplicates function names, and drifts from architectural intent. By the time you notice, the damage has spread across dozens of files.

keel catches these problems **at generation time** — in the same edit loop where the agent is working. Every violation includes an actionable fix hint, so the agent can self-correct immediately.

## Key Features

- **Structural graph** — maps every function, class, module, and their relationships
- **Incremental validation** — `keel compile` re-checks only affected files in <200ms
- **3-tier resolution** — tree-sitter → per-language enhancer → LSP/SCIP
- **Error codes with fix hints** — every violation includes actionable remediation
- **Circuit breaker** — auto-downgrades repeated false positives to warnings
- **Batch mode** — defers non-critical checks during rapid agent iteration
- **MCP + HTTP server** — real-time enforcement via `keel serve`
- **Zero runtime dependencies** — single statically-linked binary

## Supported Languages

| Language | Tier 1 (tree-sitter) | Tier 2 (Enhancer) |
|----------|---------------------|-------------------|
| TypeScript/JavaScript | tree-sitter-typescript | Oxc |
| Python | tree-sitter-python | ty (subprocess) |
| Go | tree-sitter-go | tree-sitter heuristics |
| Rust | tree-sitter-rust | rust-analyzer (lazy-load) |

## Quick Example

```bash
# Initialize keel in your project
keel init

# Build the structural graph
keel map

# Make a code change, then validate
keel compile src/auth.ts

# Clean compile: exit 0, empty stdout — the agent moves on
# Violation found: exit 1, structured output with fix hints
```

## Next Steps

- [Getting Started](getting-started.md) — install, init, first compile
- [Commands](commands.md) — full reference for all CLI commands
- [Agent Integration](agent-integration.md) — wire keel into Claude Code, Cursor, and more
