# XIFty Iteration Five Checklist

This checklist turns the iteration-five plan into executable work.

## Goal

- [ ] Prove still-image namespace depth without breaking clean architecture
- [ ] Keep the CLI and JSON contract backward compatible
- [ ] Preserve parser / namespace / policy / normalization separation

## New Crates

- [ ] Add `xifty-meta-icc`
- [ ] Add `xifty-meta-iptc`

## Container Routing

### ICC

- [ ] Route JPEG ICC APP2 payloads
- [ ] Route PNG ICC payloads
- [ ] Route WebP ICC payloads when clearly supported by current container paths
- [ ] Preserve offsets, segment/chunk identity, and provenance
- [ ] Report malformed ICC routing structures as issues

### IPTC

- [ ] Route JPEG IPTC-bearing APP13 / Photoshop payloads
- [ ] Preserve raw dataset/resource provenance
- [ ] Report malformed IPTC routing structures as issues

## Namespace Support

### ICC

- [ ] Add `xifty-meta-icc`
- [ ] Decode a bounded profile subset
- [ ] Preserve decode caveats in `MetadataEntry.notes`
- [ ] Keep ICC interpretation out of container crates

### IPTC

- [ ] Add `xifty-meta-iptc`
- [ ] Decode a bounded IPTC editorial subset
- [ ] Preserve dataset identity in interpreted output
- [ ] Keep IPTC interpretation out of container crates

## Normalized Fields

- [ ] Add `color.profile.name`
- [ ] Add `color.profile.class`
- [ ] Add `color.space`
- [ ] Add `headline` when clearly supported
- [ ] Add `description` when clearly supported
- [ ] Add `keywords` when clearly supported
- [ ] Reuse or refine `author` / `copyright`

## Policy And Reconciliation

- [ ] Keep precedence logic in `xifty-policy`
- [ ] Add explicit XMP / IPTC editorial precedence rules
- [ ] Add explicit ICC-related precedence notes where applicable
- [ ] Preserve provenance and decision notes for normalized fields

## Capability Reporting

- [ ] Add a documented capability matrix or machine-readable capability artifact
- [ ] Ensure the capability statement is test-backed or generation-backed
- [ ] Keep support claims explicit and narrow

## CLI And JSON

- [ ] Preserve `xifty probe <path>`
- [ ] Preserve `xifty extract <path>`
- [ ] Preserve `xifty extract <path> --view raw|interpreted|normalized|report`
- [ ] Keep top-level envelope compatibility
- [ ] Add only additive output richness

## Fixtures

- [ ] JPEG fixture with ICC profile
- [ ] PNG fixture with ICC profile
- [ ] WebP fixture with ICC profile if practical
- [ ] JPEG fixture with IPTC editorial metadata
- [ ] Fixture with overlapping XMP + IPTC values
- [ ] Malformed ICC fixture
- [ ] Malformed IPTC fixture
- [ ] No-ICC / no-IPTC controls
- [ ] Reproducible generator updates or clearly documented sample provenance

## Tests

- [ ] Unit tests for ICC decoding
- [ ] Unit tests for IPTC decoding
- [ ] Unit tests for routing helpers
- [ ] Unit tests for editorial policy decisions
- [ ] Snapshot tests for ICC-bearing fixtures
- [ ] Snapshot tests for IPTC-bearing fixtures
- [ ] Snapshot tests for malformed namespace reports
- [ ] ExifTool differential tests for supported ICC/IPTC fields
- [ ] Capability reporting verification
- [ ] Fuzz target coverage for ICC decoder
- [ ] Fuzz target coverage for IPTC decoder

## Done Criteria

- [ ] ICC and IPTC appear in interpreted output with provenance
- [ ] Bounded color/editorial fields populate normalized output without
      breaking existing ones
- [ ] Partial support is reported honestly in `report`
- [ ] No container-specific hacks leak into normalization or metadata crates
- [ ] Supported capability claims are explicit and maintained
