#!/usr/bin/env python3
"""Generate and validate CAPABILITIES.json entries against observed CLI output.

This tool walks every regular file under ``fixtures/minimal/``, invokes
``xifty-cli extract <path> --view raw``, and aggregates — keyed on
``input.detected_format`` — the set of observed metadata namespaces
(``raw.metadata[].namespace`` values). It then compares the observed
``{detected_format -> {namespace}}`` map against the claims in
``CAPABILITIES.json#/containers/*/namespaces``.

Modes
-----
* ``--check`` (default): exit non-zero when an observed ``(format, namespace)``
  pair is marked ``not_yet_supported`` in ``CAPABILITIES.json`` (under-reporting
  drift). Over-claims (declared but never observed) are logged as warnings.
* ``--write``: rewrite ``CAPABILITIES.json`` in place, promoting observed pairs
  currently marked ``not_yet_supported`` to ``supported`` and adding new
  namespaces for any observed pair that is not yet declared. Hand-curated
  ``bounded`` markers, ``supported_tags`` lists, ``normalized_fields``, and
  ``surfaces`` are preserved.

Hand edits remain authoritative for downgrades — only under-reporting is a
hard failure, since that is the specific drift class the issue calls out.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import OrderedDict
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures" / "minimal"
CAPABILITIES = ROOT / "CAPABILITIES.json"


def run_cli(*args: str) -> dict | None:
    """Invoke xifty-cli; return parsed JSON or None on non-zero exit.

    Malformed fixtures are expected to exit non-zero — skip them gracefully
    rather than crashing the tool.
    """
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "xifty-cli", "--", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def observed_map(fixtures_dir: Path) -> dict[str, set[str]]:
    """Return ``{detected_format -> {namespace, ...}}`` from fixture CLI output."""
    agg: dict[str, set[str]] = {}
    for path in sorted(fixtures_dir.iterdir()):
        if not path.is_file():
            continue
        if path.name.startswith(".") or path.name == "README.md":
            continue
        data = run_cli("extract", str(path), "--view", "raw")
        if data is None:
            continue
        detected = data.get("input", {}).get("detected_format")
        if not detected:
            continue
        namespaces = {
            entry.get("namespace")
            for entry in data.get("raw", {}).get("metadata", []) or []
            if entry.get("namespace")
        }
        agg.setdefault(detected, set()).update(namespaces)
    return agg


def diff_against_declared(
    observed: dict[str, set[str]], capabilities: dict
) -> tuple[list[str], list[str]]:
    """Return (errors, warnings) comparing observed to declared namespaces.

    Errors: observed pair declared ``not_yet_supported``.
    Warnings: declared pair never observed (over-claim) or entirely missing
    container/namespace entries that were observed.
    """
    errors: list[str] = []
    warnings: list[str] = []
    containers = capabilities.get("containers", {})

    for fmt, ns_set in sorted(observed.items()):
        declared = containers.get(fmt)
        if declared is None:
            for ns in sorted(ns_set):
                errors.append(
                    f"observed namespace '{ns}' under format '{fmt}' but "
                    f"container '{fmt}' is absent from CAPABILITIES.json"
                )
            continue
        declared_ns = declared.get("namespaces", {})
        for ns in sorted(ns_set):
            status = declared_ns.get(ns)
            if status is None:
                errors.append(
                    f"observed '{fmt}.{ns}' but namespace is undeclared in "
                    f"CAPABILITIES.json#/containers/{fmt}/namespaces"
                )
            elif status == "not_yet_supported":
                errors.append(
                    f"observed '{fmt}.{ns}' but CAPABILITIES.json marks it "
                    f"'not_yet_supported' — promote to 'supported' (run --write)"
                )

    for fmt, declared in sorted(containers.items()):
        for ns, status in sorted(declared.get("namespaces", {}).items()):
            if status in {"supported", "bounded"} and ns not in observed.get(fmt, set()):
                warnings.append(
                    f"declared '{fmt}.{ns}' as '{status}' but no fixture emits it "
                    f"(fixture coverage gap — not a failure)"
                )

    return errors, warnings


def rewrite_capabilities(capabilities: dict, observed: dict[str, set[str]]) -> dict:
    """Return a new capabilities dict with observed pairs promoted/added.

    Preserves key order and hand-curated ``bounded``/``supported_tags`` markers.
    ``not_yet_supported`` declarations that are observed get promoted to
    ``supported``. Observed namespaces not declared at all are appended to the
    end of the container's ``namespaces`` map with status ``supported``.
    """
    updated = json.loads(json.dumps(capabilities))  # deep copy preserving order
    containers = updated.setdefault("containers", {})
    for fmt, ns_set in observed.items():
        container = containers.setdefault(fmt, {"namespaces": {}})
        declared_ns = container.setdefault("namespaces", {})
        for ns in sorted(ns_set):
            current = declared_ns.get(ns)
            if current is None:
                declared_ns[ns] = "supported"
            elif current == "not_yet_supported":
                declared_ns[ns] = "supported"
            # Preserve 'bounded' and existing 'supported' as-is.
    return updated


def dump_capabilities(capabilities: dict) -> str:
    return json.dumps(capabilities, indent=2) + "\n"


def _self_test(observed: dict[str, set[str]]) -> None:
    """Smoke assertions over the observed map for the current fixture corpus.

    These are intentionally narrow — they guard the keying convention
    (``detected_format``) and the presence of known-good metadata in specific
    fixtures. Additional fixtures extending the corpus should either add or
    relax these assertions.
    """
    assert "heif" in observed, "expected 'heif' detected_format from .heic fixtures"
    assert {"exif", "xmp", "icc", "iptc"}.issubset(
        observed["heif"]
    ), f"heif must surface exif+xmp+icc+iptc, got {sorted(observed['heif'])}"
    assert "mp4" in observed, "expected 'mp4' detected_format from .mp4 fixtures"
    assert {"quicktime"}.issubset(
        observed["mp4"]
    ), f"mp4 must surface quicktime, got {sorted(observed['mp4'])}"
    assert "mov" in observed, "expected 'mov' detected_format from .mov fixtures"
    assert {"quicktime"}.issubset(
        observed["mov"]
    ), f"mov must surface quicktime, got {sorted(observed['mov'])}"
    assert "jpeg" in observed and "exif" in observed["jpeg"], (
        "jpeg fixtures must surface exif"
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="fail on under-reporting drift (default)",
    )
    mode.add_argument(
        "--write",
        action="store_true",
        help="rewrite CAPABILITIES.json in place, promoting observed pairs",
    )
    args = parser.parse_args(argv)

    capabilities = json.loads(CAPABILITIES.read_text())
    observed = observed_map(FIXTURES)
    _self_test(observed)

    if args.write:
        updated = rewrite_capabilities(capabilities, observed)
        CAPABILITIES.write_text(dump_capabilities(updated))
        print(f"wrote {CAPABILITIES.relative_to(ROOT)}")
        return 0

    errors, warnings = diff_against_declared(observed, capabilities)
    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)
    if errors:
        for err in errors:
            print(f"error: {err}", file=sys.stderr)
        print(
            f"CAPABILITIES.json drift detected ({len(errors)} "
            f"under-reporting error(s)); run 'python3 tools/generate_capabilities.py --write'",
            file=sys.stderr,
        )
        return 1
    print("CAPABILITIES.json matches observed fixture output")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
