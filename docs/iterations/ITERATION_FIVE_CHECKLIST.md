# XIFty Iteration Five Checklist

This checklist turns the iteration-five plan into executable work.

## Goal

- [x] Prove still-image namespace depth without breaking clean architecture
- [x] Keep the CLI and JSON contract backward compatible
- [x] Preserve parser / namespace / policy / normalization separation

## New Crates

- [x] Add `xifty-meta-icc`
- [x] Add `xifty-meta-iptc`

## Container Routing

### ICC

- [x] Route JPEG ICC APP2 payloads
- [x] Route PNG ICC payloads
- [x] Route WebP ICC payloads when clearly supported by current container paths
- [x] Preserve offsets, segment/chunk identity, and provenance
- [x] Report malformed ICC routing structures as issues

### IPTC

- [x] Route JPEG IPTC-bearing APP13 / Photoshop payloads
- [x] Preserve raw dataset/resource provenance
- [x] Report malformed IPTC routing structures as issues

## Namespace Support

### ICC

- [x] Add `xifty-meta-icc`
- [x] Decode a bounded profile subset
- [x] Preserve decode caveats in `MetadataEntry.notes`
- [x] Keep ICC interpretation out of container crates

### IPTC

- [x] Add `xifty-meta-iptc`
- [x] Decode a bounded IPTC editorial subset
- [x] Preserve dataset identity in interpreted output
- [x] Keep IPTC interpretation out of container crates

## Normalized Fields

- [x] Add `color.profile.name`
- [x] Add `color.profile.class`
- [x] Add `color.space`
- [x] Add `headline` when clearly supported
- [x] Add `description` when clearly supported
- [x] Add `keywords` when clearly supported
- [x] Reuse or refine `author` / `copyright`

## Policy And Reconciliation

- [x] Keep precedence logic in `xifty-policy`
- [x] Add explicit XMP / IPTC editorial precedence rules
- [x] Add explicit ICC-related precedence notes where applicable
- [x] Preserve provenance and decision notes for normalized fields

## Capability Reporting

- [x] Add a documented capability matrix or machine-readable capability artifact
- [x] Ensure the capability statement is test-backed or generation-backed
- [x] Keep support claims explicit and narrow

## CLI And JSON

- [x] Preserve `xifty probe <path>`
- [x] Preserve `xifty extract <path>`
- [x] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [x] Keep top-level envelope compatibility
- [x] Add only additive output richness

## Fixtures

- [x] JPEG fixture with ICC profile
- [x] PNG fixture with ICC profile
- [x] WebP fixture with ICC profile if practical
- [x] JPEG fixture with IPTC editorial metadata
- [x] Fixture with overlapping XMP + IPTC values
- [x] Malformed ICC fixture
- [x] Malformed IPTC fixture
- [x] No-ICC / no-IPTC controls
- [x] Reproducible generator updates or clearly documented sample provenance

## Tests

- [x] Unit tests for ICC decoding
- [x] Unit tests for IPTC decoding
- [x] Unit tests for routing helpers
- [x] Unit tests for editorial policy decisions
- [x] Snapshot tests for ICC-bearing fixtures
- [x] Snapshot tests for IPTC-bearing fixtures
- [x] Snapshot tests for malformed namespace reports
- [x] ExifTool differential tests for supported ICC/IPTC fields
- [x] Capability reporting verification
- [x] Fuzz target coverage for ICC decoder
- [x] Fuzz target coverage for IPTC decoder

## Done Criteria

- [x] ICC and IPTC appear in interpreted output with provenance
- [x] Bounded color/editorial fields populate normalized output without
      breaking existing ones
- [x] Partial support is reported honestly in `report`
- [x] No container-specific hacks leak into normalization or metadata crates
- [x] Supported capability claims are explicit and maintained
