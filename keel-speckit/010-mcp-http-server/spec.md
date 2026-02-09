# Spec 010: MCP/HTTP Server — `keel serve` Transport Layer

```yaml
tags: [keel, spec, mcp, http, server, watch]
owner: Agent C (Surface)
dependencies:
  - "[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]"
  - "[[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]]"
  - "[[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]]"
prd_sections: [4.8, 9.14]
priority: P1 — powers VS Code extension, MCP integration, and persistent graph performance
```

## Summary

This spec defines the transport layer for `keel serve`: three modes (MCP over stdio, HTTP on localhost, file watcher) that expose keel's core commands as server endpoints. The server is a thin wrapper (~300-500 lines) over the core library defined in [[keel-speckit/007-cli-commands/spec|Spec 007]]. It holds the graph in memory for sub-millisecond responses, watches the file system for changes, and exposes MCP tools for any MCP-compatible LLM tool. No new logic lives here — just transport. The VS Code extension (see [[keel-speckit/011-vscode-extension/spec|Spec 011]]) is the primary consumer of the HTTP mode.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 4.8 | `keel serve` modes (MCP stdio, HTTP localhost:4815, watch), behavior, memory footprint, implementation scope |
| 9.14 | MCP tools exposed (`keel_discover`, `keel_compile`, `keel_where`, `keel_map`, `keel_explain`), HTTP endpoints, file watcher |

---

## Dependencies

- **[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]** — enforcement logic invoked by `compile` endpoint
- **[[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]]** — all server endpoints wrap the `KeelCommands` trait defined there
- **[[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]]** — server returns JSON and LLM formats defined there

---

## Three Server Modes

| Flag | Transport | Port/Channel | Description |
|------|-----------|-------------|-------------|
| `--mcp` | MCP over stdio | stdin/stdout | Integrates with Claude Code, Cursor, Antigravity, Codex, any MCP client |
| `--http` | HTTP REST API | `localhost:4815` | Powers the VS Code extension. Provides REST endpoints for custom integrations. |
| `--watch` | File system watcher | (none — internal) | Auto-runs `compile` on file save. Combines with `--mcp` or `--http`. |

Modes can be combined: `keel serve --http --watch` or `keel serve --mcp --watch`.

---

## MCP Tools Exposed

When running in `--mcp` mode, keel exposes the following MCP tools over stdio:

