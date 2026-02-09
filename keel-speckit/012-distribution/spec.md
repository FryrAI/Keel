# Spec 012: Distribution — Cross-Platform Build and Install

```yaml
tags: [keel, spec, distribution, build, install, cross-platform]
owner: Agent C (Surface)
dependencies:
  - "All other specs — this is the final integration layer"
prd_sections: [10]
priority: P1 — needed for release, not for development
```

## Summary

This spec defines how keel is built, packaged, and distributed across all supported platforms. keel ships as a single binary with zero runtime dependencies — tree-sitter grammars are compiled in, SQLite is statically linked. Distribution covers pre-built binaries for Linux (x86_64, arm64), macOS (arm64, x86_64), and Windows (x86_64), with install methods including curl, Homebrew, Cargo, winget, and Scoop. Cross-compilation uses `cargo-dist`. CI via GitHub Actions automates builds on tag push.

---

## PRD Traceability

| PRD Section | Content Extracted |
|-------------|-------------------|
| 10 (Build and distribution) | Single binary via `cargo build --release`. Pre-built binaries for 3 platforms. Install methods. Binary size target 20-35MB. tree-sitter grammars compiled in, SQLite statically linked. Windows native binary (no WSL). `cargo-dist` for cross-compilation. |

---

## Dependencies

This spec depends on all other specs because it is the final integration layer that packages the complete keel binary:

- **[[keel-speckit/000-graph-schema/spec|Spec 000: Graph Schema]]** — graph structures compiled into binary
- **[[keel-speckit/001-treesitter-foundation/spec|Spec 001: Tree-sitter Foundation]]** — tree-sitter grammars compiled in
- **[[keel-speckit/002-typescript-resolution/spec|Spec 002: TypeScript Resolution]]** — Oxc crates linked
- **[[keel-speckit/003-python-resolution/spec|Spec 003: Python Resolution]]** — ty subprocess invocation
- **[[keel-speckit/004-go-resolution/spec|Spec 004: Go Resolution]]** — tree-sitter heuristics
- **[[keel-speckit/005-rust-resolution/spec|Spec 005: Rust Resolution]]** — rust-analyzer integration
- **[[keel-speckit/006-enforcement-engine/spec|Spec 006: Enforcement Engine]]** — enforcement logic
- **[[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]]** — CLI entry point
- **[[keel-speckit/008-output-formats/spec|Spec 008: Output Formats]]** — serialization code
- **[[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]]** — config generators
- **[[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]]** — server code
- **[[keel-speckit/011-vscode-extension/spec|Spec 011: VS Code Extension]]** — packaged separately (not in the Rust binary)

---

## Build Configuration

### Single Binary

```bash
cargo build --release
```

Produces a single statically-linked binary with zero runtime dependencies.

### What Is Compiled In

| Component | How Included | Notes |
|-----------|-------------|-------|
| tree-sitter grammars (TypeScript, Python, Go, Rust) | Compiled in via `tree-sitter` crate build scripts | ~4 grammar `.so`/`.dylib` objects linked into binary |
| SQLite | Statically linked via `rusqlite` with `bundled` feature | No system SQLite dependency |
| Oxc resolver + semantic | Rust crate dependency | Compiled directly |
| clap | Rust crate dependency | CLI argument parsing |
| serde / serde_json | Rust crate dependency | JSON serialization |
| xxhash-rust | Rust crate dependency | Hash computation |
| petgraph | Rust crate dependency | Graph data structure |

### Binary Size

**Expected range:** 20-35MB

This includes 4 languages of tree-sitter grammars + Oxc crates + SQLite + the resolution engine + MCP/HTTP server. Comparable to ripgrep + tree-sitter builds.

**If binary exceeds 40MB:**

1. Enable LTO (Link-Time Optimization) in `Cargo.toml`:
   ```toml
   [profile.release]
   lto = true
   ```
2. Strip debug symbols:
   ```toml
   [profile.release]
   strip = true
   ```
3. Investigate which crates contribute most to binary size using `cargo bloat`.

---

## Target Platforms

### Pre-Built Binaries

| Platform | Architecture | Binary Name |
|----------|-------------|-------------|
| Linux | x86_64 | `keel-x86_64-unknown-linux-gnu` |
| Linux | arm64 | `keel-aarch64-unknown-linux-gnu` |
| macOS | arm64 (Apple Silicon) | `keel-aarch64-apple-darwin` |
| macOS | x86_64 (Intel) | `keel-x86_64-apple-darwin` |
| Windows | x86_64 | `keel-x86_64-pc-windows-msvc.exe` |

### Windows Requirements

