# #55 — FLAC Audio Phase 1: Native Block Parser + Vorbis Comments

Merged in PR #79 (commit 4b4dfa73).

## What shipped

Native FLAC support end-to-end through the existing probe/extract surfaces.
No FFI shape change — FLAC flows through `xifty_probe_json` and
`xifty_extract_json` and serializes the format as `"flac"`.

### New crates

**`xifty-container-flac`** — FLAC stream framing.

Validates the `fLaC` 4-byte magic, then walks the chain of
`METADATA_BLOCK_HEADER` records (last-flag bit | 7-bit type | 24-bit BE
length). Exposes typed views for three block types:

- `StreamInfo` — 20-byte fixed layout decoded into `sample_rate_hz`,
  `channels`, `bits_per_sample`, `total_samples`, and a computed
  `duration_seconds` (`total_samples / sample_rate_hz` when sample rate is
  non-zero).
- `VorbisCommentBlock` — offsets only; the payload bytes are forwarded to
  `xifty-meta-vorbis-comment`.
- `FlacPicture` — MIME type, description, `width`, `height`, `color_depth`,
  `colors_used`, and block offsets. Image bytes are not copied.

**`xifty-meta-vorbis-comment`** — container-agnostic Vorbis-comment decoder.

Parses the Vorbis-comment framing (all lengths little-endian u32, distinct
from the FLAC block framing that uses big-endian lengths) and emits
`MetadataEntry` values under the `vorbis_comment` namespace. Well-known keys
map to canonical tag names:

```
TITLE  ARTIST  ALBUM  DATE  GENRE  TRACKNUMBER
ALBUMARTIST  COMPOSER  COMMENT
```

Unknown keys are emitted with the raw upper-cased key as both `tag_id` and
`tag_name`. The vendor string is emitted as a `Vendor` entry. The caller
supplies the container name (`"flac"` today; OGG will pass `"ogg"`), so the
crate carries no hard-coded container dependency.

### Core changes

- `Format::Flac` added to the enum in `xifty-core`; serializes as `"flac"`.
- `fLaC` magic branch added to `xifty-detect`.
- `audio.bit_depth` added to `xifty-policy` normalization rules (alongside the
  existing `audio.channels` and `audio.sample_rate` rules).

### CLI wiring

`xifty-cli` gains a `flac_entries` helper that emits:

| Tag name | Normalized field |
| --- | --- |
| `AudioSampleRate` | `audio.sample_rate` |
| `AudioChannels` | `audio.channels` |
| `AudioBitDepth` | `audio.bit_depth` |
| `DurationSeconds` | `duration` |
| `PictureMimeType` | — |
| `PictureWidth` | — |
| `PictureHeight` | — |

Vorbis-comment entries from `xifty-meta-vorbis-comment` flow through the
existing normalization map without additional policy changes.

### CAPABILITIES.json

Two namespaces added:

- `vorbis_comment` (bounded) — TITLE, ARTIST, ALBUM, DATE, GENRE,
  TRACKNUMBER, ALBUMARTIST, COMPOSER, COMMENT.
- `flac` (bounded) — AudioSampleRate, AudioChannels, AudioBitDepth,
  DurationSeconds, PictureMimeType, PictureWidth, PictureHeight.

Container `flac` registered with both namespaces at `bounded`.

`audio.bit_depth` added to `normalized_fields`.

### Fixtures

`fixtures/minimal/happy.flac` — 226-byte synthetic file (STREAMINFO at 44100
Hz / 2 ch / 16 bps / 44100 total samples, VORBIS_COMMENT with TITLE/ARTIST/
ALBUM, PICTURE block with a 1×1 PNG payload). Real audio frames are not
required; the metadata block chain is independent of the frame payload.

## CLI example

```bash
cargo run -p xifty-cli -- probe fixtures/minimal/happy.flac
cargo run -p xifty-cli -- extract fixtures/minimal/happy.flac --view normalized
```

The normalized view surfaces `audio.sample_rate=44100`, `audio.channels=2`,
`audio.bit_depth=16`, and `duration=1.0`.

## Divergence from plan

The plan assumed the MP3 sibling (#53) would already be merged and instructed
mirroring `mp3_entries` / `mp3_scalar_entry`. #53 was still open at merge
time, so `flac_entries` was modeled on the `media_scalar_entry` pattern from
`isobmff_entries` instead. Semantics match the plan's intent.

The plan stated `audio.bit_depth` was already present in policy. It was not;
the rule was added in this PR in the same style as the existing
`audio.channels` / `audio.sample_rate` rules.
