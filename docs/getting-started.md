# Getting Started with keel

keel is a structural code enforcement tool for LLM coding agents. It builds a fast, incrementally-updated graph of your codebase and validates architectural contracts after every code change -- at generation time, not at review time.

This guide walks you through installation, initialization, and your first compile.

## Prerequisites

- **Rust 1.75+** (if building from source)
- A codebase in TypeScript, Python, Go, or Rust (or any combination)
- Git (recommended, for pre-commit hook integration)

## Installation

### From source (recommended during early access)

```bash
cargo install --path crates/keel-cli
```

### Install script

```bash
curl -fsSL https://keel.engineer/install.sh | bash
```

The script detects your OS and architecture, downloads the correct binary, and places it on your PATH.

### Homebrew (macOS/Linux)

```bash
brew tap FryrAI/keel
brew install keel
```

### Verify installation

```bash
keel --version
```

## Initialize keel in Your Project

Navigate to your project root and run:

```bash
cd your-project
keel init
```

This does the following:

1. **Detects languages** -- scans file extensions to find TypeScript, Python, Go, and Rust files.
2. **Creates `.keel/`** -- the configuration directory, including:
   - `keel.json` -- main configuration file
   - `graph.db` -- SQLite database for the structural graph
   - `cache/` -- incremental parsing cache
3. **Creates `.keelignore`** -- default ignore patterns for `node_modules/`, `__pycache__/`, `target/`, `dist/`, `build/`, `.next/`, `vendor/`, `.venv/`.
4. **Installs a git pre-commit hook** -- runs `keel compile` before each commit (skipped if a hook already exists).
5. **Detects AI tool directories** -- finds `.cursor/`, `.windsurf/`, `.aider/`, `.continue/` and logs what it sees.

After initialization:

```
keel initialized. 2 language(s) detected, 147 files indexed.

Next steps:
  keel map       Build the structural graph
  keel compile   Validate contracts
```

## Build the Structural Graph

```bash
keel map
```

This parses every source file using tree-sitter, builds resolution edges (calls, imports, contains), and stores the complete graph in `.keel/graph.db`. For a 100k LOC codebase, this takes under 5 seconds.

Use `--depth` to control output verbosity:

```bash
keel map --depth 0    # Summary only: file count, node count, edge count
keel map --depth 1    # Modules + hotspots (default)
keel map --depth 2    # Functions with signatures
keel map --depth 3    # Full graph detail
```

## Validate Your Code

After making changes, validate with:

```bash
keel compile src/auth.ts
```

### Clean compile

If there are no violations, keel exits with code `0` and produces **empty stdout**. This is by design -- when everything is fine, the LLM agent should see nothing and move on.

### Violations found

If violations are detected, keel exits with code `1` and outputs structured violation data:

```bash
keel compile src/auth.ts --json
```

```json
{
  "errors": [
    {
      "code": "E001",
      "message": "broken caller: login() calls authenticate() which no longer exists",
      "file": "src/auth.ts",
      "line": 42,
      "hash": "a7Bx3kM9f2Q",
      "fix_hint": "Update login() to use the new verifyCredentials() function"
    }
  ],
  "warnings": []
}
```

Every error includes a `fix_hint` with actionable remediation. Every violation includes a `confidence` score and `resolution_tier` indicating how the edge was resolved.

## Output Formats

keel supports three output formats via global flags:

| Flag | Format | Best for |
|------|--------|----------|
| `--json` | Structured JSON | Programmatic consumption, CI pipelines |
| `--llm` | Token-optimized text | LLM agents (minimal tokens, maximum signal) |
| (default) | Human-readable | Terminal use, debugging |

## Integration with AI Tools

When you run `keel init`, it detects which AI coding tools are present in your project and can generate configuration files for them. Supported tools include Claude Code, Cursor, Windsurf, Aider, and more.

The standard integration pattern is:

1. **On session start** -- run `keel map --llm` to give the agent structural context.
2. **After every file edit** -- run `keel compile <file> --llm` to validate the change.

See [Agent Integration Guide](agent-integration.md) for detailed setup instructions for each tool.

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success, no violations (or only warnings in non-strict mode) |
| `1` | Violations found |
| `2` | keel internal error (not initialized, database error, etc.) |

## Next Steps

- [Commands Reference](commands.md) -- full documentation for every keel command
- [Agent Integration](agent-integration.md) -- wiring keel into Claude Code, Cursor, Windsurf, and other tools
- [Configuration](config.md) -- customizing enforcement rules, circuit breaker, and batch mode
- [FAQ](faq.md) -- troubleshooting and common questions
