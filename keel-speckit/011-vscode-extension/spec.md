# Spec 011: VS Code Extension — Display Layer for keel

```yaml
tags: [keel, spec, vscode, extension, ui, display]
owner: Agent C (Surface)
dependencies:
  - "[[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]]"
prd_sections: [9.15]
priority: P1 — human visibility layer, not on the critical LLM enforcement path
```

## Summary

This spec defines the VS Code extension (`keel-vscode`): a lightweight TypeScript extension (~500 lines) that provides human-readable keel information inside the editor. It displays a status bar indicator, inline diagnostics (red/yellow squiggles), CodeLens annotations above functions, command palette commands, and hover information. All intelligence lives in the Rust binary — the extension is a pure display layer that calls `keel serve --http` for all data. It works with VS Code, Cursor, Antigravity, and Windsurf (all VS Code forks).

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 9.15 | VS Code extension features: status bar indicator, inline diagnostics, CodeLens, command palette, hover info. Implementation: thin client calling `keel serve --http`. ~500 lines TypeScript. Works with VS Code + 3 forks. |

---

## Dependencies

- **[[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]]** — the extension calls `keel serve --http` for all data. The HTTP server must be running (auto-started by the extension if not already running).

---

## Architecture

```
┌──────────────────────────────┐
│  VS Code / Cursor / Windsurf │
│  ┌─────────────────────────┐ │
│  │   keel-vscode extension │ │     HTTP
│  │   (~500 lines TS)       │────────────┐
│  │   Display layer only    │ │          │
│  └─────────────────────────┘ │          ▼
└──────────────────────────────┘   ┌────────────┐
                                   │ keel serve  │
                                   │ --http      │
                                   │ localhost:  │
                                   │ 4815        │
                                   └────────────┘
```

**Key principle:** Zero intelligence in the extension. All graph queries, enforcement logic, and output formatting live in the Rust binary exposed via `keel serve --http`. The extension is a thin display client.

---

## Features

### Status Bar Indicator

A persistent status bar item showing keel's current graph status:

| Icon | Meaning |
|------|---------|
| `keel ✓` (green) | Graph is clean — no errors or warnings |
| `keel ⚠ 3` (yellow) | 3 warnings present |
| `keel ✗ 2` (red) | 2 errors present |
| `keel ?` (gray) | Server not running or graph not loaded |

Clicking the status bar item opens the keel output channel showing recent compile results.

### Inline Diagnostics

keel `compile` errors and warnings are displayed as VS Code diagnostic markers:

- **ERROR** (E001-E005): Red squiggly underline on the function declaration line.
- **WARNING** (W001-W004): Yellow squiggly underline on the function declaration line.
- Diagnostic messages include the error code, message, and `fix_hint` text from the compile JSON output.
- Diagnostics update automatically when files are saved (via the file watcher in `keel serve --watch`).

### CodeLens

Displays `↑N ↓M` annotations above each function declaration:

```
↑3 ↓2                          ← CodeLens: 3 callers, 2 callees
function login(email: string, password: Password): Token {
```

- `↑N` = upstream caller count
- `↓M` = downstream callee count
- Clicking the CodeLens runs `keel discover <hash>` and shows results in a peek view
- Functions with `↑0 ↓0` (isolated) still show the CodeLens for completeness

### Command Palette

The extension registers the following commands:

| Command | Description | Action |
|---------|-------------|--------|
| `keel: Discover` | Show callers/callees for current function | Runs `GET /discover/:hash` on the function at cursor, shows result in peek view |
| `keel: Compile` | Validate current file | Runs `POST /compile` with current file, updates diagnostics |
| `keel: Show Map` | Display codebase map | Runs `GET /map?format=llm`, shows in new editor tab |
| `keel: Start Server` | Start `keel serve --http --watch` | Spawns server process if not running |
| `keel: Stop Server` | Stop the keel server | Terminates server process |

### Hover Information

When hovering over a function name:

```
┌─────────────────────────────────────────────┐
│ login  hash: xK2p9Lm4Q                     │
│ ↑1 caller: handleLogin (src/routes/auth.ts) │
│ ↓3 callees: validateCredentials, hashPw,    │
│             generateToken                    │
│ Module: src/auth/  (authentication, token)   │
└─────────────────────────────────────────────┘
```

Hover data comes from `GET /discover/:hash` endpoint.

---

## Server Lifecycle Management

The extension manages the `keel serve --http --watch` process:

