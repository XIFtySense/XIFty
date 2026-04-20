# #56 — AIFF/AIFC Audio Phase 2: IFF Container Parser

Merged in PR #81 (commit d7aa787b).

## What shipped

Native AIFF and AIFC support end-to-end through the existing probe/extract
surfaces. No FFI shape change — AIFF flows through `xifty_probe_json` and
`xifty_extract_json` and serializes the format as `"aiff"`.

### New crates

**`xifty-container-aiff`** — AIFF/AIFC stream framing.

Validates big-endian IFF `FORM` magic and confirms the form type is `AIFF` or
`AIFC` (non-standard form types raise a `Severity::Info` issue but do not abort
parsing). Walks the 8-byte chunk header sequence (`id: [u8; 4]`, `size: u32 BE`,
data padded to even length) and decodes:

- `COMM` (common chunk) — `num_channels (u16 BE)`, `num_sample_frames (u32 BE)`,
  `sample_size (u16 BE)`, `sample_rate` decoded from 80-bit IEEE 754
  extended-precision big-endian format to `f64`. For AIFC, additionally reads
  the four-byte `compression_type` (e.g. `NONE`, `sowt`, `fl32`).
  `duration_seconds` is derived as `num_sample_frames / sample_rate` when both
  are finite and positive.
- `SSND` — data offset recorded for provenance; bytes not copied.
- `ID3 ` — payload offset and length recorded; IFF pad byte stripped before
  the offset is stored in `id3_payload_offset` / `id3_payload_len`.

The `AiffContainer::id3v2_payload<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]>`
accessor returns the raw ID3v2 payload slice when the chunk is present.

**Current ID3 limitation:** the `ID3 ` chunk offset is located and the
`id3v2_payload()` accessor is wired, but tag decoding is not performed in this
release. There is no ID3v2 decoder crate in the workspace yet. Tag extraction
from embedded AIFF ID3 chunks is a straightforward follow-up once that crate
lands.

### Core changes

- `Format::Aiff` added to the enum in `xifty-core`; serializes as `"aiff"`.
  Both AIFF and AIFC files map to this single variant.
- IFF `FORM`+`AIFF`/`AIFC` magic branch added to `xifty-detect` (bytes 0–3
  must equal `FORM`, bytes 8–11 must equal `AIFF` or `AIFC`).

### CLI wiring

`xifty-cli` gains an `aiff_entries` helper that emits the following from the
decoded `COMM` chunk:

| Tag name | Normalized field |
| --- | --- |
| `AudioSampleRate` | `audio.sample_rate` |
| `AudioChannels` | `audio.channels` |
| `AudioBitDepth` | `audio.bit_depth` |
| `DurationSeconds` | `duration` |

All entries use namespace `"aiff"` and container `"aiff"`. Provenance offsets
point at the `COMM` chunk byte range. The existing `xifty-policy` tag table
routes all four tag names into the standard normalized fields without any new
policy work.

### CAPABILITIES.json

One namespace and one container entry added:

- `aiff` namespace (bounded) — `AudioSampleRate`, `AudioChannels`,
  `AudioBitDepth`, `DurationSeconds`.
- `aiff` container registered with the `aiff` namespace at `bounded`.

### Fixtures

`fixtures/minimal/happy.aiff` — 44100 Hz / 2 ch / 16-bit / 44100 frames
(1 second). COMM + SSND chunks; no `ID3 ` chunk.

`fixtures/minimal/happy.aifc` — same audio parameters, AIFC form type with
`compression_type = NONE`. Exercises the AIFC detection branch and confirms
the container label propagates as `"aiff"` in probe output.

Both fixtures are generated deterministically by `tools/generate_fixtures.py`.

### Issue codes

| Code | Severity | Meaning |
| --- | --- | --- |
| `aiff_non_standard_form` | Info | Form type is not `AIFF` or `AIFC`; parsing continues |
| `aiff_form_size_truncated` | Warning | Declared FORM size exceeds file length; clamped |
| `aiff_chunk_header_truncated` | Warning | Fewer than 8 bytes remain for a chunk header |
| `aiff_chunk_length_invalid` | Warning | Chunk size overflows `usize`; chunk skipped |
| `aiff_comm_missing` | Warning | No `COMM` chunk found; audio fields not emitted |

## CLI example

```bash
cargo run -p xifty-cli -- probe fixtures/minimal/happy.aiff
cargo run -p xifty-cli -- extract fixtures/minimal/happy.aiff --view normalized
cargo run -p xifty-cli -- probe fixtures/minimal/happy.aifc
```

Probe output for `happy.aiff` surfaces three container nodes — the `aiff`
container, a `comm` chunk, and an `ssnd` chunk — with absolute byte offsets.
The normalized extract surfaces `audio.sample_rate=44100`, `audio.channels=2`,
`audio.bit_depth=16`, and `duration=1.0`.

## Divergence from plan

The plan (specs/drafts/56-audio-phase-2-aiff-aifc.md) called for wiring
ID3v2 tag decoding through `xifty-meta-id3v2` in the same PR, mirroring the
MP3 path. That step was deferred: the `id3v2_payload()` accessor is in place
and the chunk offset is tracked, but the tag decode call is absent because
`xifty-meta-id3v2` was not yet available as a stable dependency in this
workspace. The `CAPABILITIES.json` entry for the `aiff` container reflects the
actual shipped surface (`aiff` namespace only, no `id3v2` namespace entry).
