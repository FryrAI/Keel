#!/usr/bin/env python3
"""keel notify hook for Codex CLI.

Runs `keel compile --changed --json` after each agent turn.
Prints violations to stderr so Codex sees them in the next turn.

Place at: .codex/keel-notify.py
Configure in .codex/config.toml:
  notify = ".codex/keel-notify.py"
"""
import json
import subprocess
import sys


def main() -> None:
    """Run keel compile on agent-turn-complete events and print violations to stderr."""
    event = json.load(sys.stdin)
    if event.get("type") != "agent-turn-complete":
        return

    result = subprocess.run(
        ["keel", "compile", "--changed", "--json"],
        capture_output=True,
        text=True,
    )

    if result.returncode != 0 and result.stderr.strip():
        print(result.stderr, file=sys.stderr)


if __name__ == "__main__":
    main()
