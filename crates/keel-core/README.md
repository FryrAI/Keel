# keel-core

Core library for [keel](https://keel.engineer) — structural code enforcement for LLM coding agents.

## What's in this crate

- **Graph schema** — `NodeKind`, `EdgeKind`, `GraphNode`, `GraphEdge` types for the structural code graph
- **GraphStore trait** — abstract interface for graph storage with SQLite implementation
- **SqliteGraphStore** — production graph store using rusqlite (bundled, zero runtime deps)
- **Hashing** — `base62(xxhash64(...))` producing 11-character symbol hashes
- **Configuration** — `KeelConfig` for `.keel/keel.json` loading with sensible defaults

## Usage

This crate is primarily used as a dependency of other keel crates. You probably want the `keel-cli` crate (the `keel` binary) instead.

```toml
[dependencies]
keel-core = "0.1"
```

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
