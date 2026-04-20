<!-- loswf:plan -->
# Plan #41: HEIF/HEIC — wire ICC and IPTC namespace extraction

## Problem
`CAPABILITIES.json` marks `heif.icc` and `heif.iptc` as `not_yet_supported` (see
`/Users/k/Projects/XIFty/CAPABILITIES.json` lines 60–71). HEIF parsing in
`xifty-container-isobmff` recognizes Exif and XMP payloads (item-based and
top-level `mime`/`xml `), but does not expose ICC profiles carried in
`iprp/ipco/colr` boxes nor IPTC carried as metadata items (`infe` with item
type `iptc` or with mime item `application/x-iptc` — content type
`application/x-iptc`). Consequently, the `Format::Heif` arm in
`/Users/k/Projects/XIFty/crates/xifty-cli/src/lib.rs` lines 443–452 never
emits `icc` or `iptc` metadata entries for HEIC files. This blocks
`color_space` and editorial-field normalization on iPhone HEIC.

## Approach
Mirror the TIFF pattern (`icc_payload`/`iptc_payload` accessors in
`/Users/k/Projects/XIFty/crates/xifty-container-tiff/src/lib.rs` lines 227–234)
and the HEIF Exif-item routing pattern already in place
(`/Users/k/Projects/XIFty/crates/xifty-container-isobmff/src/lib.rs` lines
1329–1340 `metadata_item_kind`, lines 1315–1324 `payloads_from_items`, lines
390–399 top-level `Exif` box handling).

1. Extend `IsobmffContainer` with `icc_payloads()` and `iptc_payloads()`
   accessors following the existing `exif_payloads()` / `xmp_payloads()`
   accessors (lib.rs lines 48–56).
2. In `parse_children` (lib.rs lines 298–440), match `b"colr"` and walk
   `iprp/ipco` to capture ICC profiles; colr may be an item property, not only
   a top-level meta child. Read the 4-byte `colour_type` header and only emit
   an `icc` payload for types `prof`/`rICC` (restricted ICC) — skip `nclx` /
   `nclc` (coded primaries, no ICC bytes).
3. Extend `metadata_item_kind` (lib.rs line 1329) to return `Some("iptc")`
   when `info.item_type == b"iptc"` or when `item_type == b"mime"` with
   `content_type == "application/x-iptc"`. The existing `payloads_from_items`
   routing (lib.rs lines 1315–1324) will then emit IPTC `IsobmffPayload`
   entries.
4. Wire both payload iterators into the `Format::Heif` arm by extending
   `isobmff_entries` in `/Users/k/Projects/XIFty/crates/xifty-cli/src/lib.rs`
   lines 692–769, following the PNG `icc_payloads`/`iptc_payloads` decode
   pattern (lib.rs lines 290–360) — call `decode_icc_payload` with
   `path: "heif_icc"` and `decode_iptc_payload` with `path: "heif_iptc"`,
   emitting the standard `icc_decode_empty` / `iptc_decode_empty` issues when
   the decoder returns an empty result.
5. Promote `heif.icc` and `heif.iptc` from `not_yet_supported` to `bounded`
   in `/Users/k/Projects/XIFty/CAPABILITIES.json` (match the TIFF status).
6. Add fixtures `fixtures/minimal/icc.heic` and `fixtures/minimal/iptc.heic`
   and corresponding snapshot + interpreted tests.

## Files to touch
- `/Users/k/Projects/XIFty/crates/xifty-container-isobmff/src/lib.rs` — add
  `icc`/`iptc` payload kinds, `colr` box parser, `icc_payloads()` +
  `iptc_payloads()` accessors, and extend `metadata_item_kind` for IPTC items.
- `/Users/k/Projects/XIFty/crates/xifty-cli/src/lib.rs` — extend
  `isobmff_entries` (line 692) to iterate the new ICC/IPTC payloads and run
  `decode_icc_payload` / `decode_iptc_payload` with `heif_icc` / `heif_iptc`
  paths; emit parity issues to mirror TIFF/PNG.
- `/Users/k/Projects/XIFty/CAPABILITIES.json` — promote `heif.icc` and
  `heif.iptc` from `not_yet_supported` to `bounded`.
- `/Users/k/Projects/XIFty/crates/xifty-cli/tests/cli_contract.rs` — add
  snapshot tests for `icc.heic` and `iptc.heic` normalized views, plus
  interpreted-view assertions paralleling `icc_png_interpreted_view_includes_icc_fields`
  (line 533) and `iptc_jpeg_normalization_includes_editorial_fields` (line 362).

## New files
- `/Users/k/Projects/XIFty/fixtures/minimal/icc.heic` — minimal HEIC with a
  `colr` box carrying a `prof` ICC profile (reuse the embedded ICC bytes
  used by `icc.jpg`/`icc.png` so existing `supported_tags` decode cleanly).
- `/Users/k/Projects/XIFty/fixtures/minimal/iptc.heic` — minimal HEIC with
  an `infe` item of type `iptc` pointing via `iloc` to an IIM payload
  (reuse the IIM bytes used by `iptc.jpg`/`iptc.png` for snapshot parity).
- `/Users/k/Projects/XIFty/crates/xifty-cli/tests/snapshots/cli_contract__extract_icc_heic_normalized.snap`
- `/Users/k/Projects/XIFty/crates/xifty-cli/tests/snapshots/cli_contract__extract_iptc_heic_normalized.snap`

