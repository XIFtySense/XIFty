#!/usr/bin/env bash
# adw_chore.sh — abbreviated pipeline for XS chores.
# intake → builder (skipping plan) → reviewer → ship
#
# Falls through to adw_feature.sh if intake sizes the issue larger than XS.
#
# Usage: ./adws/adw_chore.sh <issue-num>

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

ISSUE="${1:?usage: adw_chore.sh <issue-num>}"

start_ts=$(date +%s)
log "starting adw_chore for issue #$ISSUE"

if has_status "$ISSUE" needs-attention; then
  log "issue #$ISSUE has needs-attention — skipping"; exit 0
fi

# Intake (only if not phased)
if [ -z "$(get_phase "$ISSUE" || true)" ]; then
  log "intake →"
  run_subagent loswf-intake "Run intake on issue #$ISSUE."
  emit_event intake "$ISSUE" intake done
fi

size=$(gh issue view "$ISSUE" --json labels --jq \
  '.labels[].name | select(startswith("factory:size:"))' | sed 's/factory:size://')

if [ "$size" != "xs" ]; then
  log "size=$size, not xs — delegating to adw_feature.sh"
  exec "$(dirname "$0")/adw_feature.sh" "$ISSUE"
fi

# XS path: jump straight to building.
set_phase "$ISSUE" building
log "builder → (no plan, XS chore)"
run_subagent loswf-builder "Implement issue #$ISSUE directly. The issue body is the plan; no separate spec file. Keep the diff minimal."
emit_event building "$ISSUE" builder done

phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
if [ "$phase" != "review" ]; then
  log "builder did not reach review (phase: $phase) — escalating"
  run_subagent loswf-escalation "Triage failure on chore #$ISSUE: builder ended at phase $phase."
  exit 67
fi

PR=$(gh pr list --search "linked:$ISSUE" --state open --json number --jq '.[0].number')
[ -z "$PR" ] && { log "no PR linked to #$ISSUE"; exit 68; }

log "reviewer → PR #$PR"
run_subagent loswf-reviewer "Review PR #$PR (chore, linked to issue #$ISSUE)."
emit_event review "$ISSUE" reviewer done

phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
if [ "$phase" != "ship" ]; then
  log "review did not approve (phase: $phase) — exiting"; exit 0
fi

log "ship → PR #$PR"
run_subagent default "Run the /ship slash command on PR #$PR."
emit_event ship "$ISSUE" ship done

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event complete "$ISSUE" adw_chore done "$dur"
log "adw_chore complete for #$ISSUE in ${dur}ms"
