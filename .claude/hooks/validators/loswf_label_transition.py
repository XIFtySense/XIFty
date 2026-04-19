#!/usr/bin/env python3
"""Label-transition validator — guards `gh issue edit --add-label / --remove-label`
calls so agents cannot make illegal phase transitions.

Reads the legal transition table from the loswf-factory-state skill. If a
Bash command attempts a phase transition not in the table, the hook denies it.

Hook wiring (in .claude/settings.json under PreToolUse for Bash):
  {"type": "command", "command": ".claude/hooks/validators/loswf_label_transition.py"}
"""
import json
import os
import re
import sys

# Legal phase transitions (mirrors .claude/skills/loswf-factory-state/SKILL.md).
LEGAL = {
    None: {"triage"},
    "triage": {"planning", "investigating", "needs-clarification"},
    "planning": {"plan-review", "triage"},
    "plan-review": {"building", "planning", "decomposing", "triage"},
    "investigating": {"decomposing", "needs-attention"},
    "decomposing": {"awaiting-children", "needs-attention", "planning"},
    "awaiting-children": {"rollup", "needs-attention"},
    "building": {"review", "rollup", "needs-attention", "planning"},
    "review": {"ship", "building", "needs-attention", "planning"},
    "ship": {"rollup"},
    "rollup": {"done", "review"},
    "done": set(),
}

PHASE_RE = re.compile(r"factory:phase:([a-z-]+)")
ADD_RE = re.compile(r"--add-label\s+[\"']?(factory:phase:[a-z-]+)[\"']?")
REMOVE_RE = re.compile(r"--remove-label\s+[\"']?(factory:phase:[a-z-]+)[\"']?")


def main() -> int:
    if os.environ.get("LOSWF_SELF_HOSTED") == "1":
        return 0

    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    if payload.get("tool_name") != "Bash":
        return 0

    cmd = payload.get("tool_input", {}).get("command", "")
    if "gh issue edit" not in cmd:
        return 0

    # Validate each subcommand independently. A chained command like
    # `gh issue edit X --add-label A && gh issue edit X --remove-label A --add-label B`
    # must not conflate labels across the `&&` boundary.
    for segment in re.split(r"&&|\|\||;|\n", cmd):
        if "gh issue edit" not in segment:
            continue
        add_match = ADD_RE.search(segment)
        if not add_match:
            continue  # no phase add in this segment → nothing to validate
        new_phase = add_match.group(1).split(":")[-1]
        remove_match = REMOVE_RE.search(segment)
        current_phase = remove_match.group(1).split(":")[-1] if remove_match else None

        legal_next = LEGAL.get(current_phase, set())
        if new_phase not in legal_next:
            msg = (
                f"label_transition: illegal phase change "
                f"{current_phase or '(none)'} → {new_phase}. "
                f"Legal: {sorted(legal_next) or '(terminal)'}"
            )
            print(msg, file=sys.stderr)
            return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
