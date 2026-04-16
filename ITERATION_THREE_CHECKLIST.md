# XIFty Iteration Three Checklist

This checklist turns the iteration-three plan into executable work.

## Goal

- [x] Prove modern still-image container support without breaking clean
      architecture
- [x] Keep the CLI and JSON contract backward compatible
- [x] Preserve parser / metadata / policy / normalization separation

## New Crates

- [x] Add `xifty-container-isobmff`

## Container Support

### ISOBMFF

- [x] Detect box-based ISOBMFF containers
- [x] Parse atom headers and nesting safely
- [x] Preserve atom offsets, sizes, and paths
- [x] Surface compatible brand information
- [x] Report malformed box-size conditions as issues

### HEIF / HEIC Routing

- [x] Recognize still-image HEIF brands
- [x] Surface EXIF payload locations
- [x] Surface XMP payload locations
- [x] Report recognized-but-unsupported HEIF structures honestly

## Namespace Routing

- [x] Route HEIF EXIF payloads into `xifty-meta-exif`
- [x] Route HEIF XMP payloads into `xifty-meta-xmp`
- [x] Keep namespace crates container-agnostic

## CLI And JSON

- [x] Preserve `xifty probe <path>`
- [x] Preserve `xifty extract <path>`
- [x] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [x] Keep top-level envelope compatibility
- [x] Add only additive container/report richness

## Fixtures

- [x] HEIC with EXIF fixture
- [x] HEIC with XMP fixture
- [x] HEIC with agreeing EXIF + XMP fixture
- [x] HEIC with conflicting EXIF + XMP fixture
- [x] Malformed ISOBMFF / HEIF fixture
- [x] Unsupported-but-recognized HEIF structure fixture
- [x] No-metadata HEIC fixture

## Tests

- [x] Unit tests for atom parsing
- [x] Unit tests for nested box traversal
- [x] Unit tests for brand detection
- [x] Unit tests for HEIF metadata routing
- [x] Snapshot tests for HEIC probe/extract
- [x] Snapshot tests for malformed/unsupported reports
- [x] ExifTool differential tests for supported HEIF fields
- [x] Fuzz target for ISOBMFF parser
- [x] Fuzz target for HEIF metadata routing if applicable

## Done Criteria

- [x] HEIC / HEIF routes through the existing CLI
- [x] HEIF-derived EXIF/XMP feeds the existing normalized schema
- [x] Partial support is reported honestly in `report`
- [x] No container-specific hacks leak into normalization or metadata crates

## Current Gap

- Synthetic HEIC fixtures remain useful for deterministic snapshots, but the oracle-backed differential coverage now relies on the vendored real-world `real_exif.heic` sample because ExifTool does not surface metadata from the synthetic HEIC corpus.
- The ISOBMFF and HEIF metadata-routing fuzz targets are both checked in. A local smoke run is still blocked by this machine's `cargo-fuzz` nightly-toolchain resolution, so that environment issue remains separate from checklist completeness.
