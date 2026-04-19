#!/usr/bin/env python3
"""PostToolUse hook — enqueue edited files for semantic re-indexing.

Fires on Write|Edit|MultiEdit. Appends the rel-path of the modified file to
.loswf/state/index-queue (atomic O_APPEND). adw_sweep.sh boot drains the queue
and runs `bin/loswf-index update` to incrementally re-embed.

Hard requirements:
- NEVER blocks. Returns within milliseconds.
- NEVER raises. All errors silently swallowed — index hiccups must not break
  a Claude turn.
- Skips paths that don't match .loswf/config.yaml `code_search.paths`.
- No-op when code_search block is missing entirely.

Hook wiring (in .claude/settings.json under PostToolUse):
  {"matcher": "Write|Edit|MultiEdit",
   "hooks": [{"type": "command",
              "command": ".claude/hooks/loswf_post_edit_index.py"}]}
"""
import json
import os
import sys
from pathlib import Path

QUEUE = Path(".loswf/state/index-queue")
CONFIG = Path(".loswf/config.yaml")


def get_paths():
    """Return (paths, exclude) from .loswf/config.yaml `code_search:`,
    or ([], []) if missing/unreadable. Tiny hand-rolled YAML — avoids
    a dep on PyYAML for a hot hook."""
    if not CONFIG.exists():
        return [], []
    try:
        text = CONFIG.read_text()
    except Exception:
        return [], []
    in_cs = False
    paths, exclude = [], []
    cur = None
    for raw in text.splitlines():
        line = raw.rstrip()
        if line.startswith("code_search:"):
            in_cs = True
            continue
        if in_cs:
            stripped = line.lstrip(" ")
            indent = len(line) - len(stripped)
            if indent == 0 and stripped:
                # left the block
                break
            if stripped.startswith("paths:"):
                cur = paths
                continue
            if stripped.startswith("exclude:"):
                cur = exclude
                continue
            if cur is not None and stripped.startswith("- "):
                val = stripped[2:].strip().strip('"').strip("'")
                cur.append(val)
            elif stripped and not stripped.startswith("- "):
                cur = None
    return paths, exclude


def is_indexable(rel_path, paths, exclude):
    if not paths:
        return False
    if not any(rel_path.startswith(p) for p in paths):
        return False
    if not rel_path.endswith((".ts", ".py", ".go")):
        return False
    base = rel_path.rsplit("/", 1)[-1]
    for ex in exclude:
        if ex.replace("*", "") in base:
            return False
    return True


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    if payload.get("tool_name") not in ("Write", "Edit", "MultiEdit"):
        return 0

    file_path = payload.get("tool_input", {}).get("file_path", "")
    if not file_path:
        return 0

    cwd = os.getcwd()
    try:
        rel = os.path.relpath(file_path, cwd)
    except Exception:
        return 0
    if rel.startswith(".."):
        return 0

    paths, exclude = get_paths()
    if not is_indexable(rel, paths, exclude):
        return 0

    try:
        QUEUE.parent.mkdir(parents=True, exist_ok=True)
        with open(QUEUE, "a") as f:
            f.write(rel + "\n")
    except Exception:
        pass

    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        sys.exit(0)
