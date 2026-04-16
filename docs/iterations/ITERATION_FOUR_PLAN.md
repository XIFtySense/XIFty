# XIFty Iteration Four Plan

## Summary

Iteration four should extend XIFty from modern still-image containers into
bounded media-container metadata.

The first three iterations established:

- architecture-first crate boundaries
- stable CLI and JSON output
- EXIF and XMP extraction plus reconciliation
- provenance-preserving parsing across JPEG, TIFF, PNG, WebP, and HEIF
- ISOBMFF parsing that can already support item-based still-image metadata

The next iteration should build on that foundation by adding:

- `MP4` / `MOV` container coverage through the existing ISOBMFF work
- constrained `QuickTime` metadata interpretation
- initial media-oriented normalized fields such as duration and codec metadata

The goal is not to become a full video analyzer yet. The goal is to prove that
XIFty can move from still-image metadata into bounded media metadata without
breaking the architecture that has been established so far.

## Iteration Goal

Build a **bounded media-container metadata slice** that:

- routes `MP4` / `MOV` through the existing CLI and JSON contract
- extracts constrained QuickTime/ISOBMFF metadata cleanly
- introduces the first media-oriented normalized fields
- keeps container parsing, namespace interpretation, policy, and normalization
  separate

## Why This Iteration

This is the strongest roadmap move now because:

- it closes the largest remaining MVP format gap in the original vision
- it reuses iteration-three ISOBMFF work instead of starting a new structural
  branch
- it tests whether the current architecture can scale from still-image metadata
  into broader media containers
- it creates a path toward eventual `duration`, `codec.video`, and
  `codec.audio` normalized fields that the architecture plan already anticipated

## Proposed Scope

### New container behavior

Extend `xifty-container-isobmff` to support bounded media-oriented parsing for:

- `MP4`
- `MOV`
- compatible QuickTime-family brands when clearly identified as media files

Structural coverage should include:

- top-level atoms relevant to media metadata
- `moov` tree traversal
- track and media header discovery where needed for metadata
- atom-path and offset preservation for media-oriented structures

### New namespace support

Add a new crate:

- `xifty-meta-quicktime`

Responsibilities:

- interpret constrained QuickTime / iTunes-style metadata atoms
- decode a limited set of user-facing metadata fields
- remain container-agnostic with respect to where the atom payload came from

Do not fold QuickTime interpretation into `xifty-container-isobmff`.

### New normalized-field coverage

Iteration four should add only a constrained media subset:

- `duration`
- `codec.video` when clearly derivable
- `codec.audio` when clearly derivable
- `created_at` / `modified_at` when QuickTime metadata supports them
- `author` or equivalent editorial fields when cleanly supported

Still-image fields such as EXIF-style `device.make` should not be invented for
media containers that do not actually contain them.

## Non-Goals

Iteration four should avoid:

- full playback/timeline semantics
- frame-by-frame media analysis
- bitrate/rate-control deep inspection
- subtitle, chapter, or edit-list semantics beyond what is necessary for
  duration honesty
- broad audio/video stream introspection beyond bounded normalized fields
- write support
- public bindings
- stable FFI
- ICC/IPTC support

## Workspace Changes

Add:

- `xifty-meta-quicktime`

Do not add a new container crate.

Iteration four should deliberately reuse `xifty-container-isobmff` so the
architecture is tested under broader responsibility without fragmenting the
container layer.

## Architectural Boundaries

### `xifty-container-isobmff`

Responsibilities in this iteration:

- detect media-style ISOBMFF files
- parse `moov` and relevant atom trees
- expose payload locations and structural facts
- surface brands, track/media headers, and metadata-bearing atom positions

Must not:

- interpret QuickTime metadata semantics
- normalize fields
- contain precedence rules

### `xifty-meta-quicktime`

Responsibilities:

- decode selected metadata atoms such as textual tags and creation/modification
  timestamps
- expose typed metadata entries with provenance
- describe decode caveats honestly in notes

Must not:

- walk the whole atom tree
- choose normalization precedence
- derive final app-facing fields directly

### `xifty-normalize`

Responsibilities:

- derive bounded media normalized fields from interpreted metadata
- preserve provenance and decision notes
- avoid silently merging container facts and namespace values without policy

### `xifty-policy`

