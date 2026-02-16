# keel-output

Output formatters for [keel](https://keel.engineer) — structural code enforcement for LLM coding agents.

## What's in this crate

- **JSON formatter** — machine-readable output for CI and tool integration
- **LLM formatter** — token-optimized output for AI coding agents (backpressure signals, budget directives)
- **Human formatter** — colored, readable terminal output

## Usage

This crate is primarily used as a dependency of other keel crates.

```toml
[dependencies]
keel-output = "0.1"
```

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
