#!/bin/bash
set -euo pipefail
# .keel/hooks/post-edit.sh
# Shared post-edit hook for all Tier 1 tools (Claude Code, Cursor, Gemini CLI, Windsurf, Letta).
# Reads tool_input from stdin, extracts file_path, runs keel compile.
# Exit code 2 = blocking (stderr shown to LLM, must fix before proceeding).
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
[ -z "$FILE_PATH" ] && exit 0

# Validate file path — reject metacharacters that could enable argument injection
if [[ "$FILE_PATH" =~ [^a-zA-Z0-9_./-] ]]; then
  echo "keel: rejected file path with unexpected characters: $FILE_PATH" >&2
  exit 2
fi

RESULT=$(keel compile --delta --llm -- "$FILE_PATH" 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "$RESULT" >&2
  exit 2  # Blocking: stderr shown to LLM, must fix before proceeding
fi
exit 0
