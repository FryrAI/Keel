#!/bin/bash
# keel pre-commit hook — safety net for cooperative tools.
# Catches anything that slipped past tool hooks or was committed without running hooks.
# Install: cp tools/hooks/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
RESULT=$(keel compile --changed --strict --json 2>&1)
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
  echo "keel: commit blocked — violations found:" >&2
  echo "$RESULT" >&2
  exit 1
fi
