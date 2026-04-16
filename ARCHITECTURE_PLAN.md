# XIFty Architecture And Implementation Plan

This document turns the research in [RESEARCH.md](./RESEARCH.md) into an opinionated plan for how XIFty should be built.

## Goals

XIFty should become a modern metadata engine that:

- reads metadata from still images and media containers
- preserves raw provenance
- produces stable normalized output
- explains ambiguity and conflicts
- is embeddable from multiple languages
- remains maintainable as format coverage grows

## Non-Goals For V1

The first version should not try to:

- match ExifTool’s full file-format coverage
- provide broad write support
- provide metadata repair or mutation flows
- solve every maker-note edge case
- expose a large public API surface before the internal model settles

## Core Product Shape

Every extraction should be able to produce four views:

- `raw`
- `interpreted`
- `normalized`
- `report`

The outputs should be related but distinct.

### Raw

Exact container, namespace, tag path, stored value, offsets when useful, and decode notes.

### Interpreted

Typed values, decoded enums, units, timestamp parsing results, and structural meaning.

### Normalized

Stable application-facing fields like:

- `captured_at`
- `created_at`
- `modified_at`
- `device.make`
- `device.model`
- `dimensions.width`
- `dimensions.height`
- `orientation`
- `location`
- `duration`
- `codec.video`
- `codec.audio`
- `color.profile`
- `author`
- `copyright`

### Report

Warnings, conflicts, confidence, precedence decisions, parse issues, unknown blocks, and integrity notes.

## Architectural Principles

### 1. Container parsing and metadata interpretation stay separate

Examples:

- `jpeg` finds markers and payloads
- `tiff` navigates IFD structures
- `isobmff` finds atoms and item structures
- `quicktime` interprets metadata from selected ISOBMFF atoms
- `normalize` maps interpreted fields into stable schema

This is the most important structural rule in the whole plan.

### 2. The internal model is graph-like, not key-value only

Metadata must be traceable back to source.

Recommended core entities:

- `Asset`
- `SourceRef`
- `ContainerNode`
- `MetadataBlock`
- `MetadataEntry`
- `TypedValue`
- `Evidence`
- `Conflict`
- `Issue`
- `NormalizedField`

### 3. Capabilities are explicit

Each format or namespace module should declare capabilities such as:

- detect
- parse structure
- extract raw metadata
- interpret fields
- stream support
- preserve offsets
- validation coverage
- test-corpus coverage

This keeps roadmap claims honest.

### 4. The ABI stays narrow

The public low-level interface should be a small `C ABI`.

Wrappers for `Python`, `Node`, and `Swift` should expose more ergonomic APIs, but the shared core contract should remain compact and durable.

## Workspace Layout

Recommended initial workspace:

```text
xifty/
  Cargo.toml
  crates/
    xifty-core
    xifty-source
    xifty-detect
    xifty-container-jpeg
    xifty-container-tiff
    xifty-container-png
    xifty-container-riff
    xifty-container-isobmff
    xifty-meta-exif
    xifty-meta-xmp
    xifty-meta-iptc
    xifty-meta-icc
    xifty-meta-quicktime
    xifty-normalize
    xifty-policy
    xifty-validate
    xifty-json
    xifty-cli
    xifty-ffi
  bindings/
    python/
    node/
    swift/
  tools/
    corpus-audit/
    exiftool-compare/
    fixture-min/
```

## Crate Responsibilities

### `xifty-core`

- shared types
- error types
- issue model
- typed value enums
- trait contracts between layers

### `xifty-source`

- file input
- byte slice input
- buffered reader input
- mmap support later
- offset-safe read helpers

Recommendation:

Start with a cursor-based byte reader abstraction that is easy to audit and profile. Avoid premature generic complexity.

### `xifty-detect`

- file signature sniffing
- lightweight container identification
- multi-pass detection fallback when needed

### Container crates

Responsibilities:

- parse structural units only
- expose stable internal nodes and offsets
- avoid embedding namespace semantics

Examples:

- `xifty-container-jpeg`: markers, APP segments
- `xifty-container-tiff`: header, endianness, IFD navigation
- `xifty-container-png`: chunks, payload routing
- `xifty-container-riff`: RIFF chunk tree
- `xifty-container-isobmff`: atoms, brands, item/track structures

### Metadata namespace crates

Responsibilities:

- decode namespace-specific payloads
- return typed metadata entries
- annotate provenance and decode quality

Examples:

- `xifty-meta-exif`
- `xifty-meta-xmp`
- `xifty-meta-iptc`
- `xifty-meta-icc`
- `xifty-meta-quicktime`

### `xifty-normalize`

- maps interpreted metadata into stable fields
- preserves derivation and provenance
- does not silently discard conflicts

### `xifty-policy`

- precedence rules
- trust scoring
- timestamp heuristics
- source preference profiles

This crate should hold policy, not parsers.

### `xifty-validate`

- malformed structure reporting
- suspicious-offset detection
- impossible-value detection
- namespace/container inconsistency checks

### `xifty-json`

- canonical JSON serialization for all output modes
- versioned output schema envelopes

### `xifty-cli`

- user-facing commands
- file and batch workflows
- JSON and human-readable output

### `xifty-ffi`

- stable `C ABI`
- memory ownership rules
- opaque handle types
- panic boundaries converted to error results

## Public API Shape

The low-level API should be handle-based and conservative.

Recommended C-facing operations:

- create extractor config
- analyze file / bytes
- request output modes
- retrieve JSON result
- free result buffer

