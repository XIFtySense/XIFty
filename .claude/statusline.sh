#!/usr/bin/env bash
# Factory-aware status line.
#
# Segments (left→right):
#   [Model] cwd | 🌿branch | 🏭 <phase or idle> | ▶ <subagent Ns> | ctx N% | $X.XX
#
# Cheap sources only: reads .loswf/state/ ledgers and git. The factory issue
# counts are cached to `.loswf/state/statusline_counts.json` with a 60s TTL so
# the shell-out to `gh` does not run on every event-driven refresh.

set -u

input=$(cat)

MODEL=$(printf '%s' "$input" | jq -r '.model.display_name // "?"')
CWD=$(printf '%s' "$input" | jq -r '.workspace.current_dir // .cwd // "."')
PROJECT=$(printf '%s' "$input" | jq -r '.workspace.project_dir // .cwd // "."')
PCT=$(printf '%s' "$input" | jq -r '(.context_window.used_percentage // 0) | floor')
COST=$(printf '%s' "$input" | jq -r '.cost.total_cost_usd // 0')
TRANSCRIPT=$(printf '%s' "$input" | jq -r '.transcript_path // ""')
SESSION=$(printf '%s' "$input" | jq -r '.session_id // ""')

C_DIM='\033[2m'
C_CYAN='\033[36m'
C_GREEN='\033[32m'
C_YELLOW='\033[33m'
C_MAGENTA='\033[35m'
C_RESET='\033[0m'

cd "$CWD" 2>/dev/null || true

BRANCH=""
if git rev-parse --git-dir >/dev/null 2>&1; then
  BRANCH=$(git branch --show-current 2>/dev/null)
fi

STATE_DIR="$PROJECT/.loswf/state"

# Active subagent: most recent entry in subagent_timing.jsonl whose id is NOT
# in subagent_closed.jsonl. Cheap: only scans tails.
ACTIVE=""
ACTIVE_AGE=""
if [ -f "$STATE_DIR/subagent_timing.jsonl" ]; then
  CLOSED_IDS=""
  [ -f "$STATE_DIR/subagent_closed.jsonl" ] && CLOSED_IDS=$(tail -n 50 "$STATE_DIR/subagent_closed.jsonl" 2>/dev/null | jq -r '.id // empty' 2>/dev/null | tr '\n' ' ')
  # Walk timing ledger from newest to find an open one.
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    ID=$(printf '%s' "$line" | jq -r '.id // empty' 2>/dev/null)
    [ -z "$ID" ] && continue
    case " $CLOSED_IDS " in *" $ID "*) continue ;; esac
    TS=$(printf '%s' "$line" | jq -r '.ts // 0' 2>/dev/null)
    ROLE=$(printf '%s' "$line" | jq -r '.role // "subagent"' 2>/dev/null)
    NOW=$(date +%s)
    AGE=$(( NOW - ${TS%.*} ))
    # Treat anything >30min as stale/orphaned; ignore.
    [ "$AGE" -gt 1800 ] && continue
    ACTIVE="$ROLE"
    ACTIVE_AGE="${AGE}s"
    break
  done < <(tac "$STATE_DIR/subagent_timing.jsonl" 2>/dev/null || tail -r "$STATE_DIR/subagent_timing.jsonl" 2>/dev/null)
fi

# Factory phase counts, cached to `$STATE_DIR/statusline_counts.json` (60s TTL).
PHASES=""
COUNTS_FILE="$STATE_DIR/statusline_counts.json"
REPO=""
if [ -f "$PROJECT/.loswf/config.yaml" ]; then
  REPO=$(grep -E '^repo:' "$PROJECT/.loswf/config.yaml" | awk '{print $2}' | tr -d '"')
fi
if [ -n "$REPO" ]; then
  NEED_REFRESH=1
  if [ -f "$COUNTS_FILE" ]; then
    MTIME=$(stat -f %m "$COUNTS_FILE" 2>/dev/null || stat -c %Y "$COUNTS_FILE" 2>/dev/null || echo 0)
    NOW=$(date +%s)
    AGE=$(( NOW - MTIME ))
    [ "$AGE" -lt 60 ] && NEED_REFRESH=0
  fi
  if [ "$NEED_REFRESH" = 1 ]; then
    mkdir -p "$STATE_DIR" 2>/dev/null
    ( gh issue list --repo "$REPO" --state open --limit 100 \
        --json number,labels \
        --jq '[.[] | .labels[].name | select(startswith("factory:phase:"))] | group_by(.) | map({key: (.[0] | sub("factory:phase:"; "")), value: length}) | from_entries' \
      > "$COUNTS_FILE.tmp" 2>/dev/null && mv "$COUNTS_FILE.tmp" "$COUNTS_FILE" ) &
  fi
  if [ -f "$COUNTS_FILE" ]; then
    # Prioritize active phases.
    PHASES=$(jq -r '[
      if .building          then "b:\(.building)"               else empty end,
      if .review            then "r:\(.review)"                 else empty end,
      if .ship              then "sh:\(.ship)"                  else empty end,
      if ."plan-review"     then "pr:\(."plan-review")"         else empty end,
      if .planning          then "pl:\(.planning)"              else empty end,
      if .decomposing       then "dc:\(.decomposing)"           else empty end,
      if ."awaiting-children" then "aw:\(."awaiting-children")" else empty end,
      if .investigating     then "iv:\(.investigating)"         else empty end,
      if .intake            then "in:\(.intake)"                else empty end,
      if .triage            then "t:\(.triage)"                 else empty end,
      if .rollup            then "ro:\(.rollup)"                else empty end
    ] | join(" ")' "$COUNTS_FILE" 2>/dev/null)
  fi