- **Native binary, no WSL required.** keel must run natively on Windows.
- **Path handling:** Use platform-native separators internally (`\` on Windows). Forward slashes (`/`) in all output (JSON, LLM format, CLI display) for cross-platform consistency.
- Test against Windows-specific edge cases: long paths (>260 chars), UNC paths, junction points.

---

## Install Methods

### 1. Shell installer (macOS / Linux)

```bash
curl -fsSL https://keel.engineer/install.sh | sh
```

The install script:
- Detects OS and architecture
- Downloads the correct pre-built binary from GitHub Releases
- Places binary in `~/.local/bin/` (or `$HOME/.keel/bin/` if `~/.local/bin/` is not in PATH)
- Verifies checksum
- Adds to PATH if needed (prompts user)

### 2. Homebrew (macOS / Linux)

```bash
brew install keel
```

Homebrew formula fetches pre-built binary from GitHub Releases. No compilation required on user machine.

### 3. Cargo (any platform with Rust toolchain)

```bash
cargo install keel
```

Builds from source. Requires Rust toolchain. This is the fallback for platforms without pre-built binaries.

### 4. winget (Windows)

```bash
winget install keel
```

Windows Package Manager. Fetches pre-built `.exe` from GitHub Releases.

### 5. Scoop (Windows)

```bash
scoop install keel
```

Scoop bucket with pre-built `.exe`.

---

## Cross-Compilation

**Tool:** `cargo-dist` for automated cross-compilation and release artifact generation.

`cargo-dist` configuration in `Cargo.toml`:

```toml
[workspace.metadata.dist]
cargo-dist-version = "0.x.x"
ci = ["github"]
installers = ["shell", "homebrew"]
targets = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
]
```

---

## CI: Automated Builds

### GitHub Actions

On tag push (e.g., `v2.0.0`), GitHub Actions:

1. Runs `cargo test` on all platforms
2. Builds release binaries for all 5 targets via `cargo-dist`
3. Generates checksums (SHA-256)
4. Creates GitHub Release with all binaries attached
5. Updates Homebrew formula
6. Publishes to crates.io (`cargo publish`)

```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  release:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
          - os: macos-latest
            target: aarch64-apple-darwin
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: windows-latest
            target: x86_64-pc-windows-msvc
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Build
        run: cargo build --release --target ${{ matrix.target }}
      - name: Test
        run: cargo test --release --target ${{ matrix.target }}
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: keel-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/keel*
```

**Note:** The actual release workflow will use `cargo-dist`'s generated CI config, which is more comprehensive. The above is a simplified illustration.

---

## VS Code Extension Distribution

The VS Code extension is packaged and distributed separately from the Rust binary:

- Published to the VS Code Marketplace
- Published to Open VSX Registry (for open-source VS Code forks)
- Extension package includes only TypeScript (~500 lines), no Rust binary
- Extension auto-detects keel binary on PATH or prompts installation

---

## Inter-Agent Contracts

### Consumed by this spec:

All other specs — the distribution packages the complete binary containing all components.

### Exposed by this spec:

**Binary availability:** The `keel` binary must be on the user's PATH for all tool integrations (hooks, instruction files, CI templates) to work. Install methods must ensure this.

---

## Acceptance Criteria

**GIVEN** a clean Linux x86_64 machine with no Rust toolchain
**WHEN** `curl -fsSL https://keel.engineer/install.sh | sh` is run
**THEN** the keel binary is downloaded, checksum-verified, placed on PATH, and `keel --version` prints the correct version.

**GIVEN** a macOS arm64 machine with Homebrew
**WHEN** `brew install keel` is run
**THEN** the keel binary is installed and `keel --version` prints the correct version.

**GIVEN** a Windows x86_64 machine
**WHEN** `winget install keel` is run
**THEN** `keel.exe` is installed and `keel --version` prints the correct version.

**GIVEN** a machine with Rust toolchain
**WHEN** `cargo install keel` is run
**THEN** keel is built from source and `keel --version` prints the correct version.

**GIVEN** a GitHub tag push `v2.0.0`
**WHEN** the CI release workflow runs
**THEN** pre-built binaries for all 5 targets are attached to the GitHub Release with SHA-256 checksums.

**GIVEN** the released binary for any platform
**WHEN** the binary size is measured
**THEN** it is between 20MB and 40MB. If it exceeds 40MB, LTO and stripping must be applied.

---

## Test Strategy

**Oracle:** Build and install correctness across platforms.
- Build on all 5 target platforms in CI.
- Verify binary runs and prints version on each platform.
- Verify install scripts work on fresh VMs (macOS, Ubuntu, Windows).
- Verify binary has zero runtime dependencies (no shared library errors on clean systems).
- Verify Windows path handling with forward slashes in output.

**Test files to create:**
- `tests/distribution/test_binary_size.rs` (~2 tests)
- `tests/distribution/test_version_output.rs` (~2 tests)
- `tests/distribution/test_windows_paths.rs` (~4 tests)
- `tests/distribution/test_install_script.sh` (~3 tests)
- `tests/distribution/test_no_runtime_deps.sh` (~2 tests)
- CI matrix tests (part of GitHub Actions workflow) (~5 tests)

**Estimated test count:** ~18

---

## Known Risks

| Risk | Mitigation |
|------|-----------|
| Cross-compilation fails for Linux arm64 on GitHub Actions | Use `cross` crate or Docker-based cross-compilation. Pre-test in CI before tagging releases. |
| Binary size exceeds 40MB | Enable LTO + stripping. Profile with `cargo bloat`. Consider feature flags to exclude unused language support. |
| Install script fails on exotic Linux distros | Test on Ubuntu, Fedora, Alpine, Arch. Use static linking to minimize glibc dependency issues. |
| Homebrew formula review delays | Submit formula early. Maintain a tap (`keel-engineer/tap`) as fallback. |
| Windows path handling edge cases | Comprehensive Windows path tests. Use `std::path` consistently. Normalize all output paths to forward slashes. |
| `cargo-dist` version incompatibilities | Pin `cargo-dist` version. Run release workflow on test tags before real releases. |

---

## Related Specs

- [[keel-speckit/007-cli-commands/spec|Spec 007: CLI Commands]] — CLI entry point packaged in binary
- [[keel-speckit/010-mcp-http-server/spec|Spec 010: MCP/HTTP Server]] — server code packaged in binary
- [[keel-speckit/011-vscode-extension/spec|Spec 011: VS Code Extension]] — distributed separately via VS Code Marketplace
- [[keel-speckit/009-tool-integration/spec|Spec 009: Tool Integration]] — CI templates reference install methods defined here
