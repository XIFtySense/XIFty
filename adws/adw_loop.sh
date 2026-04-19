#!/usr/bin/env bash
# adw_loop.sh — run adw_sweep.sh continuously, sleeping between cycles.
#
# Usage:
#   ./adws/adw_loop.sh [--interval-min N] [--max-cycles N] [--min-interval-sec N]
#
# Defaults:
#   --interval-min:      pulled from .loswf/config.yaml `sweep.cycle_interval_minutes`,
#                        else 10. Used when the factory is idle.
#   --min-interval-sec:  short sleep when issues are still queued in any active
#                        phase (planning, plan-review, building, review, ship,
#                        rollup). Default 30. Set to 0 to skip sleep entirely
#                        when busy.
#   --max-cycles:        unlimited (loop forever).
#
# Stop conditions:
#   - touch .loswf/state/stop  → graceful exit at next cycle boundary
#   - max-cycles reached
#   - SIGINT / SIGTERM
#
# Each cycle is logged to .loswf/state/loop.log with timestamp + duration.

set -euo pipefail

# shellcheck source=adws/_lib.sh
. "$(dirname "$0")/_lib.sh"

INTERVAL_MIN=""
MAX_CYCLES=0
MIN_INTERVAL_SEC=""

while [ $# -gt 0 ]; do
  case "$1" in
    --interval-min)     INTERVAL_MIN="$2"; shift 2 ;;
    --min-interval-sec) MIN_INTERVAL_SEC="$2"; shift 2 ;;
    --max-cycles)       MAX_CYCLES="$2"; shift 2 ;;
    -h|--help)          sed -n '2,22p' "$0"; exit 0 ;;
    *)                  echo "unknown flag: $1" >&2; exit 64 ;;
  esac
done

if [ -z "$INTERVAL_MIN" ] && [ -f .loswf/config.yaml ]; then
  INTERVAL_MIN=$(awk '/^sweep:/{f=1} f && /cycle_interval_minutes:/{print $2; exit}' \
                   .loswf/config.yaml 2>/dev/null || true)
fi
INTERVAL_MIN="${INTERVAL_MIN:-10}"
INTERVAL_SEC=$(( INTERVAL_MIN * 60 ))

if [ -z "$MIN_INTERVAL_SEC" ] && [ -f .loswf/config.yaml ]; then
  MIN_INTERVAL_SEC=$(awk '/^sweep:/{f=1} f && /min_interval_sec:/{print $2; exit}' \
                       .loswf/config.yaml 2>/dev/null || true)
fi
MIN_INTERVAL_SEC="${MIN_INTERVAL_SEC:-30}"

# pending_count — open issues queued in an active phase (excluding hold/needs-attention).
pending_count() {
  gh issue list --state open --limit 200 --json labels --jq '
    [ .[]
      | select([.labels[].name] | any(
          . == "factory:phase:investigating" or
          . == "factory:phase:planning" or
          . == "factory:phase:plan-review" or
          . == "factory:phase:decomposing" or
          . == "factory:phase:building" or
          . == "factory:phase:review" or
          . == "factory:phase:ship" or
          . == "factory:phase:rollup"))
      | select(([.labels[].name] | any(
          . == "factory:hold" or
          . == "factory:status:needs-attention")) | not)
    ] | length' 2>/dev/null || echo 0
}

STOP_FILE="$LOSWF_STATE_DIR/stop"
LOOP_LOG="$LOSWF_STATE_DIR/loop.log"
SWEEP="$(cd "$(dirname "$0")" && pwd)/adw_sweep.sh"

trap 'log "loop: SIGTERM/SIGINT — exiting"; exit 0' INT TERM

log "loop: starting (idle-interval=${INTERVAL_MIN}m, busy-interval=${MIN_INTERVAL_SEC}s, max-cycles=${MAX_CYCLES:-∞})"
log "loop: stop with: touch $STOP_FILE"

cycle=0
while :; do
  if [ -f "$STOP_FILE" ]; then
    log "loop: stop file detected — exiting and removing $STOP_FILE"
    rm -f "$STOP_FILE"
    exit 0
  fi
  if [ "$MAX_CYCLES" != 0 ] && [ "$cycle" -ge "$MAX_CYCLES" ]; then
    log "loop: reached max-cycles=$MAX_CYCLES — exiting"
    exit 0
  fi

  cycle=$((cycle + 1))
  start=$(date +%s)
  log "loop: cycle $cycle starting"
  printf '[%s] cycle %d start\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$cycle" >> "$LOOP_LOG"

  if "$SWEEP"; then
    rc=0
  else
    rc=$?
  fi
  end=$(date +%s)
  dur=$((end - start))
  printf '[%s] cycle %d end rc=%d duration=%ds\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$cycle" "$rc" "$dur" >> "$LOOP_LOG"
  pending=$(pending_count)
  if [ "$pending" -gt 0 ]; then
    effective=$MIN_INTERVAL_SEC
    log "loop: cycle $cycle done (rc=$rc, ${dur}s) — $pending issue(s) queued, sleeping ${effective}s"
  else
    effective=$INTERVAL_SEC
    log "loop: cycle $cycle done (rc=$rc, ${dur}s) — idle, sleeping ${INTERVAL_MIN}m"
  fi

  # Sleep in small chunks so the stop file is detected within ~5s.
  remaining=$effective
  while [ "$remaining" -gt 0 ]; do
    [ -f "$STOP_FILE" ] && break
    nap=$(( remaining < 5 ? remaining : 5 ))
    sleep "$nap"
    remaining=$(( remaining - nap ))
  done
done
