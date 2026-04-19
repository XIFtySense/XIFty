#!/usr/bin/env python3
"""loswf2 PostToolUse hook — append every tool invocation to the event journal."""
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

STATE_DIR = Path(os.environ.get("LOSWF_STATE_DIR", ".loswf/state"))
EVENT_LOG = STATE_DIR / "events.jsonl"


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    STATE_DIR.mkdir(parents=True, exist_ok=True)

    event = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "tool": payload.get("tool_name"),
        "session_id": payload.get("session_id"),
        "cwd": payload.get("cwd"),
        "ok": payload.get("tool_response", {}).get("error") is None,
    }

    with EVENT_LOG.open("a") as fh:
        fh.write(json.dumps(event) + "\n")

    return 0


if __name__ == "__main__":
    sys.exit(main())
