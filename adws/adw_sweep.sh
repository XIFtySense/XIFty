#!/usr/bin/env bash
# adw_sweep.sh — one full factory sweep. Advances every eligible issue by one phase.
# Designed to be re-run on a cadence (cron, launchd, GitHub Actions schedule).
#
# Usage: ./adws/adw_sweep.sh [--max-builds N]

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

MAX_BUILDS=3
while [ $# -gt 0 ]; do
  case "$1" in
    --max-builds) MAX_BUILDS="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

acquire_lock sweep

start_ts=$(date +%s)
log "sweep starting (max concurrent builds: $MAX_BUILDS)"

# TODO(code-search): drain .loswf/state/index-queue and run incremental update.
# Uncomment when bin/loswf-index is wired into install.sh and host has Bun:
#   if [ -s "$LOSWF_STATE_DIR/index-queue" ] && command -v bun >/dev/null; then
#     log "code-search: draining $(wc -l < $LOSWF_STATE_DIR/index-queue) queued files"
#     # we don't need the file list — git diff sees all changes since last_sha.
#     bun run "$(dirname "$0")/../bin/loswf-index" update >/dev/null 2>&1 || \
#       log "code-search: incremental update failed (non-fatal)"
#     : > "$LOSWF_STATE_DIR/index-queue"
#   fi

# Helper: list open issue numbers for a phase, excluding held / needs-attention.
list_phase() {
  local phase="$1"
  gh issue list --state open --label "factory:phase:$phase" \
    --json number,labels \
    --jq '.[] | select(([.labels[].name] | any(. == "factory:hold" or . == "factory:status:needs-attention")) | not) | .number'
}

# 1. Triage anything without a phase label.
log "phase: intake"
gh issue list --state open --json number,labels --jq \
  '.[] | select([.labels[].name] | any(startswith("factory:phase:")) | not)
       | select(([.labels[].name] | any(. == "factory:hold")) | not)
       | .number' \
  | while read -r issue; do
      [ -z "$issue" ] && continue
      log "  intake #$issue"
      run_subagent loswf-intake "Run intake on issue #$issue." "$issue" || \
        emit_event intake "$issue" intake error
    done

# 1b. Investigating (bug/spike/investigate types)
log "phase: investigating"
list_phase investigating | while read -r issue; do
  [ -z "$issue" ] && continue
  log "  investigator #$issue"
  run_subagent loswf-investigator \
    "Investigate issue #$issue. Reproduce, scout, or answer per type. Post exactly one verdict marker. Do not apply any factory:size:* label." \
    "$issue" || emit_event investigating "$issue" investigator error
done

# 2. Planning
log "phase: planning"
list_phase planning | while read -r issue; do
  [ -z "$issue" ] && continue
  size=$(gh issue view "$issue" --json labels --jq \
    '.labels[].name | select(startswith("factory:size:"))' | sed 's/factory:size://')
  if [ "$size" = "l" ] || [ "$size" = "xl" ]; then
    log "  architect #$issue (size $size)"
    run_subagent loswf-architect "Produce a design doc for issue #$issue." "$issue" || \
      emit_event design "$issue" architect error
  fi
  log "  planner #$issue"
  ctx=$(rerun_context "$issue" plan)
  run_subagent loswf-planner "Produce an implementation plan for issue #$issue.$ctx" "$issue" || \
    emit_event planning "$issue" planner error
done

# 3. Plan-review
log "phase: plan-review"
list_phase plan-review | while read -r issue; do
  [ -z "$issue" ] && continue
  log "  plan-reviewer #$issue"
  run_subagent loswf-plan-reviewer "Review the plan for issue #$issue." "$issue" || \
    emit_event plan-review "$issue" plan-reviewer error
done

# 3b. Decomposing (split approved l/xl plans into sub-issues)
log "phase: decomposing"
list_phase decomposing | while read -r issue; do
  [ -z "$issue" ] && continue
  log "  decomposer #$issue"
  run_subagent loswf-decomposer "Decompose the approved plan for issue #$issue into sub-issues." "$issue" || \
    emit_event decomposing "$issue" decomposer error
done

