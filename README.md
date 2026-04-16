# XIFty

XIFty is a modern metadata engine for media files.

It is being designed as a better architectural foundation for metadata work, not just a smaller clone of ExifTool. The focus is on portable parsing, clean layering, stable normalized output, provenance, conflict handling, and validation.

## Vision

Metadata in real files is messy. Multiple namespaces can overlap, timestamps can conflict, tags can be malformed, and app-facing systems still need a stable answer.

XIFty is intended to expose four useful views of an asset:

- `raw` source tags and namespaces
- `interpreted` typed and decoded values
- `normalized` stable app-facing fields
- `report` warnings, conflicts, provenance, and confidence notes

That makes it useful for both pipelines and humans.

For the fuller product framing, see [VISION.md](./VISION.md).

## Technical Direction

The current direction is:

- `Rust` core
- `C ABI` as the stable embedding surface
- bindings for `Node`, `Python`, and `Swift`
- `TypeScript` for docs, SDK ergonomics, and inspector UI
- `Python` tooling for corpus analysis, fuzz triage, and ExifTool comparison

## Architecture

XIFty is expected to follow a layered architecture:

- source and IO adapters
- format and container parsers
- metadata namespace parsers
- semantic model
- normalization layer
- policy and precedence rules
- validation and reporting
- CLI and FFI surfaces

One core rule shapes the design:

container parsing and metadata interpretation must stay separate

## MVP

The initial target scope is:

- JPEG / TIFF
- PNG / WebP
- HEIC / HEIF
- MP4 / MOV
- EXIF / XMP / IPTC / ICC / QuickTime keys
- raw + normalized JSON output
- validation and conflict reporting
- ExifTool differential comparison tooling

Write support is intentionally out of scope for v1.

## Status

This repository now includes the first implementation slice for:

- JPEG / TIFF detection
- JPEG APP1 EXIF extraction
- TIFF / IFD traversal
- PNG / WebP EXIF and XMP routing
- HEIC / HEIF detection and initial ISOBMFF routing
- EXIF decoding for the initial normalized fields
- XMP decoding and EXIF/XMP reconciliation
- JSON-only CLI output
- checked-in fixtures
- snapshot tests plus ExifTool differential tests for the currently supported oracle-backed fixtures
- vendored real-world HEIF differential coverage for iteration three

Current CLI:

```bash
cargo run -p xifty-cli -- probe fixtures/minimal/happy.jpg
cargo run -p xifty-cli -- extract fixtures/minimal/happy.jpg
cargo run -p xifty-cli -- extract fixtures/minimal/gps.jpg --view normalized
cargo run -p xifty-cli -- extract fixtures/minimal/mixed.heic --view normalized
```

Verification:

```bash
cargo test --workspace
```

Fuzz targets are scaffolded under `fuzz/`. The earlier parser targets were smoke-tested with `cargo fuzz run` under a nightly Rust toolchain; the newer ISOBMFF and HEIF-routing targets are checked in and await a clean local nightly `cargo-fuzz` resolution on this machine.

Planning docs:

- [VISION.md](./VISION.md)
- [RESEARCH.md](./RESEARCH.md)
- [ARCHITECTURE_PLAN.md](./ARCHITECTURE_PLAN.md)
- [ITERATION_ONE_CHECKLIST.md](./ITERATION_ONE_CHECKLIST.md)
- [ITERATION_TWO_PLAN.md](./ITERATION_TWO_PLAN.md)
- [ITERATION_TWO_CHECKLIST.md](./ITERATION_TWO_CHECKLIST.md)
- [ITERATION_THREE_PLAN.md](./ITERATION_THREE_PLAN.md)
- [ITERATION_THREE_CHECKLIST.md](./ITERATION_THREE_CHECKLIST.md)
- [ENGINEERING_PRINCIPLES.md](./ENGINEERING_PRINCIPLES.md)
- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [AGENTS.md](./AGENTS.md)
