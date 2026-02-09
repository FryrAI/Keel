# Contributing to keel

Thank you for your interest in contributing to keel! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

- **Rust 1.75+** — install via [rustup](https://rustup.rs/)
- **Git** — for version control

### Development Setup

```bash
# Clone the repository
git clone https://github.com/FryrAI/Keel.git
cd Keel

# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check formatting
cargo fmt --all -- --check

# Run clippy lints
cargo clippy --workspace --all-targets -- -D warnings
```

### Project Structure

keel is a Cargo workspace with 6 crates:

| Crate | Purpose |
|-------|---------|
| `keel-core` | Graph schema, hashing, SQLite storage |
| `keel-parsers` | tree-sitter parsing, query patterns, file walker |
| `keel-enforce` | Compile validation, error codes, circuit breaker |
| `keel-cli` | CLI entry point, command routing |
| `keel-server` | MCP + HTTP server, file watcher |
| `keel-output` | JSON, LLM, human output formatters |

## How to Contribute

### Reporting Bugs

Use the [bug report template](https://github.com/FryrAI/Keel/issues/new?template=bug_report.md) and include:

- Steps to reproduce
- Expected vs actual behavior
- keel version (`keel --version`)
- OS and Rust version

### Suggesting Features

Use the [feature request template](https://github.com/FryrAI/Keel/issues/new?template=feature_request.md) and describe:

- The problem you're trying to solve
- Your proposed solution
- Alternative approaches you've considered

### Submitting Pull Requests

1. **Fork** the repository
2. **Create a branch** from `main` — use `feature/description` or `fix/description`
3. **Make your changes** — keep PRs focused on a single concern
4. **Add tests** — every new feature or bug fix should include tests
5. **Run the full check suite:**
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
6. **Submit** the PR using the [PR template](https://github.com/FryrAI/Keel/blob/main/.github/PULL_REQUEST_TEMPLATE.md)

## Coding Standards

### Rust Style

- Follow standard Rust conventions and `rustfmt` defaults
- Use `thiserror` for error types
- Prefer returning `Result<T, E>` over panicking
- Keep functions under 50 lines where practical

### Testing

- Unit tests go in the same file as the code (`#[cfg(test)]` module) or in a sibling `tests.rs`
- Integration tests go in `tests/`
- Test fixtures go in `tests/fixtures/`
- Use descriptive test names: `test_parse_typescript_function`, not `test1`

### Documentation

- Public APIs should have doc comments
- Complex logic should have inline comments explaining "why", not "what"
- Don't add comments to obvious code

### Commit Messages

- Use imperative mood: "Add feature" not "Added feature"
- First line: <50 characters, summarizes the change
- Body (optional): explain "why" not "what"

## Architecture Decisions

### Frozen Contracts

These interfaces are frozen and must not be modified without explicit approval:

1. `LanguageResolver` trait in `keel-parsers`
2. `GraphStore` trait in `keel-core`
3. `CompileResult` / `DiscoverResult` / `ExplainResult` structs in `keel-enforce`
4. JSON output schemas in `tests/schemas/`

### Non-Negotiable Constraints

- Pure Rust — no FFI in hot paths
- Single binary — zero runtime dependencies
- `ty` is subprocess only (not a library)
- Cross-platform: Linux, macOS, Windows

## License

By contributing to keel, you agree that your contributions will be licensed under the FSL-1.1-MIT license. See [LICENSE](LICENSE) for details.