# 4. Building (parallelism cap via background jobs)
log "phase: building (cap: $MAX_BUILDS)"
running=0
list_phase building | while read -r issue; do
  [ -z "$issue" ] && continue
  if ! depends_satisfied "$issue"; then
    log "  #$issue: waiting on depends-on siblings"; continue
  fi
  while [ "$(jobs -rp | wc -l | tr -d ' ')" -ge "$MAX_BUILDS" ]; do sleep 5; done
  log "  builder #$issue (background)"
  ctx=$(rerun_context "$issue" build)
  ( run_subagent loswf-builder "Implement the approved plan for issue #$issue.$ctx" "$issue" \
      || emit_event building "$issue" builder error ) &
  running=$((running + 1))
done
wait
log "  $running builds completed"

# 4b. Awaiting-children — belt-and-suspenders rollup promotion.
# post_documenter's check_parent_rollup also covers this, but a bash-only
# sweep pass ensures parents advance even when the last child was documented
# outside the sweep (e.g. manually closed).
log "phase: awaiting-children"
list_phase awaiting-children | while read -r issue; do
  [ -z "$issue" ] && continue
  if all_children_done "$issue"; then
    set_phase "$issue" rollup
    emit_event awaiting-children "$issue" sweep promoted
  fi
done

# 5. Review
log "phase: review"
list_phase review | while read -r issue; do
  [ -z "$issue" ] && continue
  pr=$(gh pr list --search "linked:$issue" --state open --json number --jq '.[0].number')
  if [ -z "$pr" ]; then
    log "  #$issue: no linked PR, skipping"; continue
  fi
  log "  reviewer PR #$pr (issue #$issue)"
  run_subagent loswf-reviewer "Review PR #$pr (linked to #$issue)." "$issue" || \
    emit_event review "$issue" reviewer error
done

# TODO(code-search): after a successful merge in the ship phase, run
# `bun run bin/loswf-index update` so the post-merge HEAD becomes the new
# baseline before reviewers/builders on subsequent issues query the index.
# 6. Ship
log "phase: ship"
list_phase ship | while read -r issue; do
  [ -z "$issue" ] && continue
  pr=$(gh pr list --search "linked:$issue" --state open --json number --jq '.[0].number')
  [ -z "$pr" ] && continue
  log "  ship PR #$pr"
  run_subagent default "Run the /ship slash command on PR #$pr." "$issue" loswf-ship || \
    emit_event ship "$issue" ship error
done

# 7. Rollup → documenter
log "phase: rollup"
list_phase rollup | while read -r issue; do
  [ -z "$issue" ] && continue
  log "  documenter #$issue"
  run_subagent loswf-documenter "Update docs based on the merged work for issue #$issue." "$issue" || \
    emit_event rollup "$issue" documenter error
done

# 8. Harvest (one-shot per sweep)
log "phase: harvest"
run_subagent loswf-harvester "Scan for new work and create issues. Cap at 5." || \
  emit_event harvest 0 harvester error

# 9. Curator (cadence-gated). Keyed off mtime of .loswf/state/curator.last_run
# vs curator.cadence_hours (default 1). Absent marker → run. Matches the awk
# idiom used by adws/adw_loop.sh to read sweep.cycle_interval_minutes.
CADENCE_HOURS=$(awk '/^curator:/{f=1} f && /cadence_hours:/{print $2; exit}' \
                  .loswf/config.yaml 2>/dev/null || true)
CADENCE_HOURS="${CADENCE_HOURS:-1}"
CURATOR_MARK="$LOSWF_STATE_DIR/curator.last_run"
if [ ! -f "$CURATOR_MARK" ] || \
   find "$CURATOR_MARK" -mmin +$((CADENCE_HOURS * 60)) -print 2>/dev/null | grep -q .; then
  log "phase: curator"
  run_subagent loswf-curator "Run a curator pass. Reconcile label families, close stale needs-clarification issues older than 14 days, drop healing labels that have been green for 24 hours, cleanup merged worktrees, and open PRs for any proposed config/prompt changes. Honor .loswf/config.yaml curator ignore lists." || \
    emit_event curator 0 curator error
  touch "$CURATOR_MARK"
else
  log "phase: curator (skipped — last run under ${CADENCE_HOURS}h ago)"
fi

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event sweep 0 adw_sweep done "$dur"
log "sweep complete in ${dur}ms"
