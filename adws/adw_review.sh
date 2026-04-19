#!/usr/bin/env bash
# adw_review.sh — standalone review pass on an arbitrary PR.
# Useful for: human-opened PRs, factory PRs that need a re-review after edits,
# or scheduled drift checks on stale open PRs.
#
# Usage: ./adws/adw_review.sh <pr-num>

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

PR="${1:?usage: adw_review.sh <pr-num>}"

start_ts=$(date +%s)
log "starting adw_review for PR #$PR"

# Sanity: PR exists and is open.
state=$(gh pr view "$PR" --json state --jq .state)
if [ "$state" != "OPEN" ]; then
  log "PR #$PR is $state, not OPEN — skipping"; exit 0
fi

# Find the linked issue (if any) for label routing.
ISSUE=$(gh pr view "$PR" --json body,title --jq \
  '(.body + " " + .title) | capture("#(?<n>[0-9]+)").n // empty')

log "reviewer → PR #$PR (linked issue: ${ISSUE:-none})"
run_subagent loswf-reviewer "Review PR #$PR.${ISSUE:+ Linked to issue #$ISSUE — read the plan in specs/drafts/ if present.}"
emit_event review "${ISSUE:-0}" reviewer done

# Optional adversarial pass on non-trivial diffs.
changed=$(gh pr diff "$PR" --name-only | wc -l | tr -d ' ')
if [ "$changed" -ge 5 ]; then
  log "red-team → (changed files: $changed)"
  run_subagent loswf-red-team "Adversarial review of PR #$PR. Focus on security and edge cases."
  emit_event review "${ISSUE:-0}" red-team done
fi

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event complete "${ISSUE:-0}" adw_review done "$dur"
log "adw_review complete for PR #$PR in ${dur}ms"
