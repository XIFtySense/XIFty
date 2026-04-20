# #58 — M4A as ISOBMFF Variant with iTunes ilst Decoding

Merged in PR #82 (commit ac4e9e1d).

## What shipped

M4A is now a first-class `Format::M4a` variant, distinguished from `Format::Mp4`
at the `ftyp` major-brand level. The ISOBMFF container crate surfaces the full
iTunes `ilst` atom tree under a new `kind: "itunes"` payload, and a new
`xifty-meta-itunes` crate interprets those atoms into normalized `MetadataEntry`
values in the `itunes` namespace. No FFI ABI change — only the JSON
`detected_format` surface gains the new value `"m4a"`.

### New crate

**`xifty-meta-itunes`** — iTunes `ilst` atom interpreter.

Decodes the `data` sub-box wrapper common to all iTunes atoms:

```text
size(4) | 'data' | version(1) | flags(3) | locale(4) | payload...
```

The 24-bit flags field is the type indicator (`0x01` UTF-8, `0x00` binary,
`0x0D` JPEG, `0x0E` PNG, `0x15` signed big-endian integer).

Public surface:

```rust
pub struct ItunesPayload<'a> {
    pub key: &'a str,       // four-char atom tag, e.g. "©nam", "trkn", "covr"
    pub bytes: &'a [u8],    // inner data sub-box bytes starting at size field
    pub container: &'a str, // e.g. "m4a"
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: ItunesPayload<'_>) -> Vec<MetadataEntry>;
```

Atom coverage in the `itunes` namespace:

| Atom | Tag name | Value type |
| --- | --- | --- |
| `©nam` | `Title` | `TypedValue::String` |
| `©ART` | `Artist` | `TypedValue::String` |
| `©alb` | `Album` | `TypedValue::String` |
| `©day` | `Year` | `TypedValue::String` |
| `©gen` | `Genre` | `TypedValue::String` |
| `©cmt` | `Comment` | `TypedValue::String` |
| `©wrt` | `Composer` | `TypedValue::String` |
| `©lyr` | `Lyrics` | `TypedValue::String` |
| `©too` | `Encoder` | `TypedValue::String` |
| `aART` | `AlbumArtist` | `TypedValue::String` |
| `trkn` | `TrackNumber` | `TypedValue::String` (`"n/N"` pair) |
| `disk` | `DiskNumber` | `TypedValue::String` (`"n/N"` pair) |
| `cpil` | `Compilation` | `TypedValue::String` |
| `tmpo` | `BeatsPerMinute` | `TypedValue::Integer` |
| `covr` | `CoverArt` | `TypedValue::Bytes` (JPEG or PNG, no decoding) |

Unknown atom keys produce an empty `Vec`.

### Core changes

- `Format::M4a` added to the enum in `xifty-core`; serializes as `"m4a"`.
- `xifty-detect` branches on `ftyp` major brand before the existing
  `is_mp4_brand` check. Major brands `M4A `, `M4B `, and `M4P ` route to
  `Format::M4a`. The `M4A ` entry was removed from the generic MP4 brand list
  so these files no longer fall through to `Format::Mp4`.

### Container changes

`xifty-container-isobmff` extended to emit the wider iTunes atom set under a
new `kind: "itunes"` payload (in addition to the existing `kind: "quicktime"`
payloads for `©ART`, `©too`, and `©nam`). The dual-emit strategy keeps all
existing MP4/MOV snapshots stable while making the full iTunes atom set
available to `xifty-meta-itunes`.

The new `itunes_payloads()` accessor on `IsobmffContainer` returns
`IsobmffPayload` entries with `kind == "itunes"` for the atoms listed in the
table above.

### CLI wiring

`xifty-cli` gains a `Format::M4a` arm in both `probe_source` and
`extract_source`, delegating to the existing `parse_isobmff` helper. A new
`itunes_entries` helper iterates `container.itunes_payloads()` and calls
`decode_itunes_payload` for each, appending the results to the entry list
alongside the `isobmff_entries` output.

Probe reports `container: "isobmff"` and `detected_format: "m4a"`. Extract
surfaces iTunes tags in the `itunes` namespace alongside the existing QuickTime
entries.

### CAPABILITIES.json

Two entries added:

- `itunes` namespace (bounded) — `Title`, `Artist`, `Album`, `AlbumArtist`,
  `Year`, `Genre`, `Comment`, `Composer`, `Lyrics`, `Encoder`, `TrackNumber`,
  `DiskNumber`, `Compilation`, `BeatsPerMinute`, `CoverArt`.
- `m4a` container registered with both `quicktime: bounded` and
  `itunes: bounded`.

### Fixtures and snapshots

`fixtures/minimal/happy.m4a` — synthetic file with `ftyp` major brand `M4A `,
an audio-only track, and a rich `ilst` covering text, integer-pair, boolean,
tempo, and cover-art atoms. Generated deterministically by
`tools/generate_fixtures.py`.

Three insta snapshots under `crates/xifty-cli/tests/snapshots/`:

- `cli_contract__probe_happy_m4a.snap`
- `cli_contract__extract_happy_m4a_normalized.snap`
- `cli_contract__extract_happy_m4a_interpreted.snap`

## CLI example

```bash
cargo run -p xifty-cli -- probe fixtures/minimal/happy.m4a
cargo run -p xifty-cli -- extract fixtures/minimal/happy.m4a --view normalized
cargo run -p xifty-cli -- extract fixtures/minimal/happy.m4a --view interpreted
```

Probe output reports `container: "isobmff"` and `detected_format: "m4a"`.
Normalized extract surfaces `itunes.Title`, `itunes.Artist`, `itunes.Album`,
`itunes.TrackNumber`, and related fields alongside the ISOBMFF audio facts.

## Back-compat notes

- All existing MP4 and MOV snapshots are unchanged. The `©ART`, `©too`, and
  `©nam` atoms continue to emit `kind: "quicktime"` payloads in addition to
  the new `kind: "itunes"` payloads.
- No C ABI shape change. The only JSON surface change is that files with M4A
  `ftyp` brands now report `detected_format: "m4a"` instead of
  `detected_format: "mp4"`.
