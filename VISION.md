# XIFty Vision

## What XIFty Is

XIFty is a modern metadata engine for media files.

It is not intended to be a smaller clone of ExifTool. The goal is to build a better foundation: a portable, high-performance system that treats metadata extraction, interpretation, normalization, validation, and conflict handling as distinct but connected concerns.

ExifTool won through breadth. XIFty should win through architecture.

## Product Thesis

Metadata is not a flat map of tags.

Real-world media often contains overlapping, conflicting, partial, malformed, or vendor-specific metadata spread across multiple containers and namespaces. A modern tool should not just dump tags. It should help users understand:

- what metadata exists
- where it came from
- how it was interpreted
- which values conflict
- which values are trustworthy
- how stable app-facing fields should be derived

XIFty is designed to make that model explicit.

## Core Principles

- Read-first foundation before write support
- Clear separation between container parsing, metadata interpretation, and normalized output
- First-class provenance, conflict reporting, and validation
- Portable, embeddable, and fast by default
- Stable app-facing schema without hiding raw source data
- Capability-driven format support instead of vague claims
- Strong support for malformed and messy real-world files

## What Makes XIFty Different

XIFty should expose multiple views of the same asset:

- `raw`: exact namespaces, tags, and source values
- `interpreted`: decoded values, units, enums, and typed fields
- `normalized`: stable app-facing schema
- `report`: warnings, conflicts, confidence, provenance, and validation notes

That makes it useful for both application developers and power users.

## Intended Users

- Developers building media-heavy products
- Teams that need stable JSON metadata output
- Systems that need embeddable metadata inspection in services, CLIs, or apps
- Workflows that compare, validate, or audit metadata across tools
- Forensic or archive-adjacent workflows that care about provenance and ambiguity

## Technical Direction

XIFty should use:

- `Rust` for the core engine
- `C ABI` as the stable embedding surface
- Thin bindings for `Node`, `Python`, and `Swift`
- `TypeScript` for SDK ergonomics, docs, and an inspector UI
- `Python` for corpus analysis, differential testing, fuzz triage, and comparison tooling

## Architectural Shape

The system should be modular and layered:

- source and IO adapters
- container parsers
- metadata namespace parsers
- semantic model
- normalization layer
- policy and precedence rules
- validation and integrity reporting
- CLI and FFI surfaces

The core architectural rule is:

container parsing and metadata interpretation must remain separate

That keeps the system clean as support expands across formats and namespaces.

## Internal Model

The core should be built around strongly typed concepts such as:

- `Asset`
- `Source`
- `ContainerNode`
- `MetadataEntry`
- `Value`
- `Provenance`
- `Conflict`
- `Issue`
- `NormalizedField`

## MVP Scope

The first meaningful version should focus on:

- JPEG / TIFF
- PNG / WebP
- HEIC / HEIF
- MP4 / MOV
- EXIF / XMP / IPTC / ICC / QuickTime keys
- raw + normalized JSON output
- conflict and validation reporting
- differential comparison against ExifTool

Write support should stay out of v1.

## Long-Term Ambition

XIFty should grow into a metadata platform core that can support:

- broad media inspection
- reliable normalization for applications
- validation and repair-oriented workflows
- multiple language bindings
- desktop, server, CLI, and embedded use cases

The long-term goal is not just to parse metadata.

It is to make metadata understandable, trustworthy, and programmable.
