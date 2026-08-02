#!/bin/bash
set -euo pipefail
# .keel/hooks/post-edit.sh
# Shared post-edit hook for all Tier 1 tools (Claude Code, Cursor, Gemini CLI, Windsurf, Letta).
# Reads tool_input from stdin, extracts file_path, runs keel compile.
# $1 (optional): calling client name for telemetry attribution, e.g. "claude-code".
# Passed explicitly by the hook config that invokes this script (see
# generators.rs's inject_on_edit_hook) rather than relying on env-var
# detection (CLAUDECODE etc.), which does not reliably survive into this
# script's subprocess.
# Exit code 2 = blocking (stderr shown to LLM, must fix before proceeding).
CLIENT="${1:-}"
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
[ -z "$FILE_PATH" ] && exit 0

# Validate file path — reject metacharacters that could enable argument injection
if [[ "$FILE_PATH" =~ [^a-zA-Z0-9_./-] ]]; then
  echo "keel: rejected file path with unexpected characters: $FILE_PATH" >&2
  exit 2
fi

if [ -n "$CLIENT" ]; then
  RESULT=$(timeout 5 keel compile --delta --llm --client "$CLIENT" -- "$FILE_PATH" 2>&1)
else
  RESULT=$(timeout 5 keel compile --delta --llm -- "$FILE_PATH" 2>&1)
fi
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "$RESULT" >&2
  exit 2  # Blocking: stderr shown to LLM, must fix before proceeding
fi
exit 0
