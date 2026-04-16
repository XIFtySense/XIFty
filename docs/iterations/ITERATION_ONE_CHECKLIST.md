# XIFty Iteration One Checklist

This checklist maps the initial implementation plan to the current repository state.

## Scope

- [x] Architecture-first implementation
- [x] CLI-first delivery
- [x] Still-image foundation only
- [x] JPEG + TIFF + EXIF initial slice
- [x] JSON-only output
- [x] No write support
- [x] No public bindings shipped in this iteration

## Workspace And Boundaries

- [x] `xifty-core`
- [x] `xifty-source`
- [x] `xifty-detect`
- [x] `xifty-container-jpeg`
- [x] `xifty-container-tiff`
- [x] `xifty-meta-exif`
- [x] `xifty-normalize`
- [x] `xifty-validate`
- [x] `xifty-json`
- [x] `xifty-cli`
- [x] `xifty-ffi` placeholder

Boundary checks:

- [x] Container crates expose structure and payload locations
- [x] `xifty-meta-exif` owns EXIF decoding
- [x] `xifty-normalize` owns normalized-field derivation
- [x] `xifty-validate` owns report generation
- [x] CLI is orchestration only

## Data Model

- [x] `Asset`-equivalent analysis envelope via `AnalysisOutput`
- [x] `SourceRef`
- [x] `ContainerNode`
- [x] `MetadataEntry`
- [x] `TypedValue`
- [x] `Provenance`
- [x] `Issue`
- [x] `NormalizedField`

## CLI Contract

- [x] `xifty probe <path>`
- [x] `xifty extract <path>`
- [x] `xifty extract <path> --view raw|interpreted|normalized|report`
- [x] Stable JSON envelope with `schema_version`
- [x] `raw`, `interpreted`, `normalized`, and `report` views
- [x] Non-zero exit for fatal failures
- [x] Validation warnings remain in `report`

## Parsing Scope

- [x] Local files only
- [x] JPEG detection
- [x] JPEG marker walking
- [x] JPEG APP1 EXIF extraction
- [x] TIFF detection
- [x] TIFF little-endian support
- [x] TIFF big-endian support
- [x] IFD0 traversal
- [x] EXIF IFD traversal
- [x] GPS IFD traversal
- [x] Malformed-but-tolerated structures reported as issues
- [x] Unknown/safely decodable tags preserved as raw metadata entries

## Normalized Fields

- [x] `captured_at`
- [x] `device.make`
- [x] `device.model`
- [x] `dimensions.width`
- [x] `dimensions.height`
- [x] `orientation`
- [x] `location` when GPS is present
- [x] `software` when available

## Fixtures And Verification

- [x] Happy-path JPEG fixture
- [x] Happy-path TIFF fixture
- [x] JPEG with GPS fixture
- [x] TIFF with odd/big-endian coverage
- [x] Malformed JPEG APP1 fixture
- [x] Malformed TIFF offset fixture
- [x] No-EXIF JPEG fixture
- [x] No-EXIF TIFF fixture
- [x] Reproducible fixture generator

Testing:

- [x] Unit tests for byte reading
- [x] Unit tests for JPEG marker parsing
- [x] Unit tests for TIFF parsing
- [x] Unit tests for normalization behavior
- [x] Snapshot tests for `probe`
- [x] Snapshot tests for `extract`
- [x] ExifTool differential tests for supported-field subset
- [x] JPEG fuzz target
- [x] TIFF fuzz target
- [x] Nightly `cargo fuzz run jpeg_parser -- -max_total_time=3`
- [x] Nightly `cargo fuzz run tiff_parser -- -max_total_time=3`

## Notes

- The iteration intentionally stops short of XMP, ICC, PNG, WebP, HEIC, MOV, QuickTime, write support, and a stable public FFI surface.
- The `xifty-ffi` crate exists only to preserve architectural direction and buildability.