Responsibilities:

- decide precedence between QuickTime metadata and other candidate sources when
  they overlap
- express when media-container facts should win over editable metadata atoms for
  fields such as duration or codecs

## Scope Details

### MP4 / MOV structural coverage

Support enough structure to:

- identify media-style brands
- parse `moov`
- parse track-level structures needed for duration and codec derivation
- preserve a meaningful raw container tree for media files

### QuickTime metadata coverage

Support a narrow but meaningful subset, such as:

- creation/modification timestamps where available
- title/author-like textual metadata where cleanly represented
- user-data or metadata-list atoms that can be decoded without broad schema
  ambiguity

The exact atom subset should be explicit in the checklist and tests.

### Normalized field coverage

Fields to target in iteration four:

- `duration`
- `codec.video`
- `codec.audio`
- `created_at`
- `modified_at`
- `author` when cleanly supported

Media containers should continue to emit `raw`, `interpreted`, and `report`
even when normalized coverage is partial.

## Report And Validation Direction

Iteration four should continue the honesty standard established in iteration
three:

- report recognized-but-uninterpreted media atoms as informational issues when
  useful
- report malformed track/header sizes clearly
- avoid fabricating codec or duration values when the structure is incomplete
- preserve partial success when some media metadata is readable and some is not

## CLI Contract

Keep:

- `xifty probe <path>`
- `xifty extract <path>`
- `xifty extract <path> --view raw|interpreted|normalized|report`

Add only:

- richer ISOBMFF container trees for media files
- bounded normalized media fields
- richer reporting for partial media coverage

Do not break:

- top-level envelope shape
- existing field names
- existing view semantics

## Fixture Plan

Add a focused media-container corpus:

- minimal `MP4` with bounded metadata
- minimal `MOV` with bounded metadata
- MP4 with video-only coverage
- MP4 or MOV with audio + video coverage
- metadata-bearing QuickTime atom case
- malformed media-atom case
- unsupported-but-recognized media structure case
- no-metadata media case

Use reproducible generation where practical.

If generation becomes too costly or too synthetic for oracle comparison, accept
a small number of vendored real-world fixtures with documented provenance and
licensing.

## Testing Plan

### Unit tests

- media atom parsing in `xifty-container-isobmff`
- duration derivation from bounded structures
- codec extraction from selected sample entries or headers
- QuickTime metadata decoding
- policy decisions for media-normalized fields

### Snapshot tests

- `probe` for MP4 and MOV
- `extract` for bounded media metadata cases
- malformed and unsupported media-report snapshots

### Differential tests

Use ExifTool differentials for the supported bounded field subset:

- duration when available
- width/height if derived for video tracks and included in normalized output
- creation/modification dates when supported
- selected textual QuickTime metadata where stable enough

Only compare the fields XIFty explicitly claims to support.

### Fuzzing

Add or extend fuzz targets for:

- media-oriented ISOBMFF parsing paths
- QuickTime metadata routing/decoding entry points

## Implementation Order

### Phase 1: Media-aware ISOBMFF routing

- extend detection/brand handling for media-family files
- extend `xifty-container-isobmff` to parse media-relevant atom trees
- preserve paths, offsets, and recognized-but-uninterpreted reporting

### Phase 2: QuickTime metadata interpretation

- add `xifty-meta-quicktime`
- decode a constrained atom subset
- route metadata into the existing analysis pipeline

### Phase 3: Media normalization

- add `duration`
- add `codec.video`
- add `codec.audio`
- add limited timestamp/editorial field normalization where justified

### Phase 4: Hardening

- differential tests
- fuzz-target additions
- fixture documentation
- checklist closeout and roadmap reassessment

## Success Criteria

Iteration four is successful when:

- MP4 and MOV route through the existing CLI
- media files produce useful `raw`, `interpreted`, `normalized`, and `report`
  output without changing the envelope
- QuickTime metadata interpretation is isolated from container parsing
- duration and codec-like normalized fields are present for the supported slice
- differential tests pass for the supported media field subset
- the codebase remains cleanly layered and does not regress into
  container-specific normalization logic

## Recommended Framing

Iteration four should be communicated as:

**Bounded media metadata support on top of the proven ISOBMFF foundation, not a
full video-analysis engine.**
