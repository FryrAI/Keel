#!/bin/bash
# keel pre-commit hook — safety net for cooperative tools.
# Catches anything that slipped past tool hooks or was committed without running hooks.
# Install: cp tools/hooks/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
set -e
# Installed by keel init
keel compile --changed --strict --json 2>&1 || {
  echo "keel: commit blocked — violations found" >&2
  exit 1
}
keel audit --changed 2>&1 || true
