# XIFty Vision (factory north star)

> This file is the factory-facing distillation of the product vision. The
> full source VISION document is appended verbatim at the bottom so agents
> can always fall back to the canonical text.

## Purpose

XIFty is a modern metadata engine for media files. It is not a smaller
ExifTool clone — it is a portable, high-performance Rust core with a stable
C ABI embedding surface that treats metadata extraction, interpretation,
normalization, validation, and conflict handling as distinct but connected
concerns. ExifTool won through breadth; XIFty wins through architecture.

The factory exists to drive that architecture forward: expanding format
coverage, hardening the normalization and validation layers, growing
language bindings, and keeping the differential-comparison surface honest
against real-world messy files.

## Principles

1. **Read-first foundation** — write support stays out of v1; extraction
   and interpretation must be rock-solid before mutation is considered.
2. **Separation of concerns** — container parsing and metadata
   interpretation live in different crates and never leak into each other.
   This is a load-bearing architectural rule, not a style preference.
3. **First-class provenance** — every value can trace back to its source
   namespace, container node, and byte range. Conflicts are reported, not
   silently resolved.
4. **Portable, embeddable, fast by default** — the core runs in services,
   CLIs, apps, and WASM. Performance regressions are bugs.
5. **Stable app-facing schema without hiding raw data** — the normalized
   view is for applications; the `raw` and `interpreted` views stay
   available for power users and audit workflows.
6. **Capability-driven format support** — what XIFty claims to support is
   expressed as concrete capabilities (read EXIF, decode ICC profile,
   reconcile QuickTime keys), not vague format names.
7. **Strong support for malformed and messy files** — real-world media is
   broken; the parsers degrade gracefully and surface issues as typed
   `Issue` / `Conflict` values rather than panics or silent loss.
8. **Multiple views of the same asset** — `raw`, `interpreted`,
   `normalized`, and `report` are all first-class outputs.
9. **Differential honesty** — behavior is continuously compared against
   ExifTool and other tools via Python differential tooling to catch
   regressions and disagreements early.
10. **FFI as a contract** — the C ABI exposed by `xifty-ffi` is a stable
    interface governed by `FFI_CONTRACT.md`; breaking changes are
    deliberate and documented.

## Non-goals

- **v1 write support** — XIFty v1 does not modify files. Repair/write
  workflows are deferred.
- **Feature parity with ExifTool** — coverage grows by value and
  capability, not by racing a tag count.
- **Hiding raw source data behind a cleaned-up facade** — normalization
  never deletes or obscures raw namespace values.
- **Vendor-specific lock-in in the normalized schema** — app-facing fields
  stay portable; vendor quirks live in the `interpreted` view.
- **A single monolithic crate** — the workspace separation between
  container, metadata, normalize, policy, validate, json, ffi, cli, and
  wasm is intentional and preserved.
- **Native bindings with thick logic** — Node, Python, and Swift bindings
  stay thin shims over the C ABI.
- **Silent lossy parsing** — malformed input surfaces issues; it does not
  get quietly dropped.

## MVP scope (reference for roadmap prioritization)

- Containers: JPEG / TIFF, PNG / WebP, HEIC / HEIF, MP4 / MOV
- Metadata: EXIF / XMP / IPTC / ICC / QuickTime keys
- Output: raw + normalized JSON, plus conflict and validation reporting
- Differential comparison against ExifTool

---

# Canonical VISION.md (verbatim, appended)

The following is the unmodified source of `/VISION.md` at the repo root at
factory-setup time. If anything above disagrees with what follows, the
canonical source below wins.

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
