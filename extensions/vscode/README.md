# Keel for Visual Studio Code

**Structural code enforcement for LLM coding agents** -- inline diagnostics, CodeLens, and graph context directly in your editor.

## Features

### Compile-on-Save

Every time you save a supported file (`.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.go`, `.rs`), keel automatically compiles and shows violations as native VS Code diagnostics. Errors and warnings appear in the Problems panel and inline in the editor, complete with error codes and fix hints.

### CodeLens (Caller/Callee Counts)

Function and method declarations display a CodeLens annotation showing the number of callers and callees:

```
↑3 ↓2        ← 3 callers, 2 callees
function authenticate(user: User): boolean {
```

Click the annotation to run `Keel: Discover` and see the full adjacency graph for that symbol.

### Hover Information

Hover over any function or class name to see its keel hash, caller/callee summary, and module context. This gives you instant graph awareness without leaving the editor.

### Inline Diagnostics

Violations from `keel compile` appear as native VS Code errors and warnings. Each diagnostic includes:
- The keel error code (E001-E005, W001-W002)
- A human-readable message
- A fix hint when available

### Server Lifecycle

The extension manages the `keel serve --http --watch` process automatically:
- Starts the server when a workspace with `.keel/` is opened (configurable)
- Health-checks every 10 seconds
- Shows server status in the status bar
- Gracefully stops the server on deactivation

### Status Bar

The status bar shows the current state at a glance:
- `keel check` -- clean compile, no violations
- `keel warning N` -- N warnings
- `keel error N` -- N errors
- `keel ?` -- server not connected

## Requirements

- **keel binary** must be installed and available on your `PATH` (or configured via `keel.binaryPath`).
- A project initialized with `keel init` (the `.keel/` directory must exist).

Install keel:

```bash
# From source
cargo install --path crates/keel-cli

# Or via the install script
curl -fsSL https://keel.engineer/install.sh | bash
```

## Quick Start

1. Install the extension from the VS Code Marketplace.
2. Open a project that has been initialized with `keel init`.
3. The extension detects `.keel/` and automatically starts the keel server.
4. Edit and save a file -- diagnostics appear immediately.

No additional configuration is required for the default workflow.

## Extension Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `keel.binaryPath` | `string` | `"keel"` | Path to the keel binary. Use an absolute path if keel is not on your PATH. |
| `keel.compileOnSave` | `boolean` | `true` | Run `keel compile` automatically when a supported file is saved. |
| `keel.autoStartServer` | `boolean` | `true` | Auto-start `keel serve --http --watch` when a workspace with `.keel/` is activated. |
| `keel.serverPort` | `number` | `4815` | Port for the keel HTTP server. Must match the port used by `keel serve`. |

## Commands

Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) and type "Keel" to see all available commands:

| Command | Description |
|---------|-------------|
| `Keel: Compile` | Run a full compile of the workspace |
| `Keel: Discover` | Look up callers, callees, and context for the symbol at cursor (or enter a hash) |
| `Keel: Where (locate hash)` | Resolve a keel hash to its file and line, then open the file |
| `Keel: Show Map` | Display the structural map of the codebase in LLM format |
| `Keel: Start Server` | Manually start the keel HTTP server |
| `Keel: Stop Server` | Stop the keel HTTP server managed by this extension |
| `Keel: Show Output` | Open the Keel output channel to see server logs and compile results |

## Supported Languages

- TypeScript / TSX
- JavaScript / JSX
- Python
- Go
- Rust

## Links

- [Keel Repository](https://github.com/FryrAI/Keel)
- [Documentation](https://github.com/FryrAI/Keel/tree/main/docs)
- [Website](https://keel.engineer)

## License

FSL-1.1-MIT (Functional Source License, Version 1.1, MIT Future License)
