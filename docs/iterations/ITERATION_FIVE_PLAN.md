# XIFty Iteration Five Plan

## Summary

Iteration five should deepen still-image metadata fidelity rather than expand
container families again.

The first four iterations established:

- architecture-first crate boundaries
- stable CLI and JSON output
- EXIF and XMP extraction plus reconciliation
- provenance-preserving parsing across JPEG, TIFF, PNG, WebP, HEIF, MP4, and
  MOV
- bounded media metadata support without collapsing clean architecture
- vendor-specific depth for Sony and Apple paths

The strongest next move is to close a major remaining still-image gap:

- `ICC` color-profile metadata
- `IPTC` editorial/news metadata
- clearer capability reporting for what XIFty actually supports

The goal is not to broaden containers again. The goal is to make XIFty's
still-image understanding materially more complete while preserving the current
layered design.

## Iteration Goal

Build a **still-image fidelity slice** that:

- adds constrained `ICC` and `IPTC` namespace support
- routes those namespaces through the existing JPEG / TIFF / PNG / WebP / HEIF
  paths where applicable
- introduces the first normalized color-profile and editorial fields
- makes supported capability boundaries more explicit in-project

## Why This Iteration

This is the strongest roadmap move now because:

- it closes major MVP namespace gaps still called out in the vision
- it strengthens the still-image side of the product before broader media depth
- it gives XIFty better application-facing answers for authoring, copyright,
  profile, and image-intent workflows
- it tests whether namespace breadth can keep growing without smearing concerns
  across parsing, policy, and normalization

## Proposed Scope

### New namespace support

Add new crates:

- `xifty-meta-icc`
- `xifty-meta-iptc`

Responsibilities:

- `xifty-meta-icc`
  - decode a constrained but useful profile subset
  - surface profile/device/class/color-space fields with provenance
  - preserve decode caveats honestly in `notes`

- `xifty-meta-iptc`
  - decode a constrained IPTC IIM subset first
  - surface common editorial fields with typed values
  - preserve raw dataset identity and provenance

Do not push ICC or IPTC interpretation into container crates.

### Container routing changes

Do not add a new container crate.

Extend existing container paths only where needed to route:

- JPEG APP13 / Photoshop-style IPTC payloads
- ICC APP2 payloads in JPEG
- ICC-bearing chunks in PNG
- ICC-bearing chunks in WebP where the container already surfaces them
- HEIF/ISOBMFF routes only if the current structures already expose profile-like
  payloads cheaply and honestly

Iteration five should prefer honest partial routing over speculative breadth.

### New normalized-field coverage

Add a constrained still-image subset such as:

- `color.profile.name`
- `color.profile.class`
- `color.space`
- `author`
- `headline`
- `description`
- `keywords`
- `copyright`

Where fields overlap with existing XMP-derived normalized fields, precedence
must remain in `xifty-policy`.

## Non-Goals

Iteration five should avoid:

- full ICC profile semantic expansion
- color-management transforms or rendering simulation
- full Photoshop resource parsing beyond what is required for IPTC routing
- broad legacy IPTC edge-case support all at once
- write support
- bindings
- stable FFI
- full capability negotiation API over FFI
- broad media/timeline expansion

## Workspace Changes

Add:

- `xifty-meta-icc`
- `xifty-meta-iptc`

Do not add new container crates.

## Architectural Boundaries

### Container crates

Responsibilities in this iteration:

- find and expose ICC/IPTC payload locations
- preserve offsets, segment/chunk identity, and routing provenance
- report malformed payload containers honestly

Must not:

- interpret ICC profile semantics
- interpret IPTC dataset semantics
- choose normalization precedence

### `xifty-meta-icc`

Responsibilities:

- decode bounded ICC profile header / tag-level information
- surface typed metadata entries and decode caveats
- keep profile interpretation independent of container of origin

Must not:

- choose normalization precedence
- perform color transforms
- contain container-specific routing logic

### `xifty-meta-iptc`

Responsibilities:

- decode bounded IPTC datasets
- surface typed metadata entries with raw dataset ids and provenance
- keep namespace semantics independent of JPEG/Photoshop/etc. routing

Must not:

- normalize fields directly
- contain parser logic for JPEG segment traversal

### `xifty-policy`

