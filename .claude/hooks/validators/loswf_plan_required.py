#!/usr/bin/env python3
"""Plan-required validator — refuse code edits on a factory branch without an
approved plan file.

Reasoning: the builder agent must work from `specs/drafts/<num>-*.md`. If an
agent tries to write code on `factory/<num>-<slug>` without that plan, it's
either jumping the planning phase or ran in the wrong worktree.

Allowed without a plan:
  - Edits under specs/, docs/, README*, .github/ (planning artifacts + meta)
  - Anything when not on a factory/<num>-* branch (interactive use)
  - Bypass via LOSWF_SKIP_PLAN_GATE=1 (chores legitimately skip planning)

Exit codes:
  0 — allow
  2 — deny
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

EDIT_TOOLS = {"Write", "Edit", "MultiEdit"}
BRANCH_RE = re.compile(r"^factory/(\d+)-")
EXEMPT_PREFIXES = ("specs/", "docs/", ".github/", "README", ".loswf/state/")


def _current_branch() -> str:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True, text=True, timeout=5,
        )
        return out.stdout.strip()
    except (subprocess.SubprocessError, FileNotFoundError):
        return ""


def _repo_root() -> Path:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, timeout=5,
        )
        return Path(out.stdout.strip() or ".")
    except (subprocess.SubprocessError, FileNotFoundError):
        return Path(".")


def _deny(msg: str) -> int:
    print(f"loswf_plan_required: BLOCKED — {msg}", file=sys.stderr)
    return 2


def main() -> int:
    if os.environ.get("LOSWF_SELF_HOSTED") == "1":
        return 0
    if os.environ.get("LOSWF_SKIP_PLAN_GATE") == "1":
        return 0

    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    if event.get("tool_name") not in EDIT_TOOLS:
        return 0

    branch = _current_branch()
    m = BRANCH_RE.match(branch)
    if not m:
        return 0  # not a factory branch — out of scope
    issue_num = m.group(1)

    path = (event.get("tool_input") or {}).get("file_path", "")
    if not path:
        return 0

    root = _repo_root()
    try:
        rel = str(Path(path).resolve().relative_to(root.resolve()))
    except ValueError:
        rel = path  # outside repo; let other guards handle it

    if rel.startswith(EXEMPT_PREFIXES):
        return 0

    plans = list(root.glob(f"specs/drafts/{issue_num}-*.md"))
    if not plans:
        return _deny(
            f"no plan at specs/drafts/{issue_num}-*.md for branch '{branch}'. "
            f"Run /loswf-plan {issue_num} first, or set LOSWF_SKIP_PLAN_GATE=1 "
            f"for chores that legitimately skip planning."
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
