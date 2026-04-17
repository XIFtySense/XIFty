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

Current namespace coverage:

- EXIF
- XMP
- bounded ICC
- bounded IPTC
- bounded QuickTime
- selected Sony and Apple vendor metadata paths

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

## Verification

Core verification:

```bash
cargo test --workspace
```

The main CI workflow is intentionally kept free of extra test-only tooling and
system dependencies. Oracle-backed differential checks that use ExifTool and
header-regeneration checks that use `cbindgen` live in a separate optional
hygiene workflow rather than the default core validation path.

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

Several of those repos are now usable and documented publicly, but distribution
hardening is still an active next-stage concern. Node is the most mature
consumer package today; the others remain more source-first than turnkey.

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
