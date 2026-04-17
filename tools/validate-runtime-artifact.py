#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import tarfile
import tempfile
from pathlib import Path


def validate_dir(runtime_root: Path) -> dict:
    manifest_path = runtime_root / "manifest.json"
    include_dir = runtime_root / "include"
    lib_dir = runtime_root / "lib"

    if not manifest_path.exists():
        raise SystemExit(f"missing manifest.json in {runtime_root}")
    if not include_dir.joinpath("xifty.h").exists():
        raise SystemExit(f"missing include/xifty.h in {runtime_root}")
    if not lib_dir.exists():
        raise SystemExit(f"missing lib directory in {runtime_root}")

    manifest = json.loads(manifest_path.read_text())
    required_keys = {
        "core_version",
        "schema_version",
        "target",
        "os",
        "arch",
        "library_file",
    }
    missing = required_keys - set(manifest)
    if missing:
        raise SystemExit(f"manifest missing keys: {sorted(missing)}")

    library_path = lib_dir / manifest["library_file"]
    if not library_path.exists():
        raise SystemExit(f"missing runtime library {library_path}")

    print(
        f"validated runtime artifact for {manifest['target']} "
        f"(core {manifest['core_version']}, schema {manifest['schema_version']})"
    )
    return manifest


def validate_tarball(artifact: Path):
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        with tarfile.open(artifact, "r:gz") as archive:
            try:
                archive.extractall(temp_path, filter="data")
            except TypeError:
                archive.extractall(temp_path)
        roots = [path for path in temp_path.iterdir() if path.is_dir()]
        if len(roots) != 1:
            raise SystemExit("runtime artifact must unpack to exactly one root directory")
        validate_dir(roots[0])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", help="Path to a runtime artifact tar.gz")
    parser.add_argument("--runtime-dir", help="Path to an unpacked runtime directory")
    args = parser.parse_args()

    if bool(args.artifact) == bool(args.runtime_dir):
        raise SystemExit("provide exactly one of --artifact or --runtime-dir")

    if args.artifact:
        validate_tarball(Path(args.artifact).resolve())
    else:
        validate_dir(Path(args.runtime_dir).resolve())


if __name__ == "__main__":
    main()
