#!/usr/bin/env python3
"""loswf2 PreToolUse hook — block destructive ops the settings allowlist can't see.

Fires on every tool call. Reads Claude's JSON event on stdin:
  {"tool_name": "Bash"|"Read"|"Write"|"Edit"|"MultiEdit", "tool_input": {...}}

Exit codes:
  0 — allow (silent)
  2 — deny; stderr surfaces to the agent

The settings.json permissions.deny list is the primary static gate; this hook
catches shell-expansion and chained-command cases that static matching misses,
plus enforces secret-path access for non-Bash tools.
"""
from __future__ import annotations

import json
import re
import sys


# rm targeting /, $HOME, ~, or . with -r/-f/--recursive/--force in any order.
_DANGEROUS_RM = re.compile(
    r"""\brm\s+
        (?:-[a-zA-Z]*[rf][a-zA-Z]*\s+|--recursive\s+|--force\s+)+
        .*?
        (?:/\s*$|/\s+|\s/\*|~|\$HOME|\.\s*$|\.\s+)
    """,
    re.VERBOSE,
)

# Destructive git ops. --force-with-lease is allowed (safer cousin).
_DANGEROUS_GIT = re.compile(
    r"""\bgit\s+
        (?:push\s+.*--force(?!-with-lease)
          |push\s+.*\s-f(?:\s|$)
          |push\s+origin\s+(?:main|master)\b
          |reset\s+--hard\s+(?:origin/)?(?:main|master)\b
          |clean\s+-[a-zA-Z]*f[a-zA-Z]*d
          |branch\s+-D\s+(?:main|master)\b)
    """,
    re.VERBOSE,
)

# gh CLI escape hatches we never want an agent taking.
_DANGEROUS_GH = re.compile(
    r"\bgh\s+pr\s+merge\b.*--admin\b"
)

# --no-verify on any command (commit, push, etc.) — bypasses hooks.
_NO_VERIFY = re.compile(r"--no-verify\b")

# Secret files — refuse direct access regardless of tool.
_SECRET_PATHS = re.compile(
    r"(^|/)(\.env(\.[a-z0-9_]+)?|\.envrc|credentials\.json|id_rsa|id_ed25519)(\s|$|:)"
)


def _deny(msg: str) -> int:
    print(f"loswf_pre_tool_use: BLOCKED — {msg}", file=sys.stderr)
    return 2


def _check_bash(cmd: str) -> int:
    if _DANGEROUS_RM.search(cmd):
        return _deny(f"dangerous rm: {cmd!r}")
    if _DANGEROUS_GIT.search(cmd):
        return _deny(f"destructive git op: {cmd!r}")
    if _DANGEROUS_GH.search(cmd):
        return _deny(f"gh admin merge bypass: {cmd!r}")
    if _NO_VERIFY.search(cmd):
        return _deny(f"--no-verify bypasses hooks: {cmd!r}")
    if _SECRET_PATHS.search(cmd):
        return _deny(f"secret-file access in shell: {cmd!r}")
    return 0


def _check_path(path: str) -> int:
    if _SECRET_PATHS.search(path):
        return _deny(f"secret-file access: {path!r}")
    return 0


def main() -> int:
    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    tool = event.get("tool_name") or event.get("tool") or ""
    tool_input = event.get("tool_input") or {}

    if tool == "Bash":
        return _check_bash(tool_input.get("command", ""))

    if tool in ("Read", "Write", "Edit", "MultiEdit"):
        path = tool_input.get("file_path") or tool_input.get("path") or ""
        return _check_path(path)

    return 0


if __name__ == "__main__":
    sys.exit(main())
