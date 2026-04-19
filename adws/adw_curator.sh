#!/usr/bin/env bash
# adw_curator.sh — standalone curator pass. Reconciles label hygiene, closes
# stale needs-clarification issues, drops healing labels that have been green
# long enough, cleans up merged worktrees, and opens PRs for any proposed
# config/prompt changes.
#
# Usage: ./adws/adw_curator.sh

set -euo pipefail

# shellcheck source=./_lib.sh
. "$(dirname "$0")/_lib.sh"

start_ts=$(date +%s)
log "curator starting"

run_subagent loswf-curator "Run a curator pass. Reconcile label families, close stale needs-clarification issues older than 14 days, drop healing labels that have been green for 24 hours, cleanup merged worktrees, and open PRs for any proposed config/prompt changes. Honor .loswf/config.yaml curator ignore lists."

dur=$(( ($(date +%s) - start_ts) * 1000 ))
emit_event curator 0 adw_curator done "$dur"
log "curator complete in ${dur}ms"
