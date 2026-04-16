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

This repository is currently in the definition stage. The next steps are to formalize:

- normalized schema
- crate boundaries
- CLI surface
- FFI contract
- corpus and differential test strategy

Planning docs:

- [VISION.md](./VISION.md)
- [RESEARCH.md](./RESEARCH.md)
- [ARCHITECTURE_PLAN.md](./ARCHITECTURE_PLAN.md)
- [ENGINEERING_PRINCIPLES.md](./ENGINEERING_PRINCIPLES.md)
- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [AGENTS.md](./AGENTS.md)
