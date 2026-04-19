# adws — AI Developer Workflows

Shell scripts that compose `.claude/` primitives via headless `claude -p` invocations. Each ADW is single-purpose and emits structured events to `.loswf/state/events.jsonl`.

## Scripts

| Script | Purpose | Trigger |
| --- | --- | --- |
| `adw_feature.sh <issue>` | Full pipeline: intake → (architect)? → plan → review → build → review → ship | Per-issue, manual or webhook |
| `adw_chore.sh <issue>` | Abbreviated pipeline for XS chores (skips planning) | Per-issue; falls through to feature if S+ |
| `adw_review.sh <pr>` | Standalone review pass on a PR (factory or human-opened) | Per-PR |
| `adw_sweep.sh [--max-builds N]` | One full sweep cycle across all eligible issues | Cron / scheduled |
| `adw_harvest.sh [--cap N]` | Generate new issues from observed work | Scheduled (e.g. nightly) |
| `_lib.sh` | Shared helpers: `emit_event`, `run_subagent`, `get_phase`, `set_phase`, `require_phase`, `has_status`, `has_label`, `list_depends_on`, `depends_satisfied`, `all_children_done`, `log` | Sourced by all ADWs |

## Conventions

- POSIX-ish bash, `set -euo pipefail`
- Source `_lib.sh` from each ADW; never duplicate helpers
- Use `run_subagent <name> "<prompt>"` for headless invocation (wraps `claude -p`)
- Event log: `.loswf/state/events.jsonl` — one JSON object per line
- Exit codes:
  - `0` — success or graceful skip
  - `64` — usage error
  - `65` — replan cap exceeded
  - `66` — unexpected phase after plan-review
  - `67` — builder failed to reach review phase
  - `68` — no PR linked after build
- Honor `factory:hold` and `factory:status:needs-attention` — skip those issues
- Worktrees (per-issue isolation for parallel sweeps) are left to the calling environment for v1; in-process bash sweep runs in the host checkout

## Scheduling examples

```cron
# Sweep every 5 minutes during work hours
*/5 9-18 * * 1-5  cd /path/to/repo && ./adws/adw_sweep.sh >> .loswf/state/sweep.log 2>&1

# Harvest nightly at 02:00
0 2 * * *  cd /path/to/repo && ./adws/adw_harvest.sh --cap 10 >> .loswf/state/harvest.log 2>&1
```

## Webhook-driven (PITER) hookup

GitHub Issues `opened` or `labeled` event → tunnel (ngrok / Cloudflare) → small receiver invokes:

```bash
./adws/adw_feature.sh "$ISSUE_NUMBER"
```

The receiver should run in a worktree per invocation and de-dupe on issue number to prevent concurrent ADW runs on the same issue.
