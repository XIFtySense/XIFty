#!/usr/bin/env python3
"""Branch-guard validator — refuse Write/Edit/MultiEdit when HEAD is main/master.

Forces builders onto a `factory/<num>-*` branch before they can modify code.
Also blocks `git commit` issued while on main/master.

Bypass for legitimate one-off ops: set LOSWF_ALLOW_MAIN_EDITS=1 in the env.

Exit codes:
  0 — allow
  2 — deny
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

PROTECTED = {"main", "master"}
EDIT_TOOLS = {"Write", "Edit", "MultiEdit"}


def _current_branch() -> str:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True, text=True, timeout=5,
        )
        return out.stdout.strip()
    except (subprocess.SubprocessError, FileNotFoundError):
        return ""


def _repo_root() -> Path | None:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, timeout=5,
        )
        top = out.stdout.strip()
        return Path(top).resolve() if top else None
    except (subprocess.SubprocessError, FileNotFoundError):
        return None


def _target_inside_repo(tool_input: dict, root: Path) -> bool:
    # Edit tools expose the target as `file_path`. If absent (unexpected),
    # assume inside-repo to stay conservative.
    raw = tool_input.get("file_path")
    if not raw:
        return True
    try:
        target = Path(raw).expanduser().resolve()
    except (OSError, RuntimeError):
        return True
    try:
        target.relative_to(root)
        return True
    except ValueError:
        return False


def _deny(msg: str) -> int:
    print(f"loswf_branch_guard: BLOCKED — {msg}", file=sys.stderr)
    return 2


def main() -> int:
    if os.environ.get("LOSWF_SELF_HOSTED") == "1":
        return 0
    if os.environ.get("LOSWF_ALLOW_MAIN_EDITS") == "1":
        return 0

    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    tool = event.get("tool_name", "")
    tool_input = event.get("tool_input") or {}

    is_edit = tool in EDIT_TOOLS
    is_commit = tool == "Bash" and "git commit" in tool_input.get("command", "")

    if not (is_edit or is_commit):
        return 0

    branch = _current_branch()
    if branch not in PROTECTED:
        return 0

    # Only guard edits that actually land inside this repo's working tree.
    # Edits to ~/.claude/plugins/cache/, /tmp/, or sibling repos are not the
    # factory's concern.
    if is_edit:
        root = _repo_root()
        if root is None:
            return 0
        if not _target_inside_repo(tool_input, root):
            return 0

    action = "edit" if is_edit else "commit"
    return _deny(
        f"refusing to {action} while on '{branch}'. "
        f"Switch to a factory/<issue>-<slug> branch first. "
        f"Override: LOSWF_ALLOW_MAIN_EDITS=1"
    )


if __name__ == "__main__":
    sys.exit(main())
