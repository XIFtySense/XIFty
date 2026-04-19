#!/usr/bin/env python3
"""PreToolUse(Task) + SubagentStop — colorized open/close markers to stderr.

Purpose: factory monitorability. During interactive sweeps Claude Code
dispatches many Task subagents per turn; this hook prints a colorized
`▶ [role] <description>` on dispatch and a paired `◀ [role] done (N.Ns)`
on SubagentStop, using the `color:` from each agent's frontmatter.

Hook-kind is decided at runtime by inspecting stdin payload shape:
  - `tool_name == "Task"` with `tool_input` → open-marker (PreToolUse).
  - no `tool_name`, has `transcript_path` → close-marker (SubagentStop).

The hook must never block tool calls — all exceptions are swallowed and
exit 0 is always returned. Stderr only; stdout is reserved for the tool
pipeline.

NOTE: hooks are not mirrored into `plugin/` — the parity check at
`bin/_plugin_parity_check.sh` only checks agents, commands, and skills.
"""
from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

# Make the sibling helper importable regardless of cwd.
_HERE = Path(__file__).resolve().parent
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

from _agent_color import resolve_color  # noqa: E402

STATE_DIR = Path(os.environ.get("LOSWF_STATE_DIR", ".loswf/state"))
TIMING_LEDGER = STATE_DIR / "subagent_timing.jsonl"
CLOSED_LEDGER = STATE_DIR / "subagent_closed.jsonl"


def _ansi(code: int, text: str) -> str:
    return f"\033[{code}m{text}\033[0m"


def _record_start(tool_use_id: str) -> None:
    if not tool_use_id:
        return
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    with TIMING_LEDGER.open("a") as f:
        f.write(json.dumps({"id": tool_use_id, "ts": time.time()}) + "\n")


def _lookup_start(tool_use_id: str) -> float | None:
    if not tool_use_id or not TIMING_LEDGER.exists():
        return None
    try:
        for line in TIMING_LEDGER.read_text().splitlines():
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            if rec.get("id") == tool_use_id:
                return float(rec.get("ts") or 0) or None
    except OSError:
        return None
    return None


def _closed_ids() -> set[str]:
    if not CLOSED_LEDGER.exists():
        return set()
    out: set[str] = set()
    try:
        for line in CLOSED_LEDGER.read_text().splitlines():
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            tid = rec.get("id")
            if tid:
                out.add(tid)
    except OSError:
        return set()
    return out


def _mark_closed(tool_use_id: str) -> None:
    if not tool_use_id:
        return
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    with CLOSED_LEDGER.open("a") as f:
        f.write(json.dumps({"id": tool_use_id, "ts": time.time()}) + "\n")


def _scan_latest_unclosed(transcript_path: str) -> tuple[str, str, str] | None:
    """Return (tool_use_id, subagent_type, description) for the most recent
    completed Task subagent not yet in the closed-ledger. Returns None if
    none found.
    """
    if not transcript_path:
        return None
    p = Path(transcript_path)
    if not p.exists():
        return None

    closed = _closed_ids()
    tool_uses: list[tuple[str, str, str]] = []  # [(id, subtype, desc), ...]
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
                # Claude Code transcripts call the subagent dispatch tool
                # "Task" in some channels and "Agent" in others. Accept both.
                if c.get("type") == "tool_use" and c.get("name") in ("Task", "Agent"):
                    inp = c.get("input") or {}
                    tool_uses.append((
                        c.get("id", ""),
                        inp.get("subagent_type", ""),
                        inp.get("description", ""),
                    ))
                elif c.get("type") == "tool_result":
                    tid = c.get("tool_use_id", "")
                    if tid:
                        completed.add(tid)
    except OSError:
        return None

    # Most recent first.
    for tid, subtype, desc in reversed(tool_uses):
        if not tid or tid in closed:
            continue
        if tid not in completed:
            continue
        return (tid, subtype, desc)
    return None


def _open(payload: dict) -> None:
    tool_input = payload.get("tool_input") or {}
    subagent_type = tool_input.get("subagent_type", "")
    description = tool_input.get("description", "") or ""
    tool_use_id = payload.get("tool_use_id", "") or ""
    role, code = resolve_color(subagent_type)
    role_disp = role or (subagent_type or "agent")
    sys.stderr.write(
        f"{_ansi(code, f'▶ [{role_disp}]')} {description}\n"
    )
    sys.stderr.flush()
    _record_start(tool_use_id)


def _close(payload: dict) -> None:
    transcript = payload.get("transcript_path") or ""
    hit = _scan_latest_unclosed(transcript)
    if not hit:
        return
    tid, subtype, _desc = hit
    role, code = resolve_color(subtype)
    role_disp = role or (subtype or "agent")
    started = _lookup_start(tid)
    elapsed = (time.time() - started) if started else 0.0
    sys.stderr.write(
        f"{_ansi(code, f'◀ [{role_disp}]')} done ({elapsed:.1f}s)\n"
    )
    sys.stderr.flush()
    _mark_closed(tid)


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0
    if not isinstance(payload, dict):
        return 0
    try:
        if payload.get("tool_name") == "Task" and payload.get("tool_input"):
            _open(payload)
        elif payload.get("transcript_path") and not payload.get("tool_name"):
            _close(payload)
    except Exception:
        # Monitorability must never block factory flow.
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