fi

# Cumulative I/O tokens, parsed from transcript JSONL. Cached per-session
# keyed on transcript mtime so we only re-scan when the file changes.
TOK_IN=0
TOK_OUT=0
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ] && [ -n "$SESSION" ]; then
  TOK_CACHE="$STATE_DIR/statusline_tokens_${SESSION}.json"
  T_MTIME=$(stat -f %m "$TRANSCRIPT" 2>/dev/null || stat -c %Y "$TRANSCRIPT" 2>/dev/null || echo 0)
  C_MTIME=0
  if [ -f "$TOK_CACHE" ]; then
    C_MTIME=$(jq -r '.mtime // 0' "$TOK_CACHE" 2>/dev/null || echo 0)
  fi
  if [ "$T_MTIME" != "$C_MTIME" ]; then
    mkdir -p "$STATE_DIR" 2>/dev/null
    SUMS=$(jq -r 'select(.message.usage) | .message.usage | "\((.input_tokens // 0) + (.cache_read_input_tokens // 0) + (.cache_creation_input_tokens // 0)) \(.output_tokens // 0)"' "$TRANSCRIPT" 2>/dev/null \
      | awk 'BEGIN{i=0;o=0} {i+=$1; o+=$2} END{printf "%d %d", i, o}')
    TOK_IN=${SUMS%% *}
    TOK_OUT=${SUMS##* }
    printf '{"mtime":%s,"in":%s,"out":%s}\n' "$T_MTIME" "$TOK_IN" "$TOK_OUT" > "$TOK_CACHE.tmp" 2>/dev/null \
      && mv "$TOK_CACHE.tmp" "$TOK_CACHE"
  else
    TOK_IN=$(jq -r '.in // 0' "$TOK_CACHE" 2>/dev/null || echo 0)
    TOK_OUT=$(jq -r '.out // 0' "$TOK_CACHE" 2>/dev/null || echo 0)
  fi
fi

fmt_tokens() {
  local n=$1
  if [ "$n" -ge 1000000 ]; then awk "BEGIN{printf \"%.1fM\", $n/1000000}"
  elif [ "$n" -ge 1000 ]; then awk "BEGIN{printf \"%.1fk\", $n/1000}"
  else printf '%d' "$n"
  fi
}

# Assemble.
LINE="[${C_CYAN}${MODEL}${C_RESET}] ${CWD##*/}"
[ -n "$BRANCH" ] && LINE="$LINE ${C_DIM}|${C_RESET} 🌿${BRANCH}"
if [ -n "$PHASES" ]; then
  LINE="$LINE ${C_DIM}|${C_RESET} 🏭 ${C_GREEN}${PHASES}${C_RESET}"
elif [ -n "$REPO" ]; then
  LINE="$LINE ${C_DIM}|${C_RESET} 🏭 ${C_DIM}idle${C_RESET}"
fi
if [ -n "$ACTIVE" ]; then
  LINE="$LINE ${C_DIM}|${C_RESET} ${C_MAGENTA}▶ ${ACTIVE} ${ACTIVE_AGE}${C_RESET}"
fi

# Color context by pressure.
if [ "$PCT" -ge 95 ]; then CTX_COLOR='\033[31m'
elif [ "$PCT" -ge 80 ]; then CTX_COLOR="$C_YELLOW"
else CTX_COLOR="$C_DIM"
fi
LINE="$LINE ${C_DIM}|${C_RESET} ${CTX_COLOR}ctx ${PCT}%${C_RESET}"

if [ "$TOK_IN" -gt 0 ] || [ "$TOK_OUT" -gt 0 ]; then
  LINE="$LINE ${C_DIM}|${C_RESET} ${C_DIM}↑$(fmt_tokens "$TOK_IN") ↓$(fmt_tokens "$TOK_OUT")${C_RESET}"
fi

COST_FMT=$(printf '%.2f' "$COST" 2>/dev/null || echo "0.00")
LINE="$LINE ${C_DIM}|${C_RESET} \$${COST_FMT}"

printf '%b\n' "$LINE"
