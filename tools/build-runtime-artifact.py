#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def workspace_version() -> str:
    cargo_toml = ROOT / "Cargo.toml"
    content = cargo_toml.read_text()
    match = re.search(r'version = "([^"]+)"', content)
    if not match:
        raise SystemExit("failed to determine workspace version from Cargo.toml")
    return match.group(1)


def schema_version() -> str:
    content = (ROOT / "crates/xifty-core/src/lib.rs").read_text()
    match = re.search(r'SCHEMA_VERSION: &str = "([^"]+)"', content)
    if not match:
        raise SystemExit("failed to determine schema version from xifty-core")
    return match.group(1)


def host_target() -> tuple[str, str, str, str]:
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin" and machine in {"arm64", "aarch64"}:
        return ("macos-arm64", "macos", "arm64", "libxifty_ffi.dylib")
    if system == "linux" and machine in {"x86_64", "amd64"}:
        return ("linux-x64", "linux", "x64", "libxifty_ffi.so")

    raise SystemExit(f"unsupported host for runtime artifact build: {system} / {machine}")


def build_release():
    subprocess.run(
        ["cargo", "build", "-p", "xifty-ffi", "--release"],
        cwd=ROOT,
        check=True,
    )


def create_artifact(output: Path):
    target, os_name, arch, library_file = host_target()
    core_version = workspace_version()
    schema = schema_version()

    build_release()

    lib_path = ROOT / "target/release" / library_file
    header_path = ROOT / "include/xifty.h"
    if not lib_path.exists():
        raise SystemExit(f"missing built library: {lib_path}")
    if not header_path.exists():
        raise SystemExit(f"missing header: {header_path}")

    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as temp_dir:
        stage_root = Path(temp_dir) / f"xifty-runtime-{target}-v{core_version}"
        include_dir = stage_root / "include"
        lib_dir = stage_root / "lib"
        include_dir.mkdir(parents=True)
        lib_dir.mkdir(parents=True)

        shutil.copy2(header_path, include_dir / "xifty.h")
        shutil.copy2(lib_path, lib_dir / library_file)

        manifest = {
            "core_version": core_version,
            "schema_version": schema,
            "target": target,
            "os": os_name,
            "arch": arch,
            "library_file": library_file,
        }
        (stage_root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

        with tarfile.open(output, "w:gz") as archive:
            archive.add(stage_root, arcname=stage_root.name)

    print(output)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, help="Path to the output tar.gz artifact")
    args = parser.parse_args()
    create_artifact(Path(args.output).resolve())


if __name__ == "__main__":
    main()
