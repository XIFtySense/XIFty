#!/usr/bin/env python3
"""Commit-message validator — enforce `<type>: <summary> (#<num>)` format.

The builder agent's contract (see .claude/agents/loswf-builder.md) is to commit
with messages like `feat: add foo bar (#42)`. This hook enforces that.

Allowed types: feat, fix, chore, docs, refactor, test, perf, build, ci, style.

Recognized commit forms:
  git commit -m "<msg>"
  git commit --message="<msg>"
  git commit -m "$(cat <<'EOF' …)"  ← HEREDOC; we extract the first body line

Bypass for ad-hoc human commits: LOSWF_SKIP_COMMIT_FORMAT=1.

Exit codes:
  0 — allow / not a commit / bypassed
  2 — deny
"""
from __future__ import annotations

import json
import os
import re
import sys

ALLOWED_TYPES = "feat|fix|chore|docs|refactor|test|perf|build|ci|style|revert"
FORMAT = re.compile(rf"^({ALLOWED_TYPES}):\s+\S.*\(#\d+\)\s*$")

# `-m "msg"` or `-m 'msg'` or `--message=msg` or `--message="msg"`
_M_FLAG = re.compile(
    r"""(?:-m|--message)(?:\s+|=)
        (?:"([^"]*)"|'([^']*)'|(\S+))
    """,
    re.VERBOSE,
)
# HEREDOC body: capture first non-blank line after the opening tag.
_HEREDOC = re.compile(
    r"<<-?\s*['\"]?(\w+)['\"]?\s*\n(.*?)\n\s*\1\b",
    re.DOTALL,
)


def _deny(msg: str) -> int:
    print(f"loswf_commit_message: BLOCKED — {msg}", file=sys.stderr)
    return 2


def _extract_messages(cmd: str) -> list[str]:
    msgs: list[str] = []
    for d, s, b in _M_FLAG.findall(cmd):
        msgs.append(d or s or b)
    for _, body in _HEREDOC.findall(cmd):
        for line in body.splitlines():
            line = line.strip()
            if line:
                msgs.append(line)
                break
    return msgs


def main() -> int:
    if os.environ.get("LOSWF_SELF_HOSTED") == "1":
        return 0
    if os.environ.get("LOSWF_SKIP_COMMIT_FORMAT") == "1":
        return 0

    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    if event.get("tool_name") != "Bash":
        return 0

    cmd = event.get("tool_input", {}).get("command", "")
    if "git commit" not in cmd:
        return 0
    # Skip --amend (rare, audited separately) and merges.
    if "--amend" in cmd or "--no-edit" in cmd:
        return 0

    messages = _extract_messages(cmd)
    if not messages:
        return 0  # interactive editor commit; can't validate, allow

    for msg in messages:
        first = msg.splitlines()[0].strip()
        if not FORMAT.match(first):
            return _deny(
                f"commit subject must match `<type>: <summary> (#<num>)`. "
                f"Got: {first!r}. Allowed types: {ALLOWED_TYPES}. "
                f"Override: LOSWF_SKIP_COMMIT_FORMAT=1"
            )

    return 0


if __name__ == "__main__":
    sys.exit(main())
