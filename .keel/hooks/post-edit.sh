#!/usr/bin/env bash
# keel post-edit hook — runs after file edits to catch violations early.
#
# Install: copy to .keel/hooks/post-edit.sh in your project root
# Usage: called automatically by keel-aware editors/agents after saving a file
#
# Arguments:
#   $1 — path to the edited file (relative to project root)

set -euo pipefail

FILE="${1:-}"

if [ -z "$FILE" ]; then
    echo "Usage: post-edit.sh <file>" >&2
    exit 2
fi

# Only run for supported file types
case "$FILE" in
    *.ts|*.tsx|*.js|*.jsx|*.py|*.go|*.rs) ;;
    *) exit 0 ;;
esac

# Run incremental compile on the changed file
exec keel compile "$FILE"