## Step-by-step
1. In `xifty-container-isobmff/src/lib.rs`, add `icc_payloads()` and
   `iptc_payloads()` methods on `IsobmffContainer` filtering `payloads` by
   `kind == "icc"` / `kind == "iptc"` — verifiable by compilation + new unit
   tests.
2. In the same file's `parse_children` match (line 298), add a `b"colr"` arm
   that reads the 4-byte `colour_type`; when the type is `prof` or `rICC`,
   push an `IsobmffPayload { kind: "icc", .. }` pointing at the ICC bytes
   (data_offset + 4, length = end - data_offset - 4). Ignore `nclx`/`nclc`
   silently (they carry no ICC bytes). Verifiable with a unit test building
   an `iprp/ipco/colr[prof]` stub.
3. Extend `metadata_item_kind` (line 1329): return `Some("iptc")` for
   `item_type == b"iptc"` or `item_type == b"mime"` with
   `content_type == "application/x-iptc"`. Verifiable with a unit test
   constructing an `infe` item of type `iptc` with a matching `iloc`.
4. In `xifty-cli/src/lib.rs` inside `isobmff_entries` (line 692), add two
   loops after the xmp loop — one iterating `container.icc_payloads()` and
   decoding via `decode_icc_payload(... path: "heif_icc")`, one iterating
   `container.iptc_payloads()` and decoding via
   `decode_iptc_payload(... path: "heif_iptc")`. Emit
   `icc_decode_empty` / `iptc_decode_empty` issues on empty decode, mirroring
   the TIFF block (lines 218–256) and PNG block (lines 290–360).
5. Add fixtures `fixtures/minimal/icc.heic` and `fixtures/minimal/iptc.heic`
   generated via a small helper test (or committed bytes) that wraps an
   existing ICC profile in a HEIC `meta/iprp/ipco/colr` box and an IIM
   payload in a HEIC metadata item. Verifiable by
   `cargo run -p xifty-cli -- extract fixtures/minimal/icc.heic --view raw`
   returning `icc` namespace entries.
6. Add CLI snapshot tests `extract_icc_heic_normalized` and
   `extract_iptc_heic_normalized` in
   `crates/xifty-cli/tests/cli_contract.rs`, mirroring the existing
   `extract_icc_tiff_normalized` (line 308) and `extract_iptc_tiff_normalized`
   (line 316) tests. Also add an `icc_heic_interpreted_view_includes_icc_fields`
   assertion paralleling the PNG variant at line 533. Verifiable via
   `cargo insta review` showing expected fields.
7. Update `/Users/k/Projects/XIFty/CAPABILITIES.json` lines 65–66: change
   `"icc": "not_yet_supported"` to `"icc": "bounded"` and
   `"iptc": "not_yet_supported"` to `"iptc": "bounded"`. Verifiable via
   `python3 tools/generate_capabilities.py --check` succeeding.
8. Confirm no regression in the existing HEIC snapshots
   (`extract_mixed_heic_normalized`, `extract_real_exif_heic_normalized`,
   `malformed_heic_report`, `unsupported_heic_report`,
   `probe_happy_heic`) — these fixtures carry no ICC/IPTC so their output
   must be byte-identical. Verifiable with `cargo test -p xifty-cli`.

## Tests
- `xifty-container-isobmff` unit tests: add `parses_colr_prof_as_icc_payload`,
  `ignores_colr_nclx`, `routes_iptc_item_payload` in the `tests` module at
  `/Users/k/Projects/XIFty/crates/xifty-container-isobmff/src/lib.rs` line
  1415.
- `xifty-cli` snapshot tests: `extract_snapshot_icc_heic_normalized`,
  `extract_snapshot_iptc_heic_normalized`.
- `xifty-cli` interpreted-value tests:
  `icc_heic_interpreted_view_includes_icc_fields`,
  `iptc_heic_normalization_includes_editorial_fields`.
- Capabilities self-test: extend `_self_test` in
  `/Users/k/Projects/XIFty/tools/generate_capabilities.py` line 160 to
  assert `{"exif", "xmp", "icc", "iptc"}.issubset(observed["heif"])`.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`
- `python3 tools/generate_capabilities.py --check`

## Risks
- `colr` can appear as an ItemProperty inside `iprp/ipco` (indexed and
  associated to the primary item via `ipma`); unlike top-level boxes, it may
  not contribute metadata for non-primary items. Initial implementation should
  emit ICC for any `colr[prof]`/`colr[rICC]` encountered — tightening to
  primary-item-only association is a follow-up if needed.
- IPTC-as-HEIC-item framing is less standardized than Exif. Two plausible
  encodings: `infe.item_type == b"iptc"` (raw IIM bytes as item payload) and
  `infe.item_type == b"mime"` with `content_type == "application/x-iptc"`.
  Supporting both keeps us aligned with common encoders.
- Fixture authoring: generating valid `iloc`-based items requires matching
  `iloc` offsets to the file's `mdat`/`idat`. Consider building fixtures
  programmatically in a `build.rs`-style test helper rather than hand-hex
  bytes (follow the pattern in `real_exif.heic` which is a real file).
- `unsupported.heic` and `malformed_box.heic` snapshots may shift if the new
  `colr` arm emits issues on malformed `colr` boxes — guard with conservative
  truncation checks to avoid perturbing them.
- Capabilities status: chose `bounded` (not `supported`) to match TIFF's
  status and acknowledge that we bound to the ICC/IPTC tag subset in
  `CAPABILITIES.json` `namespaces.icc.supported_tags` /
  `namespaces.iptc.supported_tags`.

