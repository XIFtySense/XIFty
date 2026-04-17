#!/usr/bin/env python3

import json
import subprocess
import sys
from pathlib import Path

try:
    import jsonschema
except ImportError as exc:  # pragma: no cover - exercised in workflow/local usage
    raise SystemExit(
        "python package 'jsonschema' is required; install it with "
        "`python3 -m pip install jsonschema`"
    ) from exc


ROOT = Path(__file__).resolve().parent.parent
SCHEMAS = ROOT / "schemas"
FIXTURES = ROOT / "fixtures" / "minimal"


def load_schema(name: str):
    return json.loads((SCHEMAS / name).read_text())


PROBE_SCHEMA = load_schema("xifty-probe-0.1.0.schema.json")
ANALYSIS_SCHEMA = load_schema("xifty-analysis-0.1.0.schema.json")


def run_cli(*args: str):
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "xifty-cli", "--", *args],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def validate(instance, schema, label: str):
    jsonschema.validate(instance=instance, schema=schema)
    print(f"validated {label}")


def main():
    validate(
        run_cli("probe", str(FIXTURES / "happy.jpg")),
        PROBE_SCHEMA,
        "probe happy.jpg",
    )
    validate(
        run_cli("probe", str(FIXTURES / "happy.mp4")),
        PROBE_SCHEMA,
        "probe happy.mp4",
    )
    validate(
        run_cli("extract", str(FIXTURES / "happy.jpg")),
        ANALYSIS_SCHEMA,
        "extract happy.jpg full envelope",
    )
    validate(
        run_cli("extract", str(FIXTURES / "gps.jpg"), "--view", "normalized"),
        ANALYSIS_SCHEMA,
        "extract gps.jpg normalized envelope",
    )
    validate(
        run_cli("extract", str(FIXTURES / "malformed.mp4"), "--view", "report"),
        ANALYSIS_SCHEMA,
        "extract malformed.mp4 report envelope",
    )


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.stderr.write(error.stderr)
        raise
