#!/usr/bin/env bash
# One sticky keel comment per pull request.
#
# Usage: sticky_comment.sh <keel-output-file>
#
# Find-or-create by a hidden HTML marker, and rewrite the comment ONLY when the
# rendered body actually changed — a re-push that changes nothing keel can see
# must not produce a new notification. That makes deterministic rendering
# load-bearing: no timestamps, no run URLs, no run numbers anywhere in the body.
#
# REST only (`gh api repos/{owner}/{repo}/issues/{n}/comments`). `gh pr view`
# and `gh pr edit` go through GraphQL and 400 on the Projects-classic
# deprecation on FryrAI repos, which is exactly the kind of failure that would
# silently cost the comment.
#
# This script NEVER fails the job. A forked pull request runs with a read-only
# token by design; the content is written to the step summary and the run stays
# green. `pull_request_target` would fix that by handing fork code the write
# token, which is a supply-chain hole, not a fix.
#
# Environment:
#   GH_TOKEN             token for `gh api` (may be read-only)
#   GITHUB_REPOSITORY    owner/repo
#   PR_NUMBER            pull request number ('' outside a PR — then this is a no-op)
#   IS_FORK              'true' when the PR head is a fork (skip straight to the summary)
#   GITHUB_STEP_SUMMARY  file the fallback (and mirror) is written to

set -uo pipefail

MARKER='<!-- keel:sticky-review -->'
CLEAN_BODY='No contract changes and no new violations in this diff.'
FOOTER='_[keel](https://keel.engineer) posted this once and updates it in place._'

# --------------------------------------------------------------------------
# Rendering — deterministic by construction
# --------------------------------------------------------------------------

# The comment content, without the markers: keel's own output, fenced, or the
# fixed clean sentence when keel had nothing to say.
render_content() {
  local raw="$1"
  if [ -z "$raw" ]; then
    printf '### keel\n\n%s\n\n%s\n' "$CLEAN_BODY" "$FOOTER"
  else
    printf '### keel\n\n```\n%s\n```\n\n%s\n' "$raw" "$FOOTER"
  fi
}

# Short, stable digest of the content — the whole dedupe mechanism.
content_hash() {
  printf '%s' "$1" | sha256sum | cut -c1-16
}

# --------------------------------------------------------------------------
# GitHub REST
# --------------------------------------------------------------------------

# Id of the existing keel comment, or empty.
find_comment_id() {
  gh api "repos/${GITHUB_REPOSITORY}/issues/${PR_NUMBER}/comments" --paginate \
    --jq ".[] | select(.body | contains(\"${MARKER}\")) | .id" 2>/dev/null | head -n1
}

# The body-hash recorded in comment $1, or empty when it carries none.
comment_hash() {
  gh api "repos/${GITHUB_REPOSITORY}/issues/comments/$1" --jq '.body' 2>/dev/null |
    grep -m1 -o 'keel:body-hash [0-9a-f]*' | cut -d' ' -f2
}

write_summary() {
  [ -n "${GITHUB_STEP_SUMMARY:-}" ] || return 0
  printf '%s\n' "$1" >>"$GITHUB_STEP_SUMMARY"
}

# --------------------------------------------------------------------------

main() {
  local file="${1:-}"
  local raw content hash body existing old_hash

  # No file at all means keel review did not produce a report (it failed, or
  # never ran). An empty file means it ran and found nothing — a real result.
  # Only the second one is safe to publish as "clean".
  if [ -z "$file" ] || [ ! -f "$file" ]; then
    echo "keel: no review output to publish — leaving any existing comment untouched"
    return 0
  fi

  raw="$(cat "$file")"
  content="$(render_content "$raw")"
  hash="$(content_hash "$content")"
  body="${MARKER}
<!-- keel:body-hash ${hash} -->
${content}"

  # The summary always carries the report, whatever happens to the comment.
  write_summary "$content"

  if [ -z "${PR_NUMBER:-}" ]; then
    echo "keel: not a pull request — the report is in the step summary"
    return 0
  fi
  if [ "${IS_FORK:-false}" = "true" ]; then
    echo "keel: pull request from a fork — its token cannot comment, so the report is in the step summary only"
    return 0
  fi
  if ! command -v gh >/dev/null 2>&1; then
    echo "keel: gh is not installed on this runner — the report is in the step summary only"
    return 0
  fi

  existing="$(find_comment_id)"

  if [ -z "$existing" ] && [ -z "$raw" ]; then
    # Clean diff and nothing posted yet: say nothing at all. Same contract as
    # `keel compile` exiting 0 with empty stdout.
    echo "keel: nothing to report and no existing comment — not posting"
    return 0
  fi

  if [ -n "$existing" ]; then
    old_hash="$(comment_hash "$existing")"
    if [ "$old_hash" = "$hash" ]; then
      echo "keel: comment ${existing} is already current (${hash}) — not updating"
      return 0
    fi
    if gh api -X PATCH "repos/${GITHUB_REPOSITORY}/issues/comments/${existing}" \
      -f body="$body" >/dev/null; then
      echo "keel: updated comment ${existing} (${old_hash:-none} -> ${hash})"
    else
      echo "keel: could not update comment ${existing} (read-only token?) — the report is in the step summary"
    fi
    return 0
  fi

  if gh api "repos/${GITHUB_REPOSITORY}/issues/${PR_NUMBER}/comments" \
    -f body="$body" >/dev/null; then
    echo "keel: posted the review comment (${hash})"
  else
    echo "keel: could not post a comment (read-only token?) — the report is in the step summary"
  fi
}

main "$@"