| MCP Tool | Maps To | Input | Output |
|----------|---------|-------|--------|
| `keel_discover` | `keel discover <hash>` | `{ "hash": string, "depth"?: number }` | Discover JSON (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |
| `keel_compile` | `keel compile [files]` | `{ "files": string[], "batch_start"?: bool, "batch_end"?: bool }` | Compile JSON (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |
| `keel_where` | `keel where <hash>` | `{ "hash": string }` | `{ "file": string, "line_start": number, "line_end": number, "stale": bool }` |
| `keel_map` | `keel map` | `{ "format"?: "json" \| "llm", "scope"?: string[] }` | Map JSON or LLM format (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |
| `keel_explain` | `keel explain <code> <hash>` | `{ "error_code": string, "hash": string }` | Explain JSON (see [[keel-speckit/008-output-formats/spec|Spec 008]]) |

All MCP tools use JSON-RPC 2.0 over stdio as per the MCP specification.

---

## HTTP Endpoints

When running in `--http` mode, keel serves a REST API on `localhost:4815`:

| Method | Path | Maps To | Response |
|--------|------|---------|----------|
| `GET` | `/map` | `keel map --json` | Map JSON |
| `GET` | `/discover/:hash` | `keel discover <hash>` | Discover JSON |
| `POST` | `/compile` | `keel compile [files]` | Compile JSON. Request body: `{ "files": ["path1", "path2"] }` |
| `GET` | `/where/:hash` | `keel where <hash>` | Where JSON |
| `GET` | `/explain/:error_code/:hash` | `keel explain <code> <hash>` | Explain JSON |
| `GET` | `/health` | Health check | `{ "status": "ok", "version": "2.0.0", "graph_nodes": 342, "graph_edges": 891 }` |

**Query parameters for `/map`:**

- `?format=llm` — return LLM-optimized format instead of JSON
- `?scope=auth,payments` — scoped map for specific modules

**HTTP status codes:**

| Status | Meaning |
|--------|---------|
| 200 | Success |
| 400 | Bad request (invalid hash, missing parameters) |
| 404 | Hash not found in graph |
| 500 | Internal error |

---

## Graph in Memory

The key difference between `keel serve` and CLI mode is memory management:

- **`keel serve`:** Holds the full graph in memory. Sub-millisecond responses to `discover`, `where`, `explain`. No SQLite I/O per request.
- **CLI mode:** Loads a subgraph from SQLite per invocation. Uses ~20-50MB. Suitable for constrained environments or CI.

**Memory footprint for `keel serve`:**

| Codebase Size | Expected Memory |
|---------------|----------------|
| 50k LOC | ~50-100MB |
| 200k LOC | ~200-400MB |

---

## File Watching (`--watch`)

When `--watch` is active:

1. keel watches the file system for changes to source files (respecting `.keelignore` and `[exclude]` patterns).
2. On file save, keel auto-runs `compile` on the changed file(s).
3. Results are:
   - Written to the in-memory graph state
   - Emitted as MCP notifications (if `--mcp` is active)
   - Available via HTTP endpoints (if `--http` is active)
   - Written to `graph.db` for persistence

**File watcher implementation:** Use `notify` crate (Rust) for cross-platform file system watching. Debounce saves at 100ms to avoid duplicate compiles on multi-write editors.

---

## Circuit Breaker State

In `keel serve` mode, circuit breaker state (consecutive failure counts per error-code + hash pair) is held in memory as session state. This avoids the `.keel/session.json` file used in CLI mode. State resets when the server restarts.

---

## Implementation Scope

This is a **thin wrapper** (~300-500 lines of Rust) over the core library. The implementation:

- Calls the same `KeelCommands` trait that the CLI binary uses
- Adds no new logic — just transport (stdio for MCP, HTTP for REST, fswatch for file changes)
- Manages in-memory graph state (loads from SQLite on startup, updates incrementally)
- Manages circuit breaker session state in memory

**Crates used:**

- `axum` or `hyper` for HTTP server
- `notify` for file system watching
- MCP protocol handling via stdin/stdout JSON-RPC

---

## Inter-Agent Contracts

### Consumed by this spec:

- **[[keel-speckit/007-cli-commands/spec|Spec 007]]** — `KeelCommands` trait. Every server endpoint calls the corresponding trait method.
- **[[keel-speckit/008-output-formats/spec|Spec 008]]** — All response formats. The server serializes the same types.
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006]]** — Enforcement logic runs inside `compile` endpoint calls.

### Exposed by this spec (Agent C -> Agent C):

**Server availability:** The HTTP server must be running before the VS Code extension (Spec 011) can operate.

```rust
pub struct KeelServer {
    pub fn start_mcp(&self) -> Result<(), KeelError>;
    pub fn start_http(&self, port: u16) -> Result<(), KeelError>;
    pub fn start_watch(&self) -> Result<(), KeelError>;
}
```

---

## Acceptance Criteria

**GIVEN** `keel serve --http` is running
**WHEN** a `GET /health` request is made
**THEN** a 200 response with `status: "ok"`, keel version, and graph node/edge counts is returned.

**GIVEN** `keel serve --http` is running with a loaded graph
**WHEN** a `GET /discover/:hash` request is made with a valid hash
**THEN** the response contains upstream callers, downstream callees, and module context in Discover JSON format.

**GIVEN** `keel serve --http` is running
**WHEN** a `POST /compile` request is made with a list of changed files
**THEN** the response contains compile results in Compile JSON format, and the in-memory graph is updated.

**GIVEN** `keel serve --mcp` is running
**WHEN** an MCP `keel_discover` tool call is made via stdio
**THEN** a valid JSON-RPC 2.0 response with Discover JSON is returned via stdout.

**GIVEN** `keel serve --http --watch` is running
**WHEN** a source file is saved to disk
**THEN** `compile` auto-runs on the changed file, the in-memory graph is updated, and subsequent HTTP requests reflect the new state.

**GIVEN** `keel serve --http` is running
**WHEN** a `GET /discover/:hash` request is made with an invalid hash
**THEN** a 404 response is returned.

**GIVEN** `keel serve --http` is running and holding graph in memory
**WHEN** a `GET /discover/:hash` request is made
**THEN** the response time is sub-millisecond (no SQLite I/O).

**GIVEN** `keel serve --watch` is running and a file is saved twice within 50ms
**WHEN** the debounce period (100ms) elapses
**THEN** `compile` runs only once for the file.

---

## Test Strategy

**Oracle:** Server endpoint correctness and MCP protocol compliance.
- Verify every HTTP endpoint returns correct status codes and response bodies.
- Verify MCP tools follow JSON-RPC 2.0 protocol.
- Verify file watcher triggers compile on save.
- Verify in-memory graph state stays consistent with on-disk SQLite.
- Verify debounce behavior for rapid file saves.

**Test files to create:**
- `tests/server/test_http_endpoints.rs` (~8 tests)
- `tests/server/test_mcp_tools.rs` (~6 tests)
- `tests/server/test_file_watcher.rs` (~5 tests)
- `tests/server/test_health_check.rs` (~3 tests)
- `tests/server/test_in_memory_graph.rs` (~4 tests)
- `tests/server/test_debounce.rs` (~3 tests)

**Estimated test count:** ~29

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| MCP protocol evolves with breaking changes | Pin to MCP spec version used at launch. Add version negotiation. |
| Port 4815 conflicts with other services | Make port configurable via `--port` flag or `config.toml`. Default to 4815. |
| File watcher misses changes on network drives or Docker volumes | Document known file system watching limitations. Fall back to polling mode if `notify` events are unreliable. |
| In-memory graph diverges from SQLite after crash | On startup, always load from SQLite. On graceful shutdown, flush to SQLite. Periodic sync every 30s. |
| Memory usage exceeds expectations for very large codebases | Implement lazy loading: only load subgraphs on demand for codebases > 500k LOC. Warn if memory exceeds 1GB. |

---

## Related Specs

- [[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]] — enforcement logic invoked by compile endpoint
- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — command implementations wrapped by server
- [[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]] — response format schemas
- [[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]] — MCP server referenced in tool integration matrix
- [[keel-speckit/011-vscode-extension/spec|Spec 011: VS Code Extension]] — primary consumer of HTTP mode
