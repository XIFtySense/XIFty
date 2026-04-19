#!/usr/bin/env bash
# adw_harvest.sh — standalone harvest pass. Generate new GitHub Issues from
# observed work in the host repo (TODOs, failing CI, gaps vs VISION, etc.).
#
# Usage: ./adws/adw_harvest.sh [--cap N]

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

CAP=5
while [ $# -gt 0 ]; do
  case "$1" in
    --cap) CAP="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 64 ;;
  esac
done

start_ts=$(date +%s)
log "harvest starting (cap: $CAP)"

run_subagent loswf-harvester "Scan the codebase, recent commits, failing CI, and stale PRs. Generate up to $CAP new GitHub Issues for work the factory should do. Honor work-key dedupe and .loswf/config.yaml curator ignore lists. Report candidates considered, dedupe rejections, and issues created."

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event harvest 0 adw_harvest done "$dur"
log "harvest complete in ${dur}ms"
