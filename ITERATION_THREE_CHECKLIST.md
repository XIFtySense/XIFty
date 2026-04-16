# XIFty Iteration Three Checklist

This checklist turns the iteration-three plan into executable work.

## Goal

- [ ] Prove modern still-image container support without breaking clean
      architecture
- [ ] Keep the CLI and JSON contract backward compatible
- [ ] Preserve parser / metadata / policy / normalization separation

## New Crates

- [ ] Add `xifty-container-isobmff`

## Container Support

### ISOBMFF

- [ ] Detect box-based ISOBMFF containers
- [ ] Parse atom headers and nesting safely
- [ ] Preserve atom offsets, sizes, and paths
- [ ] Surface compatible brand information
- [ ] Report malformed box-size conditions as issues

### HEIF / HEIC Routing

- [ ] Recognize still-image HEIF brands
- [ ] Surface EXIF payload locations
- [ ] Surface XMP payload locations
- [ ] Report recognized-but-unsupported HEIF structures honestly

## Namespace Routing

- [ ] Route HEIF EXIF payloads into `xifty-meta-exif`
- [ ] Route HEIF XMP payloads into `xifty-meta-xmp`
- [ ] Keep namespace crates container-agnostic

## CLI And JSON

- [ ] Preserve `xifty probe <path>`
- [ ] Preserve `xifty extract <path>`
- [ ] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [ ] Keep top-level envelope compatibility
- [ ] Add only additive container/report richness

## Fixtures

- [ ] HEIC with EXIF fixture
- [ ] HEIC with XMP fixture
- [ ] HEIC with agreeing EXIF + XMP fixture
- [ ] HEIC with conflicting EXIF + XMP fixture
- [ ] Malformed ISOBMFF / HEIF fixture
- [ ] Unsupported-but-recognized HEIF structure fixture
- [ ] No-metadata HEIC fixture

## Tests

- [ ] Unit tests for atom parsing
- [ ] Unit tests for nested box traversal
- [ ] Unit tests for brand detection
- [ ] Unit tests for HEIF metadata routing
- [ ] Snapshot tests for HEIC probe/extract
- [ ] Snapshot tests for malformed/unsupported reports
- [ ] ExifTool differential tests for supported HEIF fields
- [ ] Fuzz target for ISOBMFF parser
- [ ] Fuzz target for HEIF metadata routing if applicable

## Done Criteria

- [ ] HEIC / HEIF routes through the existing CLI
- [ ] HEIF-derived EXIF/XMP feeds the existing normalized schema
- [ ] Partial support is reported honestly in `report`
- [ ] No container-specific hacks leak into normalization or metadata crates
