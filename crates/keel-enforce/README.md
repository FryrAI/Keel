# keel-enforce

Enforcement engine for [keel](https://keel.engineer) — structural code enforcement for LLM coding agents.

## What's in this crate

- **Compile validation** — checks for broken callers, missing type hints, arity mismatches, and more
- **Error codes** — E001-E005 (errors) and W001-W002 (warnings) with actionable fix hints
- **Circuit breaker** — auto-downgrades repeated false positives after 3 failures
- **Batch mode** — defers non-critical checks during rapid agent iteration
- **Fix generation** — diff-style fix plans for violations
- **Naming engine** — location-aware naming suggestions with keyword overlap scoring

## Usage

This crate is primarily used as a dependency of other keel crates.

```toml
[dependencies]
keel-enforce = "0.1"
```

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