1. **On extension activation:** Check if `keel serve --http` is already running by hitting `GET /health` on `localhost:4815`.
2. **If not running:** Auto-start `keel serve --http --watch` as a child process.
3. **On extension deactivation:** Gracefully stop the server process (if started by the extension).
4. **Health polling:** Periodically ping `/health` (every 10s) to detect server crashes. Show gray status bar indicator if server is unreachable.

---

## Data Flow

1. **On file open:** Extension queries `GET /discover/:hash` for each function in the visible range to populate CodeLens.
2. **On file save:** File watcher in `keel serve --watch` auto-compiles. Extension polls for updated diagnostics.
3. **On hover:** Extension queries `GET /discover/:hash` for the hovered function.
4. **On command:** Extension calls the appropriate HTTP endpoint and displays results.

---

## Compatibility

The extension works with all VS Code forks that support the VS Code Extension API:

| Editor | Status |
|--------|--------|
| VS Code | Primary target |
| Cursor | VS Code fork — compatible |
| Antigravity | VS Code fork — compatible |
| Windsurf | VS Code fork — compatible |

One extension, four IDEs. Published to the VS Code Marketplace and Open VSX Registry.

---

## Implementation Constraints

- **~500 lines of TypeScript** — display layer only
- No bundled graph logic, no tree-sitter, no enforcement engine
- All data comes from HTTP endpoints on `localhost:4815`
- Extension activates when a `.keel/` directory is detected in the workspace root
- Graceful degradation: if server is unavailable, show gray status bar, disable CodeLens/hover, but don't error

---

## Inter-Agent Contracts

### Consumed by this spec:

- **[[keel-speckit/010-mcp-http-server/spec|Spec 010]]** — HTTP endpoints at `localhost:4815`. The extension is the primary consumer of:
  - `GET /health` — server status
  - `GET /discover/:hash` — function context for CodeLens and hover
  - `POST /compile` — manual compile trigger
  - `GET /map` — map display
  - `GET /explain/:error_code/:hash` — error details (future)

### Exposed by this spec:

No other specs depend on the extension. It is a leaf node in the dependency graph — a display layer for human engineers.

---

## Acceptance Criteria

**GIVEN** a workspace with `.keel/` directory and `keel serve --http` running
**WHEN** the extension activates
**THEN** the status bar shows `keel ✓` (or appropriate error/warning count) and CodeLens annotations appear above functions.

**GIVEN** a file with a function that has 3 upstream callers and 2 downstream callees
**WHEN** the file is opened in the editor
**THEN** CodeLens shows `↑3 ↓2` above the function declaration.

**GIVEN** a compile error E001 on a function at line 42
**WHEN** diagnostics are updated from the server
**THEN** a red squiggly underline appears on line 42 with the error message and fix hint.

**GIVEN** the keel server is not running
**WHEN** the extension activates
**THEN** the extension auto-starts `keel serve --http --watch` and the status bar transitions from gray `keel ?` to green `keel ✓` (or appropriate state).

**GIVEN** the user hovers over a function name
**WHEN** the hover provider fires
**THEN** a hover tooltip shows the function's hash, caller count, callee count, and module context.

**GIVEN** the user selects "keel: Discover" from the command palette with cursor on a function
**WHEN** the command executes
**THEN** a peek view opens showing the function's callers, callees, and module context.

---

## Test Strategy

**Oracle:** Extension behavior correctness with mock HTTP server.
- Mock the HTTP endpoints to return known data.
- Verify CodeLens, diagnostics, hover, and commands display correct information.
- Verify server lifecycle management (auto-start, health polling, graceful shutdown).
- Verify graceful degradation when server is unavailable.

**Test files to create:**
- `tests/extension/test_status_bar.ts` (~4 tests)
- `tests/extension/test_codelens.ts` (~4 tests)
- `tests/extension/test_diagnostics.ts` (~4 tests)
- `tests/extension/test_hover.ts` (~3 tests)
- `tests/extension/test_commands.ts` (~3 tests)
- `tests/extension/test_server_lifecycle.ts` (~4 tests)

**Estimated test count:** ~22

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| VS Code fork API incompatibilities | Test against all 4 editors in CI. Use only stable VS Code API (no proposed APIs). |
| Server auto-start fails (permission, PATH issues) | Fall back to manual start via command palette. Show actionable error in status bar. |
| CodeLens performance on large files (1000+ functions) | Batch requests. Only request CodeLens for visible range + small buffer. Lazy-load on scroll. |
| Port 4815 blocked by firewall or occupied | Detect port conflict on startup. Suggest alternative port in error message. |

---

## Related Specs

- [[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]] — HTTP server that powers all extension features
- [[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]] — JSON schemas the extension parses
- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — commands exposed via command palette
