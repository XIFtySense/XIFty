# XIFty

XIFty is a modern metadata engine for media files.

It is being built as a cleaner architectural foundation for metadata work, not
as a smaller clone of ExifTool. The goal is to make metadata more
understandable, trustworthy, and embeddable by separating parsing,
interpretation, normalization, validation, and conflict reporting.

## Why XIFty

Metadata in real files is messy:

- multiple namespaces can overlap
- timestamps can conflict
- vendor-specific metadata can matter as much as standards metadata
- applications still need stable, app-facing answers

XIFty makes that model explicit through four views of the same asset:

- `raw`
- `interpreted`
- `normalized`
- `report`

## Browser Demo

Try the live browser demo:

- [XIFty Web Demo](https://xiftysense.github.io/XIFty/)

The current browser path is intentionally narrower than the native/server
surface. It is aimed first at local, in-browser inspection of still images,
with files processed locally in the browser rather than uploaded to a backend.

The current demo experience emphasizes readable inspection instead of raw JSON
alone:

- structured normalized metadata facts
- grouped complete-field inventories
- explicit `report` issues and conflicts
- readable timestamps and GPS when present
- copyable JSON when you still want the exact envelope

## AWS Lambda

The recommended AWS serverless production path today is the Node binding on
Lambda, not the browser/WASM surface.

Start here:

- [docs/adoption/AWS_LAMBDA_NODE.md](./docs/adoption/AWS_LAMBDA_NODE.md)
- [examples/aws-sam-node](./examples/aws-sam-node)

This Lambda path is now validated in the main CI workflow through:

- local fixture invocation
- layer assembly
- `sam validate`
- `sam build`

## Quickstart

Build and run the CLI directly from the repo:

```bash
cargo run -p xifty-cli -- probe fixtures/minimal/happy.jpg
cargo run -p xifty-cli -- extract fixtures/minimal/happy.jpg
cargo run -p xifty-cli -- extract fixtures/minimal/gps.jpg --view normalized
```

Or install the CLI locally from the workspace:

```bash
cargo install --path crates/xifty-cli
xifty-cli probe fixtures/minimal/happy.jpg
```

The two core commands are:

- `probe <path>`: detect the container and surface top-level issues
- `extract <path> [--view raw|interpreted|normalized|report]`: emit the JSON
  envelope or a selected view

## What XIFty Supports Today

Current container coverage:

- JPEG / TIFF
- PNG / WebP
- HEIF / HEIC
- MP4 / MOV
- FLAC

Current namespace coverage:

- EXIF
- XMP
- bounded ICC
- bounded IPTC
- bounded QuickTime
- selected Sony and Apple vendor metadata paths
- bounded Vorbis comment (FLAC)
- bounded FLAC stream info (sample rate, channels, bit depth, duration, embedded picture)

Current product surfaces:

- CLI
- JSON-first `C ABI`
- a minimal C example proving the ABI seam locally
- extracted org repos for Node, Swift, Python, Go, Rust, and C++

Support claims are tracked explicitly in [CAPABILITIES.json](./CAPABILITIES.json).
Keep that artifact narrow and honest.

The public JSON contract is also tracked explicitly through checked-in schema
artifacts in [schemas/](./schemas/) and the schema lifecycle rules in
[docs/SCHEMA_POLICY.md](./docs/SCHEMA_POLICY.md).

Release guardrails for core and package surfaces live in
[docs/RELEASE_CHECKLIST.md](./docs/RELEASE_CHECKLIST.md).

Canonical `xifty-ffi` runtime bundles are now defined in
[docs/adoption/CORE_RUNTIME_ARTIFACTS.md](./docs/adoption/CORE_RUNTIME_ARTIFACTS.md).

## Binding Maturity

XIFty’s public bindings no longer all claim the same maturity level.

| Repo | Package / Install | Current maturity | Supported targets |
| --- | --- | --- | --- |
| [XIFtyNode](https://github.com/XIFtySense/XIFtyNode) | `npm install @xifty/xifty` | Canonical production package today | `macos-arm64`, `linux-x64` |
| [XIFtyPython](https://github.com/XIFtySense/XIFtyPython) | wheel build, PyPI not yet default | Release-ready package target | `macos-arm64`, `linux-x64` |
| [XIFtyRust](https://github.com/XIFtySense/XIFtyRust) | crate repo, crates.io not yet default | Release-ready but still source-first | current runtime artifacts on `macos-arm64`, `linux-x64` |
| [XIFtySwift](https://github.com/XIFtySense/XIFtySwift) | SwiftPM source dependency | Source-first binding | source-first |
| [XIFtyGo](https://github.com/XIFtySense/XIFtyGo) | Go module source dependency | Source-first binding | source-first |
| [XIFtyCpp](https://github.com/XIFtySense/XIFtyCpp) | CMake / source integration | Source-first binding | source-first |

For non-Node bindings, the preferred native-runtime contract is now:

1. bundled runtime inside the package, if present
2. `XIFTY_RUNTIME_DIR`, if explicitly set
3. repo-local runtime cache populated from canonical core release artifacts
4. `XIFTY_CORE_DIR` only as an explicit source-tree override

## What Makes It Different

XIFty is opinionated about structure:

- container parsing and metadata interpretation stay separate
- normalized fields are policy-driven
- provenance, conflicts, and issues are first-class output concerns
- malformed files are reported explicitly instead of hand-waved away
- embeddability matters as much as CLI ergonomics

## Repository Map

Start here:

- [VISION.md](./VISION.md): product thesis and long-term ambition
- [STATE_OF_THE_PROJECT.md](./STATE_OF_THE_PROJECT.md): honest current-state assessment
- [CONTRIBUTING.md](./CONTRIBUTING.md): contributor entry point
- [ENGINEERING_PRINCIPLES.md](./ENGINEERING_PRINCIPLES.md): clean-code and clean-architecture expectations
- [FFI_CONTRACT.md](./FFI_CONTRACT.md): embedding contract for the `C ABI`
- [docs/SCHEMA_POLICY.md](./docs/SCHEMA_POLICY.md): schema-versioning and compatibility rules for the JSON envelope
- [docs/RELEASE_CHECKLIST.md](./docs/RELEASE_CHECKLIST.md): shipped-artifact verification checklist for core and Node releases
- [docs/README.md](./docs/README.md): architecture, research, and iteration history

Important directories:

- `crates/`: Rust workspace crates
- `demo/`: browser demo assets and local demo instructions
- `fixtures/minimal/`: checked-in fixtures for examples and tests
- `fixtures/local/`: local-only larger real-world regression fixtures
- `examples/`: minimal core-repo examples, currently centered on the C ABI seam
  plus the AWS SAM Node Lambda example
- `fuzz/`: parser and routing fuzz targets
- `schemas/`: checked-in JSON schema artifacts for the public envelope

## Verification

Core verification:

```bash
cargo test --workspace
```

The main CI workflow is intentionally kept free of extra test-only tooling and
system dependencies. Merge-blocking hygiene gates that are cheap and
deterministic — the `cbindgen`-based header staleness check and the JSON
schema artifact validation — run on every pull request and push to `main` via
the `Docs And Contract` job of the hygiene workflow. Oracle-backed
differential checks that require ExifTool remain opt-in and run only on the
weekly schedule and on manual dispatch, keeping the PR path free of external
system dependencies.

The main CI workflow now also validates the AWS Lambda Node adoption path
through the checked-in SAM example.

FFI verification:

```bash
cbindgen --config cbindgen.toml --crate xifty-ffi --output include/xifty.h --lang c
cargo test -p xifty-ffi
```

## Public Binding Repos

- [XIFtyNode](https://github.com/XIFtySense/XIFtyNode)
- [XIFtySwift](https://github.com/XIFtySense/XIFtySwift)
- [XIFtyPython](https://github.com/XIFtySense/XIFtyPython)
- [XIFtyGo](https://github.com/XIFtySense/XIFtyGo)
- [XIFtyRust](https://github.com/XIFtySense/XIFtyRust)
- [XIFtyCpp](https://github.com/XIFtySense/XIFtyCpp)

Those repos are now intentionally tiered instead of pretending equal maturity.
Node is the canonical production package today. Python is the first binding on
the new self-contained runtime-artifact path. Rust is cleaner and more
release-ready now, but still honestly source-first. Swift, Go, and C++ remain
source-first until their runtime/distribution story is hardened further.

The main XIFty repository is intentionally the core engine repo. Canonical
language package implementations now live in their own repositories rather than
remaining duplicated under this repo.

## Notes

Large real-world camera/media examples are intentionally not stored in git.
Keep those under `fixtures/local/` when you want optional real-camera
regression and differential tests to run.

Fuzz targets are checked in under `fuzz/`. The earlier parser targets were
smoke-tested with `cargo fuzz run` under nightly Rust; some newer targets are
still awaiting a cleaner local nightly `cargo-fuzz` resolution on this machine.
