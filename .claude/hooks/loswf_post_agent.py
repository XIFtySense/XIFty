#!/usr/bin/env python3
"""Stop / SubagentStop hook — deterministic factory:phase transitions.

After each agent run (ADW-driven or interactive Task-tool invocation),
observe repo state (gh issue, gh pr, git diff) and apply the correct
transition. Agents stay strategic; this hook owns state.

Identification — two paths:

  1. ADW path: $LOSWF_CURRENT_AGENT + $LOSWF_CURRENT_ISSUE are set by
     `adws/_lib.sh::run_subagent`. Preferred when present.
  2. Interactive path: stdin payload includes `transcript_path`. The hook
     scans the JSONL backward for the most recent completed Agent tool_use,
     extracts `subagent_type` and parses the issue number from
     `description` (e.g. "Intake #238", "Plan #96"). Dispatches once per
     tool_use_id; a dedupe ledger at `.loswf/state/dispatched.jsonl`
     prevents re-dispatch across Stop and SubagentStop firings.

This hook runs gh/git directly as subprocesses, so it bypasses the
PreToolUse label_transition validator by design — that validator constrains
agents; the orchestrator computes the truth.

Hook wiring (in .claude/settings.json under Stop **and** SubagentStop):
  {"type": "command", "command": ".claude/hooks/loswf_post_agent.py"}
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

STATE_DIR = Path(os.environ.get("LOSWF_STATE_DIR", ".loswf/state"))
EVENT_LOG = STATE_DIR / "events.jsonl"


def gh_json(args):
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        return None
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return None


def get_phase(issue):
    data = gh_json(["issue", "view", str(issue), "--json", "labels"])
    if not data:
        return None
    for l in data.get("labels", []):
        if l["name"].startswith("factory:phase:"):
            return l["name"].split(":")[-1]
    return None


def get_size(issue):
    """Return the `factory:size:*` suffix (e.g. 's', 'l') or None."""
    data = gh_json(["issue", "view", str(issue), "--json", "labels"])
    if not data:
        return None
    for l in data.get("labels", []):
        if l["name"].startswith("factory:size:"):
            return l["name"].split(":")[-1]
    return None


def get_parent(issue):
    """Return the int from a `factory:parent:<num>` label, or None."""
    data = gh_json(["issue", "view", str(issue), "--json", "labels"])
    if not data:
        return None
    for l in data.get("labels", []):
        if l["name"].startswith("factory:parent:"):
            try:
                return int(l["name"].split(":")[-1])
            except ValueError:
                return None
    return None


def set_phase(issue, new):
    cur = get_phase(issue)
    args = ["issue", "edit", str(issue)]
    if cur and cur != new:
        args += ["--remove-label", f"factory:phase:{cur}"]
    args += ["--add-label", f"factory:phase:{new}"]
    subprocess.run(["gh", *args], capture_output=True)


def add_label(issue, label):
    subprocess.run(
        ["gh", "issue", "edit", str(issue), "--add-label", label],
        capture_output=True,
    )


def find_open_pr(issue):
    data = gh_json([
        "pr", "list", "--search", f"linked:{issue}", "--state", "open",
        "--json", "number,headRefName,reviewDecision",
    ])
    return data[0] if data else None


def find_merged_pr(issue):
    data = gh_json([
        "pr", "list", "--search", f"linked:{issue}", "--state", "merged",
        "--json", "number",
    ])
    return data[0] if data else None


def factory_branch(issue):
    r = subprocess.run(
        ["git", "branch", "--list", f"factory/{issue}-*"],
        capture_output=True, text=True,
    )
    line = r.stdout.strip().splitlines()[0:1]
    if not line:
        return None
    return line[0].strip().lstrip("* ").strip()


def branch_has_diff(branch):
    """True if <branch> differs from origin/main."""
    r = subprocess.run(
        ["git", "diff", "--quiet", f"origin/main...{branch}"],
        capture_output=True,
    )
    return r.returncode != 0


def emit(phase, issue, agent, outcome, details=None):
    try:
        STATE_DIR.mkdir(parents=True, exist_ok=True)
        rec = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "phase": phase, "issue": issue, "agent": agent, "outcome": outcome,
        }
        if details:
            rec.update(details)
        with open(EVENT_LOG, "a") as f:
            f.write(json.dumps(rec) + "\n")
    except Exception:
        pass


# ---------- per-agent dispatchers ---------------------------------------

INVESTIGATING_TYPES = ("bug", "spike", "investigate")


def post_intake(issue):
    data = gh_json(["issue", "view", str(issue), "--json", "labels"]) or {}
    names = {l["name"] for l in data.get("labels", [])}
    types = [
        n.split(":")[-1] for n in names if n.startswith("factory:type:")
    ]
    if not types:
        add_label(issue, "factory:status:needs-clarification")
        return ("triage", "missing-type")
    if any(t in INVESTIGATING_TYPES for t in types):
        set_phase(issue, "investigating")
        return ("investigating", "ok")
    set_phase(issue, "planning")
    return ("planning", "ok")


INVESTIGATION_CONFIRMED = "<!-- loswf:investigation:confirmed -->"
INVESTIGATION_NOT_REPRO = "<!-- loswf:investigation:not-reproducible -->"
INVESTIGATION_TYPE_MISMATCH_PREFIX = "<!-- loswf:investigation:type-mismatch:"


def post_investigator(issue):
    data = gh_json(["issue", "view", str(issue), "--json", "comments"]) or {}
    comments = data.get("comments", [])
    for c in reversed(comments):
        body = c.get("body") or ""
        if INVESTIGATION_CONFIRMED in body:
            set_phase(issue, "decomposing")
            return ("decomposing", "confirmed")
        if INVESTIGATION_NOT_REPRO in body:
            add_label(issue, "factory:status:not-a-bug")
            add_label(issue, "factory:status:needs-attention")
            return ("investigating", "not-reproducible")
        idx = body.find(INVESTIGATION_TYPE_MISMATCH_PREFIX)
        if idx != -1:
            tail = body[idx + len(INVESTIGATION_TYPE_MISMATCH_PREFIX):]
            end = tail.find("-->")
            new_type = tail[:end].strip() if end != -1 else ""
            if new_type:
                subprocess.run(
                    [
                        "gh", "issue", "edit", str(issue),
                        "--remove-label", "factory:type:bug",
                        "--remove-label", "factory:type:spike",
                        "--remove-label", "factory:type:investigate",
                        "--add-label", f"factory:type:{new_type}",
                    ],
                    capture_output=True,
                )
                set_phase(issue, "planning")
                return ("planning", f"retyped-{new_type}")
    add_label(issue, "factory:status:needs-attention")
    return ("investigating", "no-verdict")


PLAN_MARKER = "<!-- loswf:plan -->"
REVISION_BUDGET = int(os.environ.get("LOSWF_REVISION_BUDGET", "3"))
BUDGET_MARKER = "<!-- loswf:budget-exhausted -->"


def post_final_feedback(issue_or_pr_args, last_feedback, kind):
    body = (
        f"{BUDGET_MARKER}\n"
        f"## Revision budget exhausted ({REVISION_BUDGET} rounds) — {kind}\n\n"
        f"Last reviewer feedback:\n\n{last_feedback}\n\n"
        f"Needs human review."
    )
    subprocess.run(
        ["gh", *issue_or_pr_args, "--body", body], capture_output=True,
    )


def post_planner(issue):
    # Authoritative transport: an issue comment whose body begins with the
    # loswf:plan marker. Local specs/drafts/*.md is a convenience copy that
    # lives inside the planner's worktree and won't survive to the next agent.
    data = gh_json(["issue", "view", str(issue), "--json", "comments"]) or {}
    for c in data.get("comments", []):
        if PLAN_MARKER in (c.get("body") or ""):
            set_phase(issue, "plan-review")
            return ("plan-review", "plan-comment")
    add_label(issue, "factory:status:needs-attention")
    return ("planning", "no-plan-produced")


def post_plan_reviewer(issue):
    data = gh_json(["issue", "view", str(issue), "--json", "comments"]) or {}
    comments = data.get("comments", [])
    if not comments:
        add_label(issue, "factory:status:needs-attention")
        return ("plan-review", "no-verdict-comment")
    last = comments[-1]["body"].lower()[:200]
    if "approved" in last:
        size = get_size(issue)
        if size in ("l", "xl"):
            set_phase(issue, "decomposing")
            return ("decomposing", f"approved-size-{size}")
        set_phase(issue, "building")
        return ("building", "approved")
    if "rejected" in last or "revise" in last or "changes requested" in last:
        rejections = sum(
            1 for c in comments
            if any(k in (c.get("body") or "").lower()[:200]
                   for k in ("rejected", "revise", "changes requested"))
        )
        if rejections >= REVISION_BUDGET:
            post_final_feedback(
                ["issue", "comment", str(issue)],
                comments[-1]["body"], "plan-review",
            )
            add_label(issue, "factory:status:needs-attention")
            return ("plan-review", f"budget-exhausted-{rejections}")
        set_phase(issue, "planning")
        return ("planning", f"rejected-{rejections}")
    add_label(issue, "factory:status:needs-attention")
    return ("plan-review", "ambiguous-verdict")


def post_decomposer(issue):
    children = gh_json([
        "issue", "list", "--search",
        f'label:"factory:parent:{issue}"',
        "--state", "all", "--json", "number,labels",
    ]) or []
    # TODO: spike short-circuit — a confirmed spike with findings but no
    # code change legitimately produces 0 children; add a path that routes
    # these to done/rollup instead of needs-attention.
    if len(children) < 2:
        add_label(issue, "factory:status:needs-attention")
        return ("decomposing", f"only-{len(children)}-subtasks")
    set_phase(issue, "awaiting-children")
    return ("awaiting-children", f"spawned-{len(children)}")


def check_parent_rollup(parent):
    """Advance parent from awaiting-children → rollup if all children are done."""
    children = gh_json([
        "issue", "list", "--search",
        f'label:"factory:parent:{parent}"',
        "--state", "all", "--json", "number,labels",
    ]) or []
    if not children:
        return False
    for c in children:
        phases = [
            l["name"].split(":")[-1] for l in c.get("labels", [])
            if l["name"].startswith("factory:phase:")
        ]
        if not phases or phases[0] != "done":
            return False
    set_phase(parent, "rollup")
    return True


def post_builder(issue):
    branch = factory_branch(issue)
    if not branch:
        set_phase(issue, "rollup")
        return ("rollup", "no-branch-no-op")
    if not branch_has_diff(branch):
        set_phase(issue, "rollup")
        return ("rollup", "no-diff-no-op")
    pr = find_open_pr(issue)
    if pr:
        set_phase(issue, "review")
        return ("review", f"pr-{pr['number']}")
    add_label(issue, "factory:status:needs-attention")
    return ("building", "diff-but-no-pr")


def post_reviewer(issue):
    pr = find_open_pr(issue)
    if not pr:
        add_label(issue, "factory:status:needs-attention")
        return ("review", "no-pr")
    # Route off the latest sentinel PR comment — `gh pr review --approve`
    # fails with self-approval when builder+reviewer share one identity.
    data = gh_json([
        "pr", "view", str(pr["number"]), "--json", "comments",
    ]) or {}
    verdict = None
    for c in reversed(data.get("comments", [])):
        body = (c.get("body") or "").lstrip().lower()
        for kw in ("approved:", "rejected:", "block:"):
            if body.startswith(kw):
                verdict = kw[:-1]
                break
        if verdict:
            break
    if verdict == "approved":
        set_phase(issue, "ship")
        return ("ship", f"approved-pr-{pr['number']}")
    if verdict == "rejected":
        rejections = sum(
            1 for c in data.get("comments", [])
            if (c.get("body") or "").lstrip().lower().startswith("rejected:")
        )
        if rejections >= REVISION_BUDGET:
            post_final_feedback(
                ["pr", "comment", str(pr["number"])],
                next(
                    (c["body"] for c in reversed(data.get("comments", []))
                     if (c.get("body") or "").lstrip().lower().startswith("rejected:")),
                    "",
                ),
                "review",
            )
            add_label(issue, "factory:status:needs-attention")
            return ("review", f"budget-exhausted-{rejections}")
        set_phase(issue, "building")
        return ("building", f"rejected-{rejections}")
    if verdict == "block":
        add_label(issue, "factory:status:needs-attention")
        return ("review", "blocked")
    return ("review", "pending-no-verdict")


def post_ship(issue):
    pr = find_merged_pr(issue)
    if pr:
        set_phase(issue, "rollup")
        return ("rollup", f"merged-pr-{pr['number']}")
    add_label(issue, "factory:status:needs-attention")
    return ("ship", "merge-failed")


def post_documenter(issue):
    set_phase(issue, "done")
    subprocess.run(["gh", "issue", "close", str(issue)], capture_output=True)
    # If this issue is a decomposition child, check whether the parent can
    # advance awaiting-children → rollup.
    parent = get_parent(issue)
    if parent:
        check_parent_rollup(parent)
    return ("done", "closed")


# Keys stay `loswf-*` (not `loswf:*`) after the plugin rename in #31.
# `LOSWF_CURRENT_AGENT` is set by `adws/_lib.sh::run_subagent`, which passes
# the embed-install agent name (project-scope) regardless of whether the
# operator also installed the user-scope plugin. ADWs run against the embed
# surface; the plugin is user-scope only. Don't add `loswf:*` aliases here.
DISPATCH = {
    "loswf-intake":        post_intake,
    "loswf-investigator":  post_investigator,
    "loswf-planner":       post_planner,
    "loswf-plan-reviewer": post_plan_reviewer,
    "loswf-decomposer":    post_decomposer,
    "loswf-builder":       post_builder,
    "loswf-reviewer":      post_reviewer,
    "loswf-ship":          post_ship,
    "loswf-documenter":    post_documenter,
    # loswf-architect, loswf-harvester, loswf-curator, loswf-designer,
    # loswf-escalation, loswf-red-team, loswf-scout, loswf-setup — no transition.
    # (Setup configures the host repo itself; it is not a pipeline phase.)
}


DISPATCHED_LEDGER = STATE_DIR / "dispatched.jsonl"


def _normalize_agent(name: str) -> str:
    """Normalize plugin/embed/bare agent names to DISPATCH keys.

    Accepts `loswf:intake`, `loswf-intake`, or bare `intake`; returns
    `loswf-intake`. Returns "" if the role is unknown.
    """
    if not name:
        return ""
    role = name.split(":", 1)[-1] if ":" in name else name
    if role.startswith("loswf-"):
        role = role[len("loswf-"):]
    key = f"loswf-{role}"
    return key if key in DISPATCH else ""


_ISSUE_RE = None
def _parse_issue(description: str) -> int | None:
    global _ISSUE_RE
    import re
    if _ISSUE_RE is None:
        _ISSUE_RE = re.compile(r"#(\d+)")
    if not description:
        return None
    m = _ISSUE_RE.search(description)
    return int(m.group(1)) if m else None


def _already_dispatched(tool_use_id: str) -> bool:
    if not tool_use_id or not DISPATCHED_LEDGER.exists():
        return False
    try:
        for line in DISPATCHED_LEDGER.read_text().splitlines():
            try:
                if json.loads(line).get("id") == tool_use_id:
                    return True
            except json.JSONDecodeError:
                continue
    except OSError:
        return False
    return False


def _mark_dispatched(tool_use_id: str, agent: str, issue: int) -> None:
    if not tool_use_id:
        return
    try:
        DISPATCHED_LEDGER.parent.mkdir(parents=True, exist_ok=True)
        with DISPATCHED_LEDGER.open("a") as f:
            f.write(json.dumps({
                "id": tool_use_id, "agent": agent, "issue": issue,
                "ts": int(time.time()),
            }) + "\n")
    except OSError:
        pass


def _scan_transcript(path: str) -> list[tuple[str, str, int]]:
    """Return completed, undispatched (tool_use_id, agent_key, issue) from
    the transcript. Only Agent tool_uses with a matching tool_result count
    as completed.
    """
    if not path:
        return []
    p = Path(path)
    if not p.exists():
        return []

    tool_uses: dict[str, tuple[str, str]] = {}  # id → (subagent_type, description)
    completed: set[str] = set()
    try:
        for line in p.read_text().splitlines():
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = ev.get("message") or {}
            content = msg.get("content")
            if not isinstance(content, list):
                continue
            for c in content:
                if not isinstance(c, dict):
                    continue
                if c.get("type") == "tool_use" and c.get("name") == "Agent":
                    inp = c.get("input") or {}
                    tool_uses[c.get("id", "")] = (
                        inp.get("subagent_type", ""),
                        inp.get("description", ""),
                    )
                elif c.get("type") == "tool_result":
                    tid = c.get("tool_use_id", "")
                    if tid:
                        completed.add(tid)
    except OSError:
        return []

    out: list[tuple[str, str, int]] = []
    for tid, (subtype, desc) in tool_uses.items():
        if tid not in completed:
            continue
        if _already_dispatched(tid):
            continue
        agent_key = _normalize_agent(subtype)
        issue = _parse_issue(desc)
        if not agent_key or not issue:
            continue
        out.append((tid, agent_key, issue))
    return out


def _dispatch(agent: str, issue: int, source: str,
              tool_use_id: str = "") -> None:
    before = get_phase(issue)
    try:
        after, outcome = DISPATCH[agent](issue)
    except Exception as e:
        emit("post-agent", issue, agent, "error",
             {"error": str(e)[:300], "source": source})
        return
    emit("post-agent", issue, agent, "transition",
         {"before": before, "after": after, "result": outcome,
          "source": source})
    _mark_dispatched(tool_use_id, agent, issue)


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        payload = {}

    # Path 1: ADW — env-var identification.
    agent = os.environ.get("LOSWF_CURRENT_AGENT", "")
    issue_str = os.environ.get("LOSWF_CURRENT_ISSUE", "")
    if agent and agent in DISPATCH and issue_str.isdigit():
        _dispatch(agent, int(issue_str), source="env")
        return 0

    # Path 2: interactive — transcript-based discovery. Fires on both Stop
    # (end-of-turn, possibly multiple completed subagents) and SubagentStop
    # (per-subagent). Dedupe via dispatched.jsonl ledger.
    transcript = ""
    if isinstance(payload, dict):
        transcript = payload.get("transcript_path") or ""
    for tool_use_id, agent_key, issue in _scan_transcript(transcript):
        _dispatch(agent_key, issue, source="transcript",
                  tool_use_id=tool_use_id)
    return 0


if __name__ == "__main__":
    sys.exit(main())
