# keel

Structural code enforcement for LLM coding agents. This is the main binary crate.

## Install

```bash
# From source
cargo install keel-cli

# Via install script
curl -fsSL https://keel.engineer/install.sh | bash

# Via Homebrew
brew tap FryrAI/tap && brew install keel
```

## Quick Start

```bash
cd your-project
keel init          # Initialize keel, detect tools, generate configs
keel map           # Build the structural graph
keel compile       # Validate contracts
```

## Commands

| Command | Purpose |
|---------|---------|
| `keel init` | Initialize keel in a repo |
| `keel map` | Full structural map |
| `keel compile` | Incremental validation |
| `keel discover` | Adjacency lookup |
| `keel where` | Hash to file:line |
| `keel explain` | Resolution chain |
| `keel fix` | Generate fix plans |
| `keel name` | Naming suggestions |
| `keel serve` | MCP/HTTP server |
| `keel stats` | Telemetry dashboard |
| `keel deinit` | Clean removal |

See [full documentation](https://github.com/FryrAI/Keel) for details.

## License

[FSL-1.1-MIT](https://github.com/FryrAI/Keel/blob/main/LICENSE) — free for non-competing use, converts to MIT after 2 years.
