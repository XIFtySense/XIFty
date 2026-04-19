#!/usr/bin/env bash
# adw_feature.sh — full pipeline on a single GitHub Issue.
# intake → (architect|designer)? → planner → plan-reviewer → builder → reviewer → ship
#
# Usage: ./adws/adw_feature.sh <issue-num>

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

ISSUE="${1:?usage: adw_feature.sh <issue-num>}"

start_ts=$(date +%s)
log "starting adw_feature for issue #$ISSUE"

# Bail if the issue is held or already needs human attention.
if has_status "$ISSUE" needs-attention; then
  log "issue #$ISSUE has needs-attention — skipping"
  exit 0
fi
if gh issue view "$ISSUE" --json labels --jq '.labels[].name' | grep -qx 'factory:hold'; then
  log "issue #$ISSUE is on hold — skipping"
  exit 0
fi

# Phase 1: intake (only if not yet phased)
current=$(get_phase "$ISSUE" || true)
if [ -z "$current" ]; then
  log "intake →"
  run_subagent loswf-intake "Run intake on issue #$ISSUE."
  emit_event intake "$ISSUE" intake done
fi

# Refresh phase + size after intake
phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
size=$(gh issue view "$ISSUE" --json labels --jq '.labels[].name | select(startswith("factory:size:"))' | sed 's/factory:size://')

# Optional design pass for L/XL or UI-tagged issues
if [ "$phase" = "planning" ] && { [ "$size" = "l" ] || [ "$size" = "xl" ]; }; then
  log "architect → (size $size)"
  run_subagent loswf-architect "Produce a design doc for issue #$ISSUE."
  emit_event design "$ISSUE" architect done
fi

# Phase 2: planning
require_phase "$ISSUE" planning
log "planner →"
run_subagent loswf-planner "Produce an implementation plan for issue #$ISSUE."
emit_event planning "$ISSUE" planner done

# Phase 3: plan-review (loop with planner up to 2 revisions)
revisions=0
while true; do
  require_phase "$ISSUE" plan-review
  log "plan-reviewer → (attempt $((revisions + 1)))"
  run_subagent loswf-plan-reviewer "Review the plan for issue #$ISSUE."
  emit_event plan-review "$ISSUE" plan-reviewer done

  phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
  if [ "$phase" = "building" ]; then
    break
  fi
  if [ "$phase" = "planning" ]; then
    revisions=$((revisions + 1))
    if [ "$revisions" -ge 2 ]; then
      log "plan-review hit revision cap — escalating"
      gh issue edit "$ISSUE" --add-label "factory:status:needs-attention" >/dev/null
      emit_event plan-review "$ISSUE" plan-reviewer cap-exceeded
      exit 65
    fi
    log "plan revisions requested — replanning"
    run_subagent loswf-planner "Revise the plan for issue #$ISSUE based on plan-reviewer comments."
    emit_event planning "$ISSUE" planner revise
  else
    log "unexpected phase after plan-review: $phase — escalating"
    exit 66
  fi
done

# Phase 4: build
log "builder →"
run_subagent loswf-builder "Implement the approved plan for issue #$ISSUE."
emit_event building "$ISSUE" builder done

phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
if [ "$phase" != "review" ]; then
  log "builder did not reach review phase (current: $phase) — escalating"
  run_subagent loswf-escalation "Triage failure on issue #$ISSUE: builder ended at phase $phase."
  exit 67
fi

# Phase 5: review
PR=$(gh pr list --search "linked:$ISSUE" --state open --json number --jq '.[0].number')
if [ -z "$PR" ]; then
  log "no open PR linked to #$ISSUE after build — escalating"
  exit 68
fi
log "reviewer → PR #$PR"
run_subagent loswf-reviewer "Review PR #$PR (linked to issue #$ISSUE)."
emit_event review "$ISSUE" reviewer done

phase=$(get_phase "$ISSUE" | sed 's/factory:phase://')
if [ "$phase" != "ship" ]; then
  log "review did not approve (phase: $phase) — exiting"
  exit 0
fi

# Phase 6: ship
log "ship → PR #$PR"
run_subagent default "Run the /ship slash command on PR #$PR."
emit_event ship "$ISSUE" ship done

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event complete "$ISSUE" adw_feature done "$dur"
log "adw_feature complete for #$ISSUE in ${dur}ms"
