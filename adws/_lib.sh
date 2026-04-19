#!/usr/bin/env bash
# _lib.sh — shared helpers for loswf2 ADW scripts. Source from any adw_*.sh.

set -euo pipefail

LOSWF_STATE_DIR="${LOSWF_STATE_DIR:-.loswf/state}"
LOSWF_EVENT_LOG="$LOSWF_STATE_DIR/events.jsonl"

mkdir -p "$LOSWF_STATE_DIR"

# emit_event <phase> <issue> <agent> <outcome> [duration_ms]
emit_event() {
  local phase="$1" issue="$2" agent="$3" outcome="$4" dur="${5:-0}"
  local ts
  ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '{"ts":"%s","phase":"%s","issue":%s,"agent":"%s","outcome":"%s","duration_ms":%s}\n' \
    "$ts" "$phase" "$issue" "$agent" "$outcome" "$dur" >> "$LOSWF_EVENT_LOG"
}

# rerun_context <issue> <kind>
# Print a prompt suffix carrying the last rejection feedback so the agent knows
# this is a second pass and what to address. <kind> is "plan" (scan issue
# comments) or "build" (scan the linked PR's comments). Empty output means
# first pass / no prior rejection — caller should pass the normal prompt.
rerun_context() {
  local issue="$1" kind="$2" body=""
  if [ "$kind" = "plan" ]; then
    body=$(gh issue view "$issue" --json comments --jq \
      '[.comments[] | select((.body // "") | test("^(rejected:|revise|changes requested)"; "i"))] | last.body // ""' 2>/dev/null)
  else
    local pr
    pr=$(gh pr list --search "linked:$issue" --state open --json number --jq '.[0].number' 2>/dev/null)
    [ -z "$pr" ] && return 0
    body=$(gh pr view "$pr" --json comments --jq \
      '[.comments[] | select((.body // "") | test("^rejected:"; "i"))] | last.body // ""' 2>/dev/null)
  fi
  [ -z "$body" ] && return 0
  printf '\n\nNOTE: This is a re-run after rejection. Address the reviewer feedback below specifically.\n\nPrevious reviewer feedback:\n---\n%s\n---\n' "$body"
}

# run_subagent <agent-name> <prompt> [issue] [hook-agent]
# Invoke a Claude Code subagent headlessly.
# - <agent-name>  drives the prompt ("Use the X subagent. ...")
# - [issue]       passed to the post-agent Stop hook for state transitions (default 0)
# - [hook-agent]  override the identity reported to the hook; defaults to <agent-name>.
#                 Use this when the prompt invokes a slash command via the `default`
#                 agent but the hook should treat it as e.g. loswf-ship.
# Returns claude's exit code; output streams to stdout.
run_subagent() {
  local agent="$1" prompt="$2" issue="${3:-0}" hook_agent="${4:-$1}"
  LOSWF_CURRENT_AGENT="$hook_agent" LOSWF_CURRENT_ISSUE="$issue" \
    claude -p "Use the $agent subagent. $prompt" \
      --output-format stream-json --verbose \
      --permission-mode default
}

# get_phase <issue-num> — print the current factory:phase:* label, or empty.
get_phase() {
  gh issue view "$1" --json labels --jq \
    '.labels[].name | select(startswith("factory:phase:"))' 2>/dev/null | head -1
}

# set_phase <issue-num> <new-phase> — replace the phase label atomically.
set_phase() {
  local issue="$1" new="$2" current
  current=$(get_phase "$issue" || true)
  local args=()
  [ -n "$current" ] && args+=(--remove-label "$current")
  args+=(--add-label "factory:phase:$new")
  gh issue edit "$issue" "${args[@]}" >/dev/null
}

# require_phase <issue-num> <expected-phase> — exit non-zero if mismatch.
require_phase() {
  local issue="$1" expected="$2" actual
  actual=$(get_phase "$issue" | sed 's/factory:phase://')
  if [ "$actual" != "$expected" ]; then
    echo "issue #$issue: expected phase '$expected', got '${actual:-none}'" >&2
    exit 64
  fi
}

# has_status <issue-num> <status> — 0 if label present, 1 otherwise.
has_status() {
  gh issue view "$1" --json labels --jq \
    ".labels[].name | select(. == \"factory:status:$2\" or . == \"factory:hold\")" \
    | grep -q . && return 0 || return 1
}

# has_label <issue-num> <label> — 0 if label present, 1 otherwise.
has_label() {
  gh issue view "$1" --json labels --jq \
    ".labels[].name | select(. == \"$2\")" \
    | grep -q . && return 0 || return 1
}

# list_depends_on <issue-num> — print each sibling number from factory:depends-on:<N>.
list_depends_on() {
  gh issue view "$1" --json labels --jq \
    '.labels[].name | select(startswith("factory:depends-on:")) | sub("factory:depends-on:"; "")' \
    2>/dev/null
}

# depends_satisfied <issue-num> — 0 if every declared dependency is factory:phase:done.
# 0 if no depends-on labels. 1 if any sibling is not yet done.
depends_satisfied() {
  local issue="$1" sibling sibling_phase
  while read -r sibling; do
    [ -z "$sibling" ] && continue
    sibling_phase=$(get_phase "$sibling" | sed 's/factory:phase://')
    if [ "$sibling_phase" != "done" ]; then
      return 1
    fi
  done < <(list_depends_on "$issue")
  return 0
}

# all_children_done <parent-num> — 0 if ≥1 children and all are factory:phase:done.
all_children_done() {
  local parent="$1" count=0 not_done=0
  while read -r child; do
    [ -z "$child" ] && continue
    count=$((count + 1))
    local phase
    phase=$(get_phase "$child" | sed 's/factory:phase://')
    [ "$phase" != "done" ] && not_done=$((not_done + 1))
  done < <(gh issue list --state all --label "factory:parent:$parent" \
             --json number --jq '.[].number' 2>/dev/null)
  [ "$count" -ge 1 ] && [ "$not_done" -eq 0 ]
}

# log <message> — timestamped stderr log.
log() {
  printf '[loswf2 %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2
}

# acquire_lock <name> — portable single-instance guard via atomic mkdir.
# Exits the calling script with code 75 (EX_TEMPFAIL) if another holder
# is alive; takes over a stale lock whose pid no longer exists. Auto-
# released on EXIT/INT/TERM via trap.
acquire_lock() {
  local name="$1"
  local lock="$LOSWF_STATE_DIR/$name.lock"
  if mkdir "$lock" 2>/dev/null; then
    :
  else
    local other=""
    [ -r "$lock/pid" ] && other=$(cat "$lock/pid" 2>/dev/null || true)
    if [ -n "$other" ] && kill -0 "$other" 2>/dev/null; then
      log "$name: another instance is running (pid=$other) — exiting"
      exit 75
    fi
    log "$name: stale lock from pid=${other:-unknown} — taking over"
    rm -rf "$lock"
    mkdir "$lock" || { log "$name: failed to acquire lock"; exit 75; }
  fi
  echo $$ > "$lock/pid"
  # shellcheck disable=SC2064
  trap "rm -rf '$lock'" EXIT INT TERM
}