Responsibilities:

- resolve overlap among EXIF / XMP / IPTC / ICC-derived candidates
- keep precedence decisions explicit and explainable
- preserve notes when a more-editable namespace wins over a lower-level source

### `xifty-normalize`

Responsibilities:

- derive application-facing color/editorial fields from interpreted metadata
- preserve provenance and policy decisions
- avoid inventing fields when namespaces are absent or ambiguous

## Scope Details

### ICC coverage

Support a narrow, useful subset such as:

- profile name / description
- profile class
- device manufacturer / model when cleanly derivable
- PCS / color-space identity

This should be enough to prove the architecture and expose meaningful profile
facts without turning iteration five into a full color-management project.

### IPTC coverage

Support a narrow editorial subset such as:

- object name / headline
- caption / description
- keywords
- byline / creator
- copyright / rights-style fields

The exact supported dataset list should be explicit in the checklist and tests.

### Capability reporting

Iteration five should also introduce a modest capability-reporting layer, such
as:

- a documented capability matrix in-repo
- or a machine-readable capabilities artifact generated by tests/tools

The goal is to make support claims more explicit than today without designing a
premature public API around capabilities.

## Report And Validation Direction

Iteration five should preserve the honesty standard from earlier iterations:

- report recognized-but-uninterpreted ICC tags or IPTC datasets when useful
- report malformed ICC/IPTC payloads clearly
- avoid fabricating profile/editorial fields when the underlying source is
  incomplete
- preserve partial success when some namespaces decode and others do not

## CLI Contract

Keep:

- `xifty probe <path>`
- `xifty extract <path>`
- `xifty extract <path> --view raw|interpreted|normalized|report`

Add only:

- richer interpreted output for ICC and IPTC
- additive normalized still-image fidelity fields
- richer reporting for partial namespace support

Do not break:

- top-level envelope shape
- existing field names
- existing view semantics

## Fixture Plan

Add a focused still-image corpus:

- JPEG with ICC profile
- PNG with ICC profile
- WebP with ICC profile if practical
- JPEG with IPTC editorial metadata
- JPEG with overlapping XMP + IPTC editorial values
- malformed ICC payload case
- malformed IPTC payload case
- no-ICC / no-IPTC controls

Prefer reproducible synthetic fixtures where practical.
Use optional local corpus only when a realistic sample is necessary and cannot
be responsibly checked in.

## Testing Plan

### Unit tests

- ICC header / selected tag decoding
- IPTC dataset decoding
- routing helpers for JPEG APP2 / APP13 and PNG/WebP payloads
- policy precedence for overlapping editorial fields

### Snapshot tests

- interpreted / normalized output for ICC-bearing still-image fixtures
- interpreted / normalized output for IPTC-bearing fixtures
- report snapshots for malformed or partial namespace cases

### Differential tests

Compare the supported ICC/IPTC subset against ExifTool for the oracle-backed
fixtures.

### Capability verification

Add a verification path that ensures the documented capability matrix stays in
sync with the implemented/tests-backed subset.

### Fuzzing

Add:

- `icc_decoder` fuzz target
- `iptc_decoder` fuzz target
- routing fuzz targets only if the container extraction surface materially grows

## Implementation Order

### Phase 1: Namespace scaffolding

- add `xifty-meta-icc`
- add `xifty-meta-iptc`
- define the bounded supported field/dataset lists

### Phase 2: Container routing

- route ICC/IPTC payloads out of existing container crates / CLI orchestration
- preserve provenance and issue reporting

### Phase 3: Normalization and policy

- add bounded color/editorial normalized fields
- resolve XMP / IPTC overlap in `xifty-policy`

### Phase 4: Verification and capability reporting

- add fixtures, snapshots, and ExifTool differentials
- introduce explicit capability reporting/documentation checks

## Done Criteria

Iteration five is complete when:

- `ICC` and `IPTC` appear in interpreted output with provenance
- bounded color/editorial fields populate normalized output where supported
- overlap with existing XMP paths is handled by policy instead of ad hoc rules
- malformed ICC/IPTC payloads produce clear report issues instead of crashes
- the repo has a more explicit, maintained statement of supported capabilities
- crate boundaries remain clean and container crates still do only routing and
  structure
