# XIFty Iteration Two Checklist

This checklist turns the iteration-two plan into executable work.

## Goal

- [ ] Prove still-image metadata reconciliation across multiple containers and namespaces
- [ ] Keep the CLI and JSON contract backward compatible
- [ ] Preserve clean separation between parsing, interpretation, policy, and normalization

## New Crates

- [x] Add `xifty-container-png`
- [x] Add `xifty-container-riff`
- [x] Add `xifty-meta-xmp`
- [x] Add `xifty-policy`

## Container Support

### PNG

- [x] Detect PNG in `xifty-detect`
- [x] Validate PNG signature
- [x] Walk PNG chunks with offset provenance
- [x] Surface `eXIf` payload locations
- [x] Surface XMP-bearing `iTXt` / `tEXt` payload locations
- [x] Report malformed chunk-length conditions as issues

### WebP / RIFF

- [x] Detect RIFF / WebP in `xifty-detect`
- [x] Validate RIFF header and size fields
- [x] Walk RIFF chunks with offset provenance
- [x] Surface `EXIF` chunk payload locations
- [x] Surface `XMP ` chunk payload locations
- [x] Report malformed RIFF-length conditions as issues

## Namespace Support

### XMP

- [x] Parse XMP packet bytes
- [x] Extract constrained normalized-field subset
- [x] Preserve packet provenance
- [x] Preserve decode caveats in `MetadataEntry.notes`

Supported XMP-derived fields for iteration two:

- [x] `captured_at`
- [x] `created_at`
- [x] `modified_at`
- [x] `device.make`
- [x] `device.model`
- [x] `dimensions.width`
- [x] `dimensions.height`
- [x] `orientation`
- [x] `location`
- [x] `software`
- [x] `author`
- [x] `copyright`

## Policy And Reconciliation

- [x] Introduce candidate-based normalization flow
- [x] Keep precedence logic in `xifty-policy`
- [x] Prefer EXIF for camera-native capture details by explicit rule
- [x] Prefer XMP for editorial / authorial fields by explicit rule
- [x] Emit `report.conflicts` for material EXIF/XMP disagreements
- [x] Annotate normalized fields with decision notes and source evidence

## CLI And JSON

- [x] Preserve `xifty probe <path>`
- [x] Preserve `xifty extract <path>`
- [x] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [x] Keep the top-level JSON envelope stable
- [x] Add only additive output changes

## Fixtures

- [x] PNG with EXIF fixture
- [x] PNG with XMP fixture
- [x] PNG with agreeing EXIF + XMP fixture
- [x] PNG with conflicting EXIF + XMP fixture
- [x] WebP with EXIF fixture
- [x] WebP with XMP fixture
- [x] WebP with mixed EXIF + XMP fixture
- [x] Malformed PNG fixture
- [x] Malformed WebP fixture
- [x] XMP-only fixture
- [x] Reproducible generator updates

## Tests

- [x] Unit tests for PNG parsing
- [x] Unit tests for RIFF parsing
- [x] Unit tests for XMP extraction
- [x] Unit tests for policy decisions
- [x] Unit tests for mixed-source normalization
- [x] Snapshot tests for PNG probe/extract
- [x] Snapshot tests for WebP probe/extract
- [x] Snapshot tests for conflict reports
- [x] ExifTool differential tests for supported iteration-two fields
- [x] Fuzz target for PNG chunk walker
- [x] Fuzz target for RIFF / WebP chunk walker
- [x] Fuzz target for XMP parser entry point if applicable

## Done Criteria

- [x] PNG and WebP route through the same CLI successfully
- [x] XMP appears in raw and interpreted output with provenance
- [x] Normalized output can reconcile EXIF and XMP candidates
- [x] Conflicts are explicit and test-covered
- [ ] The codebase remains cleanly layered
