# XIFty Iteration Two Plan

## Summary

Iteration two should strengthen XIFty as a metadata engine, not just add more parsers.

The first iteration proved the architecture on a narrow JPEG/TIFF/EXIF slice. The second iteration should prove that the architecture still holds once metadata comes from multiple containers and multiple namespaces that can agree, disagree, or overlap.

The recommended focus is:

- expand still-image container coverage to `PNG` and `WebP`
- add namespace coverage for `XMP`
- introduce first-class `policy` and `conflict` handling
- extend normalization from single-source derivation to multi-source reconciliation
- preserve the CLI-first, JSON-first surface

This keeps XIFty on the cleanest path toward its long-term vision: a metadata platform core with provenance, normalization, and explainable decisions.

## Why This Iteration

Iteration one answers:

- can XIFty parse cleanly?
- can it preserve provenance?
- can it emit stable JSON?

Iteration two should answer:

- can XIFty reconcile metadata across namespaces?
- can it support more than one still-image container family?
- can it explain why a normalized field was chosen when multiple candidates exist?

That is a much more meaningful architectural proof than simply adding more EXIF tags or immediately jumping to video containers.

## Iteration Goal

Build a **reconciliation-first still-image expansion** that proves XIFty can combine container parsing, namespace decoding, and policy-driven normalization without eroding the clean boundaries established in iteration one.

## Proposed Scope

### New container support

- `PNG`
  - detect PNG
  - walk chunk structure
  - surface textual and metadata-bearing chunks
  - locate `eXIf`, `iTXt`, and `tEXt` payloads relevant to XMP/EXIF

- `WebP`
  - detect RIFF/WebP
  - parse RIFF chunk structure
  - locate `EXIF` and `XMP ` chunks
  - preserve chunk-level provenance

### New metadata namespace support

- `XMP`
  - parse UTF-8 XML packet payloads
  - extract a constrained set of fields needed for normalized output
  - preserve raw packet provenance and decoded field provenance

### New policy and reconciliation support

- introduce `xifty-policy`
- reconcile `EXIF` and `XMP` candidates for the same normalized field
- emit explicit conflicts in `report.conflicts`
- annotate normalized fields with source evidence and notes about selection decisions

### Extended normalized fields

Keep all iteration-one normalized fields and add support for deriving them from either EXIF or XMP where applicable:

- `captured_at`
- `created_at`
- `modified_at`
- `device.make`
- `device.model`
- `dimensions.width`
- `dimensions.height`
- `orientation`
- `location`
- `software`
- `author`
- `copyright`

## Non-Goals

Iteration two should still avoid:

- HEIC / HEIF
- MP4 / MOV
- QuickTime metadata
- ICC parsing
- IPTC parsing beyond what may appear redundantly represented in XMP
- write support
- public language bindings
- stable public FFI
- batch ingestion workflows
- streaming / mmap / object-store input

## Workspace Changes

Add only the crates that are necessary for this slice:

- `xifty-container-png`
- `xifty-container-riff`
- `xifty-meta-xmp`
- `xifty-policy`

Do not add `xifty-container-isobmff`, `xifty-meta-iptc`, `xifty-meta-icc`, or `xifty-meta-quicktime` yet.

## Architectural Boundaries

### `xifty-container-png`

Responsibilities:

- validate PNG signature
- enumerate chunks
- preserve chunk offsets, lengths, and types
- expose metadata-bearing payload locations

Must not:

- decode EXIF semantics
- parse XML
- normalize fields

### `xifty-container-riff`

Responsibilities:

- validate RIFF header
- enumerate RIFF chunks
- expose WebP chunk payloads and offsets

Must not:

- interpret WebP metadata semantics beyond chunk identity
- decode EXIF or XMP content

### `xifty-meta-xmp`

Responsibilities:

- parse XMP packets from bytes
- extract a constrained field set into `MetadataEntry`
- preserve packet provenance and decode notes

Must not:

- decide field precedence against EXIF
- normalize directly to application-facing output

### `xifty-policy`

Responsibilities:

- evaluate competing metadata candidates
- choose normalized winners by explicit rule
- emit reconciliation notes and conflicts
- keep precedence logic out of `xifty-normalize`

Must not:

- parse containers
- decode namespaces

### `xifty-normalize`

Responsibilities in iteration two:

- gather interpreted candidates by semantic meaning
- delegate precedence decisions to `xifty-policy`
- build stable normalized output with provenance

## Policy Direction

Iteration two is where XIFty should stop pretending normalization is just renaming tags.

Recommended initial policy rules:

