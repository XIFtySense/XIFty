# XIFty Iteration Four Checklist

This checklist turns the iteration-four plan into executable work.

## Goal

- [x] Prove bounded media-container metadata support without breaking clean
      architecture
- [x] Keep the CLI and JSON contract backward compatible
- [x] Preserve parser / namespace / policy / normalization separation

## New Crates

- [x] Add `xifty-meta-quicktime`

## Container Support

### Media ISOBMFF

- [x] Detect media-style ISOBMFF containers
- [x] Extend atom parsing for `moov`-relevant media metadata structures
- [x] Preserve atom offsets, sizes, and paths for media files
- [x] Surface recognized media brands honestly
- [x] Report malformed media-atom structures as issues

### MP4 / MOV Routing

- [x] Route MP4 through the existing CLI
- [x] Route MOV through the existing CLI
- [x] Preserve recognized-but-uninterpreted reporting for unsupported media atoms

## Namespace Support

### QuickTime

- [x] Add `xifty-meta-quicktime`
- [x] Decode a constrained textual/timestamp atom subset
- [x] Preserve decode caveats in `MetadataEntry.notes`
- [x] Keep QuickTime interpretation out of `xifty-container-isobmff`

## Normalized Fields

- [x] Add `duration`
- [x] Add `codec.video` when clearly derivable
- [x] Add `codec.audio` when clearly derivable
- [x] Add `created_at` when clearly supported
- [x] Add `modified_at` when clearly supported
- [x] Add `author` or equivalent editorial field when clearly supported

## Policy And Reconciliation

- [x] Keep precedence logic in `xifty-policy`
- [x] Add explicit media-field precedence rules
- [x] Preserve provenance and decision notes for media-normalized fields
- [x] Avoid fabricating still-image fields for media files

## CLI And JSON

- [x] Preserve `xifty probe <path>`
- [x] Preserve `xifty extract <path>`
- [x] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [x] Keep top-level envelope compatibility
- [x] Add only additive output richness

## Fixtures

- [x] MP4 fixture with bounded metadata
- [x] MOV fixture with bounded metadata
- [x] Video-only media fixture
- [x] Audio + video media fixture
- [x] QuickTime metadata-bearing fixture
- [x] Malformed media-atom fixture
- [x] Unsupported-but-recognized media structure fixture
- [x] No-metadata media fixture
- [x] Reproducible generator updates or documented vendored samples

## Tests

- [x] Unit tests for media atom parsing
- [x] Unit tests for duration derivation
- [x] Unit tests for codec derivation
- [x] Unit tests for QuickTime metadata decoding
- [x] Unit tests for media policy decisions
- [x] Snapshot tests for MP4 probe/extract
- [x] Snapshot tests for MOV probe/extract
- [x] Snapshot tests for malformed/unsupported media reports
- [x] ExifTool differential tests for supported iteration-four media fields
- [x] Fuzz target coverage for media-oriented ISOBMFF parsing
- [x] Fuzz target for QuickTime metadata routing/decoding if applicable

## Done Criteria

- [x] MP4 and MOV route through the existing CLI
- [x] Bounded QuickTime metadata appears in interpreted output with provenance
- [x] Media files populate supported normalized fields without breaking existing
      ones
- [x] Partial support is reported honestly in `report`
- [x] No container-specific hacks leak into normalization or metadata crates

## Closeout Note

- The iteration-four fuzz targets are checked in and listed by `cargo fuzz list`.
- Local smoke runs for `isobmff_parser` and `quicktime_metadata` remain blocked by this machine's `cargo-fuzz` toolchain invocation path still reaching a stable `rustc` frontend despite nightly being installed.
