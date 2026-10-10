#!/bin/bash
# keel pre-commit hook — safety net for cooperative tools.
# Catches anything that slipped past tool hooks or was committed without running hooks.
# Install: cp tools/hooks/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
set -e
# Installed by keel init
STATUS=0
keel compile --changed --strict --json 2>&1 || STATUS=$?
if [ "$STATUS" -ne 0 ]; then
  # Exit 1 = violations; any other non-zero is keel failing or refusing to check.
  if [ "$STATUS" -eq 1 ]; then
    echo "keel: commit blocked — violations found" >&2
  else
    echo "keel: commit blocked — keel could not check this commit (see above)" >&2
  fi
  exit 1
fi
keel audit --changed 2>&1 || true
