# keel-server

MCP and HTTP server for [keel](https://keel.engineer) — structural code enforcement for LLM coding agents.

## What's in this crate

- **MCP server** — Model Context Protocol server for Claude Code, Cursor, and other MCP-compatible tools
- **HTTP server** — REST API for compile, discover, where, explain, and map operations
- **File watcher** — automatic re-compilation on file changes using notify

## Usage

This crate is primarily used as a dependency of the `keel-cli` crate. Use `keel serve` to start the server.

```toml
[dependencies]
keel-server = "0.1"
```

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
