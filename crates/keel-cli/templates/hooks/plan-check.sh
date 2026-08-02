#!/bin/bash
# .keel/hooks/plan-check.sh
# PreToolUse hook on Claude Code's `ExitPlanMode`: check the plan against the
# code graph BEFORE any code exists. Resteering a plan is cheap; resteering
# 2,000 lines is not.
#
# Reads the hook payload on stdin and pipes `.tool_input.plan` into
# `keel validate-plan --llm -`, then prints any P001 (unknown call target) /
# P002 (signature mismatch) findings to stderr, where Claude Code shows them
# to the model.
#
# ADVISORY BY DEFAULT — this hook always exits 0, so no session is ever
# blocked by the default config.
#
#   Bypass (one line):   KEEL_PLAN_HOOK=0 claude
#   Blocking (opt-in):   KEEL_PLAN_STRICT=1 claude    # exit 2 on P001/P002
#
# Repeat findings are routed through keel's circuit breaker: after 3 strikes on
# the same P-code + symbol, the finding auto-downgrades to INFO and stops
# failing --strict, so a stubborn claim degrades to advice instead of
# deadlocking the session.
set -uo pipefail

[ "${KEEL_PLAN_HOOK:-1}" = "0" ] && exit 0
command -v keel >/dev/null 2>&1 || exit 0
command -v jq >/dev/null 2>&1 || exit 0

INPUT=$(cat)
PLAN=$(printf '%s' "$INPUT" | jq -r '.tool_input.plan // empty')
[ -z "$PLAN" ] && exit 0

ARGS=(validate-plan --llm -)
STRICT="${KEEL_PLAN_STRICT:-0}"
if [ "$STRICT" = "1" ]; then
  ARGS+=(--strict)
fi

RESULT=$(printf '%s' "$PLAN" | timeout 5 keel "${ARGS[@]}" 2>&1)
STATUS=$?

# Only the P-namespace lines belong here. The risk report (actions, callers at
# risk, suggested order) stays available via `keel validate-plan` on demand.
FINDINGS=$(printf '%s\n' "$RESULT" | grep -E '^(P00[12] |  at: |  fix: )' || true)
[ -z "$FINDINGS" ] && exit 0

echo "keel plan check (advisory — the plan was accepted):" >&2
printf '%s\n' "$FINDINGS" >&2

if [ "$STRICT" = "1" ] && [ "$STATUS" -eq 1 ]; then
  exit 2 # opt-in blocking: stderr is shown to the model, which must revise
fi
exit 0