Do not expose Rust-native data structures over the ABI.

## JSON Envelope

Recommended top-level shape:

```json
{
  "schema_version": "0.1.0",
  "input": {
    "path": "sample.heic",
    "detected_format": "heif",
    "container": "isobmff"
  },
  "raw": {},
  "interpreted": {},
  "normalized": {},
  "report": {
    "issues": [],
    "conflicts": [],
    "confidence": {}
  }
}
```

The `schema_version` should be explicit from day one.

## FFI Strategy

### C ABI first

Expose:

- plain integers
- enums with explicit repr
- opaque pointers
- owned buffers with matching free functions

Avoid:

- callback-heavy interfaces in v1
- passing nested structs across the ABI
- exposing Rust allocation semantics directly

### Python

Use `PyO3`, but build the wrapper against the C ABI or a deliberately narrow Rust facade.

### Node

Use `napi-rs` for the JS wrapper, again keeping wrapper logic thin.

### Swift

Wrap the `C ABI` with Swift-friendly value types.

Do not make Swift/C++ interop the primary bridge because the public contract should stay more stable than current C++ interop evolution.

## Parser Strategy

Recommendation:

- Prefer explicit cursor-based parsers for container layers.
- Use small decode helpers and typed readers instead of building a parser-combinator-heavy core.
- Reserve third-party crates for XML or highly specialized cases when they clearly reduce risk.

Reasoning:

- container formats are often easier to audit when the parser flow is imperative
- offset tracking and malformed-file behavior are easier to control
- hot paths become easier to profile

This is an architectural preference, not a purity rule.

## Initial Format Scope

V1 should support:

- JPEG / TIFF
- PNG / WebP
- HEIC / HEIF
- MP4 / MOV

Namespaces:

- EXIF
- XMP
- IPTC
- ICC
- QuickTime keys and related metadata atoms

## Implementation Phases

## Phase 0: Contracts and skeleton

Deliverables:

- workspace skeleton
- core types
- issue model
- JSON envelope
- CLI stub
- C ABI stub

Exit criteria:

- crate boundaries compile
- one no-op extraction path returns valid JSON envelope

## Phase 1: Detection and still-image foundations

Deliverables:

- source abstraction
- detection crate
- JPEG parser
- TIFF parser
- EXIF decoder
- basic normalization for timestamps, device info, dimensions, orientation

Exit criteria:

- JPEG and TIFF fixtures produce raw + interpreted + normalized output
- snapshot tests exist
- ExifTool comparison harness starts producing deltas

## Phase 2: PNG / WebP and XMP / ICC

Deliverables:

- PNG parser
- WebP container support
- XMP parser integration
- ICC extraction
- precedence rules between EXIF and XMP for overlapping fields

Exit criteria:

- normalized conflict reporting exists for common timestamp collisions
- mixed EXIF/XMP assets are covered by regression tests

## Phase 3: ISOBMFF / QuickTime / HEIF

Deliverables:

- ISOBMFF parser
- QuickTime metadata interpretation
- HEIF item metadata extraction
- MP4 / MOV metadata support

Exit criteria:

- HEIC and MOV sample assets produce stable normalized output
- container/namespaces remain cleanly separated in code structure

## Phase 4: Validation and hardening

Deliverables:

- stronger issue taxonomy
- malformed-file validation
- fuzz targets
- larger corpus differential testing

Exit criteria:

- cargo-fuzz targets cover all primary container readers
- known malformed fixtures produce stable, non-crashing results

## Phase 5: Bindings

Deliverables:

- Python wrapper
- Node wrapper
- Swift wrapper

Exit criteria:

- wrappers expose the same schema envelope
- wrapper tests run against shared fixture corpus

## Testing Plan

XIFty should use four testing layers:

### 1. Unit tests

For byte readers, offsets, decode helpers, and field parsing.

### 2. Snapshot tests

For:

- raw JSON
- normalized JSON
- validation reports

### 3. Differential tests

Compare against ExifTool for:

- discovered fields
- interpreted values
- unsupported or unknown tags
- precedence mismatches

### 4. Fuzzing

Start with:

- JPEG markers
- TIFF IFD traversal
- PNG chunk iteration
- ISOBMFF atom traversal

## Corpus Strategy

Maintain three fixture tiers:

### Minimal curated corpus

Small checked-in files covering happy paths and targeted edge cases.

### Differential corpus

Larger sample set used for ExifTool comparisons.

### Malformed corpus

Files that are truncated, conflicting, cyclic, offset-broken, or structurally odd.

Each fixture should eventually have metadata like:

- detected format
- expected capabilities
- known issues
- known conflicts
- source provenance

## Biggest Risks

### 1. Scope explosion

Mitigation:

- keep v1 read-only
- publish capability matrix
- gate new formats behind clear acceptance criteria

### 2. Normalization becoming magical

Mitigation:

- always preserve derivation and provenance
- surface policy decisions in `report`

### 3. ABI instability

Mitigation:

- keep C ABI narrow
- return serialized outputs instead of rich cross-language structs

### 4. Media-container complexity

Mitigation:

- invest early in ISOBMFF abstractions
- keep QuickTime logic out of the container parser

## Immediate Next Steps

The best order for the next implementation artifacts is:

1. normalized schema draft
2. Rust workspace scaffold
3. core issue and provenance model
4. CLI contract
5. JPEG + TIFF + EXIF spike

If XIFty follows this plan, it has a strong chance of becoming a genuinely modern metadata platform instead of a thin reimplementation of legacy tooling.
