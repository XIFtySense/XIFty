#!/usr/bin/env python3
"""watch-factory.py — live tail of Claude Code session transcripts with color
per subagent role. Run this in a side terminal while /loswf:loop (or any
factory sweep) runs in another.

Usage:
  ./bin/watch-factory.py              # watch current repo's project dir
  ./bin/watch-factory.py --project <slug>
  ./bin/watch-factory.py --all        # watch every project

What you see (per new assistant turn):
  ┌─ [role · session-prefix] (thinking 3.2s)
  │ …thinking text…
  │ …assistant text…
  └─ tool: Bash("rg …") | Read("…")

Sidechain sessions (subagents) get a color drawn from the agent's
`.claude/agents/*.md` color frontmatter. Main session is dim grey.
"""
from __future__ import annotations
import argparse, json, re, sys, time
from pathlib import Path

HOME = Path.home()
PROJECTS = HOME / ".claude" / "projects"

# ANSI
R = "\033[0m"
DIM = "\033[2m"
BOLD = "\033[1m"
COLORS = {
    "cyan": "\033[36m", "blue": "\033[34m", "green": "\033[32m",
    "yellow": "\033[33m", "orange": "\033[33m", "red": "\033[31m",
    "purple": "\033[35m", "magenta": "\033[35m", "pink": "\033[35m",
    "white": "\033[37m", "grey": "\033[90m", "gray": "\033[90m",
}

def agent_colors(repo_root: Path) -> dict[str, str]:
    """Map agent role name → ANSI color, by scanning `.claude/agents/*.md`."""
    out: dict[str, str] = {}
    agents_dir = repo_root / ".claude" / "agents"
    if not agents_dir.is_dir():
        return out
    for f in agents_dir.glob("*.md"):
        try:
            text = f.read_text(errors="ignore")
        except OSError:
            continue
        fm = re.match(r"^---\s*\n(.*?)\n---", text, re.DOTALL)
        if not fm:
            continue
        body = fm.group(1)
        m_name = re.search(r"^name:\s*(\S+)", body, re.MULTILINE)
        m_color = re.search(r"^color:\s*(\S+)", body, re.MULTILINE)
        if not m_name:
            continue
        role = m_name.group(1).strip().removeprefix("loswf-").removeprefix("loswf:")
        color = (m_color.group(1).strip().lower() if m_color else "white")
        out[role] = COLORS.get(color, COLORS["white"])
    return out

def project_slug(cwd: Path) -> str:
    """Claude Code encodes cwd as `-Users-k-…` (leading slash → leading `-`)."""
    return "-" + str(cwd.resolve()).lstrip("/").replace("/", "-")

def detect_role(msg_content) -> str | None:
    """Best-effort role detection: look for role markers in the system or
    initial user prompt. Subagents spawned via Task get an 'agent' label
    on their first user turn like 'You are the loswf-builder …' — we match
    on 'loswf-<name>' tokens."""
    if isinstance(msg_content, str):
        m = re.search(r"loswf[-:](\w[\w-]*)", msg_content)
        return m.group(1) if m else None
    if isinstance(msg_content, list):
        for b in msg_content:
            if isinstance(b, dict):
                txt = b.get("text") or b.get("thinking") or ""
                m = re.search(r"loswf[-:](\w[\w-]*)", txt[:2000])
                if m:
                    return m.group(1)
    return None

class Follower:
    def __init__(self, path: Path):
        self.path = path
        self.offset = 0
        self.role: str | None = None
        self.session: str = path.stem[:8]

    def read_new(self) -> list[dict]:
        try:
            size = self.path.stat().st_size
        except FileNotFoundError:
            return []
        if size <= self.offset:
            return []
        out: list[dict] = []
        with self.path.open("rb") as f:
            f.seek(self.offset)
            chunk = f.read(size - self.offset).decode("utf-8", "replace")
            self.offset = f.tell()
        for line in chunk.splitlines():
            if not line.strip():
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        return out

def format_block(b: dict, color: str) -> str | None:
    t = b.get("type")
    if t == "thinking":
        txt = b.get("thinking", "").strip()
        if not txt: return None
        return f"{DIM}{color}  💭 {txt}{R}"
    if t == "text":
        txt = b.get("text", "").strip()
        if not txt: return None
        return f"{color}  {txt}{R}"
    if t == "tool_use":
        name = b.get("name", "?")
        inp = b.get("input") or {}
        hint = ""
        for k in ("command", "file_path", "pattern", "description", "prompt"):
            if k in inp and isinstance(inp[k], str):
                hint = inp[k][:90]
                break
        return f"{DIM}{color}  ▶ {name}({hint}){R}"
    return None

def banner(entry: dict, follower: Follower, agent_map: dict[str, str]) -> tuple[str, str]:
    sidechain = entry.get("isSidechain", False)
    # Try to sharpen role from content if not known yet.
    msg = entry.get("message") or {}
    role = follower.role
    if role is None:
        detected = detect_role(msg.get("content"))
        if detected:
            follower.role = role = detected
    if role and role in agent_map:
        color = agent_map[role]
        label = role
    elif sidechain:
        color = COLORS["cyan"]
        label = "subagent"
    else:
        color = COLORS["grey"]
        label = "main"
    return color, f"{label}·{follower.session}"

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--project", help="project slug (default: derive from cwd)")
    ap.add_argument("--all", action="store_true", help="watch every project dir")
    ap.add_argument("--repo", default=".", help="repo root for color map")
    ap.add_argument("--poll", type=float, default=0.5)
    args = ap.parse_args()

    repo_root = Path(args.repo).resolve()
    agent_map = agent_colors(repo_root)

    if args.all:
        project_dirs = [p for p in PROJECTS.iterdir() if p.is_dir()]
    elif args.project:
        project_dirs = [PROJECTS / args.project]
    else:
        project_dirs = [PROJECTS / project_slug(repo_root)]

    for pd in project_dirs:
        if not pd.exists():
            print(f"(no project dir: {pd})", file=sys.stderr)

    print(f"{BOLD}watching:{R} {' '.join(str(p.name) for p in project_dirs)}")
    print(f"{DIM}(colors from {repo_root}/.claude/agents/*.md){R}\n")

    followers: dict[Path, Follower] = {}
    # Start at EOF for existing files so we only see new activity.
    for pd in project_dirs:
        for f in pd.glob("*.jsonl"):
            fol = Follower(f)
            try:
                fol.offset = f.stat().st_size
            except OSError:
                pass
            followers[f] = fol

    while True:
        for pd in project_dirs:
            if not pd.exists():
                continue
            for f in pd.glob("*.jsonl"):
                if f not in followers:
                    followers[f] = Follower(f)
                fol = followers[f]
                for entry in fol.read_new():
                    if entry.get("type") != "assistant":
                        # Still use user turns to learn the role early.
                        if entry.get("type") == "user" and fol.role is None:
                            msg = entry.get("message") or {}
                            detected = detect_role(msg.get("content"))
                            if detected:
                                fol.role = detected
                        continue
                    color, label = banner(entry, fol, agent_map)
                    msg = entry.get("message") or {}
                    content = msg.get("content") or []
                    if not isinstance(content, list):
                        continue
                    lines = [f"{color}┌─ [{label}]{R}"]
                    for b in content:
                        if not isinstance(b, dict):
                            continue
                        line = format_block(b, color)
                        if line:
                            lines.append(line)
                    if len(lines) > 1:
                        print("\n".join(lines))
                        print(f"{color}└─{R}")
        time.sleep(args.poll)

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
