# XIFty State Of The Project

## Summary

XIFty is in a strong architectural position.

The project has already proven the core thesis from the vision:

- metadata is not treated as a flat tag dump
- container parsing is kept separate from metadata interpretation
- normalization is policy-driven
- provenance, conflicts, and issues are first-class output concerns
- the CLI and JSON envelope remain stable as capability expands

The main remaining gap is not architecture. It is breadth.

## What Is Proven

### 1. The core architectural rule is working

The most important design constraint in the project is that container parsing
and metadata interpretation remain separate.

That rule is no longer theoretical. It has been exercised across:

- JPEG / TIFF
- PNG / WebP
- HEIC / HEIF / ISOBMFF

The same high-level extraction pipeline now survives segment-based, chunk-based,
and atom/item-based container families without collapsing into container-specific
logic in normalization or policy.

### 2. The four-view model is real

The vision called for:

- `raw`
- `interpreted`
- `normalized`
- `report`

That shape is now implemented and stable in the CLI contract.

This matters because it proves XIFty is becoming a metadata engine with a clear
mental model, not just a parser that happens to emit JSON.

### 3. Reconciliation is real

Iteration two established that XIFty can reconcile overlapping EXIF and XMP
metadata across multiple still-image containers.

That means the project has already moved beyond "can we decode bytes?" into the
more valuable product question: "can we derive stable, explainable fields from
messy real-world metadata?"

### 4. Modern-container support is no longer speculative

Iteration three proved that XIFty can handle modern still-image containers
without abandoning its clean boundaries.

In particular, the project now supports:

- ISOBMFF / HEIF brand detection
- box-tree parsing with offsets and paths
- HEIF metadata routing for EXIF and XMP
- primary-item dimension derivation from HEIF property structures
- differential validation against a real-world HEIC sample

That is an important milestone because modern container complexity is where many
clean designs begin to erode. XIFty held up.

## What Is Still Missing

### Breadth gaps relative to the vision

The original MVP and long-term direction still include major areas that are not
implemented yet:

- MP4 / MOV container support
- QuickTime metadata interpretation
- IPTC support
- ICC support
- broader video/audio-oriented normalized fields
- public bindings for Python / Node / Swift
- stable `C ABI`

These are roadmap gaps, not architecture gaps.

### Verification gaps

The project is in a good state, but a few validation areas are still less
mature than the eventual vision:

- HEIF oracle-backed differential coverage currently relies on a real vendored
  sample because ExifTool does not surface metadata from the synthetic HEIC
  corpus
- the newest HEIF fuzz targets are checked in, but local smoke execution is
  still blocked by this machine's nightly `cargo-fuzz` resolution
- capability reporting is implicit in docs and tests, not yet exposed as a
  first-class machine-readable contract

## Assessment Against The Vision

### Where XIFty is ahead

XIFty is ahead of where many projects would be at this stage in:

- architectural discipline
- separation of concerns
- honesty about supported capabilities
- test-backed iteration closure
- conflict/provenance/report modeling

That is exactly where an architecture-first project should be ahead.

### Where XIFty is behind

XIFty is behind only in intentionally deferred capability breadth.

That includes namespace coverage, container breadth, bindings, and public
embedding surfaces.

This is acceptable and even desirable at this stage because the project is
trying to become a durable metadata platform, not a shallow checklist of file
formats.

## Roadmap Implication

The next iteration should not redesign the core.

The architecture has now been proven across the first three slices. The next
best move is to extend capability while preserving the current boundaries.

That suggests the next iteration should likely focus on one of two directions:

- `MP4 / MOV + QuickTime metadata`
- `ICC / IPTC` namespace expansion for still-image fidelity

The stronger roadmap choice is probably `MP4 / MOV + QuickTime metadata`.

Why:

- it closes the largest remaining MVP-format gap in the vision
- it reuses the ISOBMFF work from iteration three
- it tests whether the current architecture can scale from modern still-image
  containers into broader media containers without blending container and
  namespace concerns
- it opens the path toward `duration`, `codec.video`, `codec.audio`, and other
  normalized fields already anticipated in the architecture plan

## Recommended Next-Step Framing

Iteration four should probably be framed as:

**Expand from modern still-image containers into bounded media-container
metadata, without taking on full playback/timeline semantics.**

That keeps XIFty on the path laid out in the original vision while preserving
the discipline that has made the first three iterations successful.
