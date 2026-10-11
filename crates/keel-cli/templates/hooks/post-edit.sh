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
#
# Exit codes (as the Tier 1 tools read them):
#   0 = nothing to report.
#   2 = blocking: violations, no usable graph, or an edit keel skipped because
#       the graph lock was busy; stderr reaches the LLM. The agent re-runs the
#       compile: this is 2 rather than 1 because nobody else re-checks the file.
#   1 = non-blocking error: keel could not check the file (internal error,
#       timeout, a path this script will not pass along). Surfaced to the
#       user, not as "fix before proceeding" — no edit to this file can
#       resolve an internal error or a timeout.
# A non-zero exit ALWAYS carries a reason on stderr; a silent block is a bug.
CLIENT="${1:-}"
NO_USABLE_GRAPH='keel compile: no usable graph — '
LOCK_SKIP='keel compile: another keel process holds the graph lock, skipping'
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
[ -z "$FILE_PATH" ] && exit 0

# Skip files outside this repository. The editor fires this hook for every
# write it makes, including ones far from the project (agent memory files, a
# config in $HOME); keel's graph cannot know them, so checking them can only
# produce noise. Relative paths are always in-tree.
if [[ "$FILE_PATH" == /* ]]; then
  ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || ROOT=""
  [ -n "$ROOT" ] || ROOT="$PWD"
  ROOT=$(cd "$ROOT" && pwd -P) || exit 0
  FILE_DIR=$(cd "$(dirname "$FILE_PATH")" 2>/dev/null && pwd -P) || FILE_DIR=""
  if [ -n "$FILE_DIR" ]; then
    case "$FILE_DIR" in
      "$ROOT" | "$ROOT"/*) ;;
      *) exit 0 ;;
    esac
  fi
fi

# Validate the path only once it is known to be in scope — the scope check
# above quotes every use of $FILE_PATH, so it is safe on any spelling, and an
# out-of-tree path must be skipped, not rejected. A rejection is "keel could
# not check this file", not a violation: exit 1, non-blocking, with the reason.
if [[ "$FILE_PATH" =~ [^a-zA-Z0-9_./-] ]]; then
  echo "keel: rejected file path with unexpected characters: $FILE_PATH" >&2
  exit 1
fi

ARGS=(compile --delta --llm)
if [ -n "$CLIENT" ]; then
  ARGS+=(--client "$CLIENT")
fi

# `RESULT=$(...)` on its own would abort the script under `set -e` before the
# status could be read, throwing the diagnostic away — that is what made every
# failure a content-free block. In an `&&`/`||` list `set -e` stands down.
RESULT=$(timeout 5 keel "${ARGS[@]}" -- "$FILE_PATH" 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
if [ "$EXIT_CODE" -eq 0 ]; then
  if ! grep -q "^$LOCK_SKIP" <<< "$RESULT"; then
    exit 0
  fi
  echo "$RESULT" >&2
  echo "keel: $FILE_PATH was NOT checked — another keel process holds the graph lock (usually a running keel map). Nothing in the file needs fixing because of this message; once the lock is free, run in $PWD: keel compile -- $FILE_PATH" >&2
  exit 2
fi

if [ -z "$RESULT" ]; then
  if [ "$EXIT_CODE" -eq 124 ]; then
    RESULT="keel: compile timed out after 5s for $FILE_PATH"
  else
    RESULT="keel: compile exited $EXIT_CODE with no output for $FILE_PATH"
  fi
fi
echo "$RESULT" >&2

# Match the start of ANY line, even after a version-drift warning.
if [ "$EXIT_CODE" -eq 2 ] && grep -q "^$NO_USABLE_GRAPH" <<< "$RESULT"; then
  exit 2
fi

# keel exit 1 = violations. Other internal errors and timeouts stay non-blocking.
[ "$EXIT_CODE" -eq 1 ] && exit 2
exit 1
