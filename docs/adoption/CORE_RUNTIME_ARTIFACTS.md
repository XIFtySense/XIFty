# Core Runtime Artifacts

XIFty now defines a canonical runtime artifact for `xifty-ffi`.

These artifacts are intended to give bindings and downstream integrations a
stable runtime bundle instead of requiring source checkouts by default.

## Artifact Name

Release assets use this pattern:

- `xifty-runtime-<target>-v<core_version>.tar.gz`

Current iteration targets:

- `macos-arm64`
- `linux-x64`

## Artifact Layout

Each runtime artifact unpacks to a single root directory containing:

- `manifest.json`
- `include/xifty.h`
- `lib/libxifty_ffi.dylib` or `lib/libxifty_ffi.so`

## Manifest Contract

`manifest.json` contains:

- `core_version`
- `schema_version`
- `target`
- `os`
- `arch`
- `library_file`

Bindings should treat this manifest as the runtime contract for locating the
native library and understanding which core/schema version they are using.

## Binding Resolution Contract

For bindings that consume these artifacts directly, the preferred runtime
resolution order is:

1. bundled runtime inside the package, if present
2. `XIFTY_RUNTIME_DIR`, if explicitly set
3. repo-local runtime cache populated from these canonical release artifacts
4. `XIFTY_CORE_DIR` only as an explicit source-tree override for maintainers

Bindings should not infer a source-build path from sibling checkouts or stale
caches.

## Intended Consumers

The first intended consumers are:

- Python wheel builds
- Rust release-validation and runtime-backed local use
- future binding/package hardening work

Node remains on its native prebuild model and does not consume this runtime
artifact directly.

## Validation

The core repo provides:

- `tools/build-runtime-artifact.py`
- `tools/validate-runtime-artifact.py`

Use them to build and validate runtime bundles locally.

Example:

```bash
python3 tools/build-runtime-artifact.py --output /tmp/xifty-runtime.tar.gz
python3 tools/validate-runtime-artifact.py --artifact /tmp/xifty-runtime.tar.gz
```
