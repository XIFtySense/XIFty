---
name: loswf-factory-state
description: Canonical loswf2 GitHub-label vocabulary and state-machine transitions. Load when an agent needs to read or change pipeline state.
---

# Factory State Reference

The loswf2 pipeline state lives entirely on GitHub Issues and PRs via labels. This skill defines the label families and the legal transitions between them.

## Label families

Each issue carries **at most one** label per family. When advancing an issue, replace the existing label in the family — do not stack.

### `factory:type:*` (set at intake, persists)

- `factory:type:feature` — net-new capability
- `factory:type:bug` — defect against shipped behavior
- `factory:type:spike` — time-boxed technical exploration with findings deliverable
- `factory:type:investigate` — technical question requiring code investigation
- `factory:type:chore` — maintenance, dep bumps, comment fixes
- `factory:type:refactor` — internal restructuring, no behavior change
- `factory:type:docs` — documentation only

### `factory:size:*` (set exclusively by the decomposer, on child issues only)

**Size-label invariant.** Parent issues (issues that enter `decomposing`)
never carry a `factory:size:*` label. Only child issues created by the
decomposer in its step-1 emission receive size labels. Intake and
investigator never write size.

- `factory:size:xs` — single file, < 30 LoC, < 15 min total
- `factory:size:s` — single module, < 200 LoC, < 1 hr
- `factory:size:m` — multiple files, < 500 LoC, < 4 hr
- `factory:size:l` — cross-module, design needed, < 1 day
- `factory:size:xl` — multi-day, must be re-sliced before building

### `factory:phase:*` (the state machine)

```
(none)
  └─ triage
       ├─ investigating ─> decomposing ─> awaiting-children ─> rollup ─> done
       │     │                (bug/spike/investigate; retype may route to planning)
       │     └─ (retype) ─> planning
       └─ planning ─> plan-review ─┬─> building ─> review ─> ship ─> rollup ─> done
                          │         │                │          │
                          │         └─> decomposing ─> awaiting-children ─> rollup
                          ↓            ↓           ↓
                       (back to    (back to     (back to
                        planning)   building)    building)
```

- `factory:phase:triage` — intake is processing
- `factory:phase:investigating` — investigator is reproducing a bug, scouting a spike, or answering an investigate question before any sizing or planning
- `factory:phase:planning` — planner (or architect/designer first) is producing a plan
- `factory:phase:plan-review` — plan-reviewer is validating
- `factory:phase:decomposing` — decomposer splitting an approved `l`/`xl` plan (or a confirmed investigation) into sub-issues
- `factory:phase:awaiting-children` — parent blocked until all sub-issues reach `done`
- `factory:phase:building` — builder is implementing
- `factory:phase:review` — PR open, reviewer is evaluating
- `factory:phase:ship` — approved, awaiting merge
- `factory:phase:rollup` — merged, documenter pending
- `factory:phase:done` — complete

### `factory:status:*` (orthogonal health flag)

- `factory:status:needs-clarification` — awaiting human answer in comments
- `factory:status:needs-attention` — pipeline gave up; human must triage
- `factory:status:healing` — escalation agent is attempting repair
- `factory:status:blocked` — waiting on external dependency
- `factory:status:not-a-bug` — investigator could not reproduce a reported bug (paired with `needs-attention` for operator triage)

### `factory:parent:<num>` (dynamic, set by decomposer on sub-issues)

Applied to every sub-issue produced by `loswf-decomposer`. Points to the
parent issue whose approved plan was decomposed. Used by the sweep to gate the
parent's `awaiting-children → rollup` transition.

### `factory:depends-on:<num>` (dynamic, zero or more per sub-issue)

Declares ordering between siblings: a sub-issue with
`factory:depends-on:<N>` will not be built until issue `#N` reaches
`factory:phase:done`. Enforced by `depends_satisfied` in `adws/_lib.sh` during
the sweep's `building` pass.

### `factory:hold` (single label, no family)

When present, all factory automation skips this issue. Only humans remove it.

## Standard label commands

```bash
# Replace label within a family (safe pattern)
gh issue edit <num> --remove-label "factory:phase:planning" --add-label "factory:phase:plan-review"

# List by phase
gh issue list --label "factory:phase:building" --state open

# Check current phase
gh issue view <num> --json labels --jq '.labels[].name | select(startswith("factory:phase:"))'
```

## Legal transitions (enforce in agents)

| From phase | Legal next | Owner |
| --- | --- | --- |
| (none) | `triage` | intake (auto) |
| `triage` | `planning` \| `investigating` \| `needs-clarification` | intake |
| `investigating` | `decomposing` \| `needs-attention` | investigator |
| `planning` | `plan-review` \| `triage` (reset) | planner / architect / designer |
| `plan-review` | `building` \| `planning` (revise) \| `decomposing` (l/xl) \| `triage` (reset) | plan-reviewer |
| `decomposing` | `awaiting-children` \| `needs-attention` \| `planning` (recover) | decomposer |
| `awaiting-children` | `rollup` \| `needs-attention` | sweep (bash `all_children_done`) |
| `building` | `review` \| `needs-attention` \| `planning` (re-plan) | builder / escalation |
| `review` | `ship` \| `building` (revise) \| `needs-attention` \| `planning` (re-plan) | reviewer / escalation |
| `ship` | `rollup` | /ship command |
| `rollup` | `done` \| `review` (regression) | documenter |

Any transition not in this table is illegal. Escalation agent must reject and reroute.
Recovery transitions (`→ planning`, `→ triage`) are the escalation-only escape hatches when a phase produced bogus output and needs to be redone — they are not part of the happy path.

Advisory: `decompose.max_tasks` in `.loswf/config.yaml` is a soft default
guiding how many sub-issues a decomposition should produce. It is not a
hard cap — `post_decomposer` enforces only `N ≥ 2`.
