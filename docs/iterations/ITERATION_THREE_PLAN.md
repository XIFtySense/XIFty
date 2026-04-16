# XIFty Iteration Three Plan

## Summary

Iteration three should prove that XIFty's architecture survives the jump from
simple still-image container families into modern structured media containers.

Iterations one and two established:

- typed metadata extraction
- provenance-preserving container parsing
- EXIF + XMP reconciliation
- policy-driven normalization
- stable CLI + JSON output

The next architectural proof should be:

- `ISOBMFF` / `HEIF` container parsing
- image-oriented metadata extraction from modern atom-based files
- richer report fidelity for unsupported or partially understood structures

This is the right next step because it stretches the container architecture in
ways JPEG/PNG/WebP do not, while still staying within the still-image domain.

## Iteration Goal

Build a **modern-container still-image expansion** that proves XIFty can parse,
route, and normalize metadata from `HEIC` / `HEIF`-class files without breaking
the clean boundaries established in earlier iterations.

## Why This Iteration

Iteration three should answer:

- can XIFty handle deeply nested atom-based containers cleanly?
- can it preserve provenance across item/property-style metadata structures?
- can it integrate modern still-image metadata into the same normalized output
  contract without special-casing the whole pipeline?

It should not yet try to answer full video questions. We want the complexity of
modern containers without the blast radius of tracks, timelines, codecs, and
audio/video semantics.

## Proposed Scope

### New container support

- `ISOBMFF`
  - detect `ftyp` / box-based containers
  - walk atom hierarchy with offsets and sizes
  - surface compatible brand information
  - expose metadata-bearing atoms and item/property payloads

- `HEIF` / `HEIC`
  - identify still-image HEIF flavors via brands
  - route EXIF and XMP payloads out of HEIF structures
  - preserve box and item provenance

### New namespace support

- no brand-new namespace crate is required by default
- reuse `xifty-meta-exif` and `xifty-meta-xmp` where possible
- add only the minimum glue needed for extracting those payloads from ISOBMFF

### Report/model refinement

- improve `Conflict` and `Issue` fidelity where modern-container routing is only
  partially supported
- introduce a lightweight unsupported-block reporting pattern for atoms/items
  that are recognized structurally but not yet semantically interpreted

## Non-Goals

Iteration three should avoid:

- full MP4 / MOV track-level media metadata
- audio/video duration and codec normalization
- QuickTime metadata namespace support
- ICC parsing
- IPTC parsing
- write support
- public bindings
- stable FFI
- streaming/mmap redesign

## Workspace Changes

Add the minimum crates needed:

- `xifty-container-isobmff`

Possible but not required:

- `xifty-meta-heif` only if payload-routing glue becomes large enough to justify
  its own crate

Do not add `xifty-meta-quicktime` yet unless a specific still-image requirement
forces it.

## Architectural Boundaries

### `xifty-container-isobmff`

Responsibilities:

- parse atom headers and nesting
- preserve offsets, lengths, atom paths, and brands
- expose payload locations for:
  - EXIF-bearing boxes
  - XMP-bearing boxes
  - item/property structures needed for HEIF still images

Must not:

- interpret EXIF or XMP payload semantics
- normalize fields
- contain policy logic

### Existing metadata crates

`xifty-meta-exif` and `xifty-meta-xmp` remain responsible for namespace
decoding. Iteration three should continue the rule that metadata crates do not
know or care whether bytes came from JPEG APP1, PNG, WebP, or HEIF.

### CLI and normalization

The CLI should continue to orchestrate only.

Normalization and policy should remain container-agnostic wherever possible.
If HEIF-specific selection rules appear, they belong in `xifty-policy`, not in
container or namespace crates.

## Scope Details

### ISOBMFF structural coverage

Support enough structure to:

- parse top-level and nested boxes safely
- identify `ftyp`
- preserve atom tree information in `raw.containers`
- extract still-image metadata payloads routed through HEIF structures

### HEIF still-image metadata coverage

Support enough routing to:

- extract EXIF payloads
- extract XMP payloads when present
- associate payload provenance with relevant box/item structure

### Normalized field coverage

No major field expansion is required. The goal is to populate the existing
normalized field set from HEIF-derived metadata through the same normalization
path already used for EXIF/XMP elsewhere.

## Report And Validation Direction

Iteration three should deepen report honesty:

- report recognized-but-unsupported atom/item structures as informational issues
- report malformed box sizes and nesting failures clearly
- avoid pretending unsupported HEIF substructures are decoded when they are not

If a file is partially analyzable:

- keep successful metadata extraction
- keep structural issues in `report`
- never silently suppress routing failures

## CLI Contract

Keep:

- `xifty probe <path>`
- `xifty extract <path>`
- `xifty extract <path> --view raw|interpreted|normalized|report`

Add only:

- richer container trees for ISOBMFF / HEIF
- richer report output for partial routing support

Do not break:

- top-level envelope shape
- existing field names
- existing view semantics

## Fixture Plan

Add a focused modern-container corpus:

- minimal HEIC with EXIF
- minimal HEIC with XMP
- HEIC with both EXIF and XMP agreeing
- HEIC with conflicting EXIF/XMP values
- malformed box-size HEIC/ISOBMFF case
- unsupported-but-recognized HEIF structure case
- no-metadata HEIC case

Keep fixture generation reproducible where practical, but accept checked-in
golden binaries if generation is disproportionately costly.

## Testing Plan

### Unit tests

- atom header parsing
- nested box traversal
- brand detection
- payload-location extraction for HEIF metadata routes

### Snapshot tests

- `probe` for HEIC / HEIF
- `extract` for EXIF-only, XMP-only, mixed-source, and malformed cases
- `report` snapshots for unsupported/partial-routing cases

### Differential tests

Compare supported normalized fields against ExifTool for the HEIF fixtures.

### Fuzzing

Add:

- `isobmff_parser` fuzz target
- metadata-routing fuzz target if the routing surface is large enough

## Implementation Order

### Phase 1: Container foundation

- add `xifty-container-isobmff`
- extend detection for ISOBMFF brands
- parse atom trees and expose stable structural nodes

### Phase 2: HEIF metadata routing

- identify still-image HEIF brands
- route EXIF payloads into `xifty-meta-exif`
- route XMP payloads into `xifty-meta-xmp`

### Phase 3: Validation and reporting

- report partial support honestly
- add issue codes for malformed atom sizes and unsupported structures

### Phase 4: Verification

- add fixtures
- add snapshots
- add ExifTool comparisons
- add fuzz coverage

## Success Criteria

Iteration three is successful when:

- HEIC / HEIF files route through the existing CLI
- EXIF/XMP payloads extracted from HEIF populate the existing normalized schema
- unsupported modern-container structures are reported honestly
- the codebase remains layered and easier to extend
- no HEIF-specific hacks leak into normalization or metadata crates

## Risks To Watch

- overcommitting to general MP4/MOV semantics too early
- embedding HEIF routing knowledge directly into EXIF/XMP crates
- under-reporting partial support, which would weaken trust in the engine
- creating an atom parser that is too clever to audit

## Recommended Deliverables

- `ITERATION_THREE_PLAN.md`
- `ITERATION_THREE_CHECKLIST.md`
- `xifty-container-isobmff` scaffold when implementation begins