1. Prefer semantically stronger timestamp fields over generic modification timestamps.
2. Prefer structured GPS coordinates over lossy textual location representations.
3. Prefer EXIF for camera-native capture details when EXIF and XMP agree or XMP is absent.
4. Prefer XMP when it supplies authorial or editorial metadata not present in EXIF.
5. When EXIF and XMP materially disagree on the same field:
   - choose by explicit field rule
   - emit a `Conflict`
   - attach notes to the normalized field describing the decision
6. Never silently collapse conflicting candidates into one value without leaving evidence in `report`.

The first version of `xifty-policy` should be explicit and table-driven, not abstract or plugin-oriented.

## Data Model Adjustments

Iteration one’s model is sufficient, but iteration two should refine it in a few focused ways:

- allow `MetadataEntry` notes to carry decode caveats for XMP field extraction
- make `Conflict` richer:
  - field name
  - competing sources
  - resolution summary
- consider introducing a lightweight internal `CandidateField` type inside normalization/policy code without expanding the public JSON schema yet

Do not redesign the entire core model unless a specific limitation forces it.

## CLI Contract

Keep the current commands:

- `xifty probe <path>`
- `xifty extract <path>`
- `xifty extract <path> --view raw|interpreted|normalized|report`

Iteration two should preserve compatibility with the iteration-one JSON envelope.

Acceptable additive changes:

- richer `raw.containers`
- richer `interpreted.metadata`
- populated `report.conflicts`
- additional normalized fields
- additional provenance notes

Avoid breaking:

- `schema_version` strategy
- top-level envelope shape
- existing normalized field names from iteration one

## Fixture Plan

Add a new checked-in fixture slice with these cases:

- PNG with `eXIf`
- PNG with `iTXt` XMP
- PNG with both EXIF and XMP agreeing
- PNG with EXIF and XMP conflicting on timestamp
- WebP with EXIF chunk
- WebP with XMP chunk
- WebP with both EXIF and XMP
- malformed PNG chunk-length case
- malformed WebP RIFF-length case
- XMP-only image where EXIF is absent

Keep fixtures reproducible from generator scripts where practical.

## Testing Plan

### Unit tests

- PNG chunk traversal
- RIFF chunk traversal
- XMP packet field extraction
- policy decisions for single-field conflicts
- normalization behavior with mixed EXIF + XMP candidates

### Snapshot tests

- `probe` for PNG and WebP
- `extract` for EXIF-only, XMP-only, and mixed-source cases
- `report` snapshots with conflicts present

### Differential tests

Compare XIFty against ExifTool for the subset XIFty claims to support in iteration two:

- timestamps
- make/model
- dimensions
- orientation
- GPS
- software
- author
- copyright

Only compare fields that ExifTool exposes consistently for the specific fixture.

### Fuzzing

Add fuzz targets for:

- PNG chunk walker
- RIFF/WebP chunk walker
- XMP packet parser entry point if the parser is implemented over raw bytes

## Implementation Order

### Phase 1: Container expansion

- add `xifty-container-png`
- add `xifty-container-riff`
- extend `xifty-detect`
- extend CLI orchestration for PNG and WebP routing

### Phase 2: XMP interpretation

- add `xifty-meta-xmp`
- support packet extraction from PNG and WebP payloads
- decode constrained XMP field set into `MetadataEntry`

### Phase 3: Policy and normalization

- add `xifty-policy`
- refactor `xifty-normalize` to resolve candidates through policy
- emit `report.conflicts`

### Phase 4: Verification and hardening

- add new fixtures
- extend snapshot coverage
- add ExifTool differential coverage
- add fuzz targets and smoke runs

## Success Criteria

Iteration two is successful when:

- PNG and WebP files can be probed and extracted through the same CLI contract
- XMP fields can appear in `raw` and `interpreted` output with provenance
- normalized output can be derived from either EXIF or XMP
- conflicting EXIF/XMP candidates result in explicit `report.conflicts`
- policy decisions are testable, deterministic, and isolated from parsing code
- the JSON contract remains backward compatible for iteration-one consumers
- the codebase remains cleanly layered and easier to extend, not harder

## Risks To Watch

- letting XMP parsing sprawl into a general XML framework before the supported field set is stable
- smuggling precedence logic into parser crates
- over-generalizing `xifty-policy` before there are enough real conflict cases
- widening scope into HEIF or video before still-image reconciliation is solid

## Recommended Deliverables

- `ITERATION_TWO_PLAN.md`
- updated workspace manifest with new crates when implementation begins
- a new `ITERATION_TWO_CHECKLIST.md` once the plan is accepted
- a small set of golden conflict fixtures that become permanent regression tests
