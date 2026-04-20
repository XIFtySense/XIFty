# DNG Support (#61)

DNG is now a first-class format in XIFty, distinct from TIFF in all output
surfaces while reusing the TIFF container parser internally.

## Detection

`xifty-detect` branches on the `DNGVersion` tag (`0xC612`) found in IFD0 of a
TIFF-shaped byte stream. Any TIFF-magic file (`II*\0` or `MM\0*`) that carries
tag `0xC612` in IFD0 is routed to `Format::Dng`; all others fall through to
`Format::Tiff` as before.

The IFD0 scan is defensive: a malformed or truncated IFD offset returns `false`
and detection degrades to `Format::Tiff` rather than surfacing a parse error.

## Parsing and extraction

No changes were made to `xifty-container-tiff`. DNG files walk IFD0 and ExifIFD
through the existing `parse_tiff` path without modification.

`xifty-cli` now routes `Format::Dng` through a shared `tiff_extract` helper that
was factored out of the former inline `Format::Tiff` branch. Both `Format::Tiff`
and `Format::Dng` call `tiff_extract` with their respective container label:

```rust
Format::Tiff => tiff_extract(&source, "tiff")?,
Format::Dng  => tiff_extract(&source, "dng")?,
```

Namespace decoders (EXIF, XMP, ICC, IPTC, Apple, Sony vendor paths) are all
exercised through this shared helper, with the `container_label` threaded
through so provenance in every `MetadataEntry` identifies the source as `"dng"`.

## Output

`probe` and `extract` output identifies DNG files with:

```json
{
  "input": {
    "container": "dng",
    "detected_format": "dng",
    "path": "happy.dng"
  }
}
```

This is a behavioral surface change visible in `input.detected_format`. No C
ABI shape change occurred; `xifty-ffi` tests pass without contract edits.

## Capabilities

`CAPABILITIES.json` now includes:

```json
"dng": {
  "namespaces": {
    "exif": "bounded",
    "xmp": "bounded",
    "icc": "bounded",
    "iptc": "bounded"
  }
}
```

The namespace matrix and `bounded` status mirror the `tiff` entry.

## Fixture

`fixtures/minimal/happy.dng` is a synthesized minimal DNG produced by
`tools/generate_fixtures.py` (`build_tiff(dng=True)`). It carries the three
DNG marker tags alongside the standard TIFF baseline fields:

| Tag | ID | Value |
|---|---|---|
| DNGVersion | `0xC612` | `1.4.0.0` (inline 4-byte BYTE array) |
| DNGBackwardVersion | `0xC613` | `1.4.0.0` (inline 4-byte BYTE array) |
| UniqueCameraModel | `0xC614` | `"XIFtyCam DNG Test"` (ASCII, offset) |

Standard TIFF baseline fields (Make, Model, Orientation, Software, ExifIFD
pointer) are also present so normalized extraction exercises the full field
set.

## Tests

Two new snapshot tests in `crates/xifty-cli/tests/cli_contract.rs`:

- `probe_snapshot_happy_dng` — verifies `Format::Dng` detection and probe JSON
- `extract_snapshot_happy_dng_normalized` — verifies normalized extraction
  through the shared TIFF path with `container = "dng"`

Two new unit tests in `crates/xifty-detect/src/lib.rs`:

- `detects_dng_when_dng_version_present` — asserts `detect` returns
  `Format::Dng` for a byte buffer with tag `0xC612` in IFD0 and `Format::Tiff`
  for a plain TIFF
- `malformed_tiff_ifd_offset_falls_back_to_tiff` — asserts that a byte buffer
  whose IFD0 offset points past the end of the buffer does not return `Dng` and
  does not panic
