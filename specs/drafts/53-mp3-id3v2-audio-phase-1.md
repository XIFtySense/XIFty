<!-- loswf:plan -->
# Plan #53: Audio phase 1 — MP3 (ID3v2) + duration/sample-rate/channels/bit-depth

## Problem
XIFty currently enumerates no audio container. `Format` in `crates/xifty-core/src/lib.rs:8-16` covers only image and ISOBMFF video formats, and `detect()` in `crates/xifty-detect/src/lib.rs:4-35` has no branch for MPEG audio frames or ID3 headers. `CAPABILITIES.json` advertises `audio.channels` and `audio.sample_rate` (lines 120-121) but only lists them as derived from ISOBMFF tracks — there is no path to extract them from a bare MP3 file, and the normalize surface does not yet own `audio.bit_depth`. Parent issue #5 (kstore creator wedge) flags audio (MP3 first) as the P0 gap; this sub-issue delivers phase 1: a native MP3 container parser, an ID3v2 namespace decoder, detection, CLI wiring, and the new `audio.bit_depth` normalized field slot.

## Approach
Mirror the existing container/metadata split: add `xifty-container-id3` (container-only — ID3v2 tag framing, MPEG audio frame header walk, Xing/VBRI detection for VBR duration) and `xifty-meta-id3v2` (metadata-only — interpret ID3v2 frames into `MetadataEntry` values under the `id3v2` namespace). Surface MPEG-derived audio properties (`AudioSampleRate`, `AudioChannels`, `AudioBitDepth`, `DurationSeconds`, plus a new `MpegLayer`) using the same `media_scalar_entry`-style emission the ISOBMFF path uses at `crates/xifty-cli/src/lib.rs:807-865`. Container-file reference: `crates/xifty-container-riff/src/lib.rs:41` (parser entry with `parse`/`parse_bytes` split) and `crates/xifty-container-riff/src/lib.rs:171-205` (synthetic-fixture tests). Namespace crate reference: `crates/xifty-meta-iptc` (small, payload-scoped decoder). Extend `Format`/detector, add an `Mp3` arm in `probe_source` (`crates/xifty-cli/src/lib.rs:40-69`) and `extract_source` (`crates/xifty-cli/src/lib.rs:99-461`), add `audio.bit_depth` to the policy map in `crates/xifty-policy/src/lib.rs:218-231`, and snapshot-test probe/extract for a synthetic MP3 fixture.

## Files to touch
- `Cargo.toml` — register `xifty-container-id3` and `xifty-meta-id3v2` workspace members (after line 18, grouping with other `xifty-container-*` / `xifty-meta-*` entries).
- `crates/xifty-core/src/lib.rs` — add `Format::Mp3` to the enum at lines 8-16 and the matching `"mp3"` arm to `as_str` at lines 19-29.
- `crates/xifty-detect/src/lib.rs` — add an MP3 branch in `detect` (lines 4-35) that matches either an ID3v2 header (`"ID3"` + version byte) or an MPEG sync word (0xFFE bits) and skips past ID3v1 trailers where trivial; add a corresponding test in the `detects_formats` block at lines 119-163.
- `crates/xifty-cli/src/lib.rs` — add `Format::Mp3` arms to `probe_source` (lines 40-69) and `extract_source` (lines 99-461); emit synthesized audio `MetadataEntry`s via a helper analogous to `media_scalar_entry` (lines 919-940); route ID3v2 payload bytes through `xifty_meta_id3v2::decode_payload`.
- `crates/xifty-policy/src/lib.rs` — add an `audio.bit_depth` entry after the `audio.sample_rate` mapping at lines 225-231, using `&["AudioBitDepth"]` and `NamespacePreference::QuickTimeFirst` (consistent with peer audio fields).
- `crates/xifty-normalize/src/lib.rs` — no new derivation needed; the policy crate carries the field. Add a test that `audio.bit_depth` surfaces from an `AudioBitDepth` entry (append to the existing test module).
- `CAPABILITIES.json` — add `"mp3"` to `containers` with namespaces `{ "id3v2": "bounded" }`; add `"id3v2"` to top-level `namespaces` (status `bounded`, `supported_tags: ["TIT2", "TPE1", "TALB"]`); append `"audio.bit_depth"` to `normalized_fields` (lines 90-122).
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshot tests for `happy.mp3` (mirror the pattern at lines 155-160 and 914-928), plus an assertion that `audio.bit_depth` surfaces and a Xing-absent VBR fixture emits the `mp3_vbr_duration_unknown` issue.
- `tools/generate_fixtures.py` — add a `build_mp3` helper (CBR + VBR-with-Xing + VBR-without-Xing variants) and register `happy.mp3`, `vbr_xing.mp3`, `vbr_no_xing.mp3` alongside the MP4/MOV generators at lines 753-759.

## New files
- `crates/xifty-container-id3/Cargo.toml` — mirror `crates/xifty-container-png/Cargo.toml`; depend only on `xifty-core` and `xifty-source`.
- `crates/xifty-container-id3/src/lib.rs` — `parse`/`parse_bytes` entry points returning an `Id3Container` with: detected ID3v2 tag slice (version, flags, extracted raw frame bytes), first MPEG frame header (version/layer/bitrate/sample-rate/channels), derived `duration_seconds` (CBR: file-size / bitrate; VBR: Xing/VBRI frame-count × samples-per-frame ÷ sample-rate), a `bit_depth` constant (16 per MPEG audio convention) surfaced as `Option<u32>`, and a `Vec<Issue>` carrying `mp3_vbr_duration_unknown` when VBR lacks Xing/VBRI. Provide an `id3v2_payload()` accessor returning the raw frames slice plus absolute offset. Synthetic-fixture unit tests mirror `crates/xifty-container-riff/src/lib.rs:171-205`.
- `crates/xifty-meta-id3v2/Cargo.toml` — mirror `crates/xifty-meta-iptc/Cargo.toml` style; depend on `xifty-core` only.
- `crates/xifty-meta-id3v2/src/lib.rs` — `decode_payload(Id3v2Payload)` returning `Vec<MetadataEntry>`; handle ID3v2.3/2.4 frame framing (10-byte header, size decoding that differs between 2.3 big-endian and 2.4 syncsafe), and map `TIT2`/`TPE1`/`TALB` (plus best-effort `TCON`, `TYER`/`TDRC`) into `namespace: "id3v2"` entries with `tag_id`/`tag_name` equal to the 4-char frame id. Unit tests build synthetic ID3v2.3 and 2.4 payloads and assert the three required round-trips.
- `fixtures/minimal/happy.mp3` — generated by `tools/generate_fixtures.py` (CBR 128 kbps, 44.1 kHz, stereo, with TIT2/TPE1/TALB ID3v2.3 header).
- `fixtures/minimal/vbr_xing.mp3`, `fixtures/minimal/vbr_no_xing.mp3` — generated VBR fixtures exercising both Xing-present and Xing-absent paths.

## Step-by-step
1. Add `Format::Mp3` to `crates/xifty-core/src/lib.rs:8-29` — `cargo build -p xifty-core` compiles and `as_str()` returns `"mp3"`.
2. Extend `crates/xifty-detect/src/lib.rs` with an ID3/MPEG-sync branch and a new test case — `cargo test -p xifty-detect` passes for a raw sync-word fixture and an ID3v2-prefixed fixture.
3. Scaffold `crates/xifty-container-id3` with `parse_bytes` walking ID3v2 header → first MPEG frame header → optional Xing/VBRI → duration computation, with `Issue` emission for VBR-without-Xing — `cargo test -p xifty-container-id3` covers CBR, VBR-with-Xing, and VBR-without-Xing synthetic bytes.
4. Scaffold `crates/xifty-meta-id3v2` with `decode_payload` for ID3v2.3 and ID3v2.4 frame layouts — `cargo test -p xifty-meta-id3v2` asserts TIT2/TPE1/TALB round-trip for both versions, including a syncsafe-size test.
5. Register both crates in the root `Cargo.toml` (alphabetical within the container/meta groups) — `cargo metadata --format-version 1 | grep xifty-container-id3` succeeds.
6. Wire `Format::Mp3` into `crates/xifty-cli/src/lib.rs` probe + extract paths, emitting `AudioSampleRate`/`AudioChannels`/`AudioBitDepth`/`DurationSeconds`/`MpegLayer` entries plus ID3v2 entries, and propagating container issues — `cargo build -p xifty-cli` passes and `happy.mp3` extract contains all five derived entries.
7. Add `audio.bit_depth` to `crates/xifty-policy/src/lib.rs:218-231` and a unit test in `crates/xifty-normalize/src/lib.rs` — `cargo test -p xifty-policy -p xifty-normalize` passes.
8. Update `CAPABILITIES.json` (add `mp3` container, `id3v2` namespace, `audio.bit_depth` normalized field) and run `python3 tools/generate_capabilities.py` if it is authoritative; otherwise edit by hand — capability JSON validates.
9. Extend `tools/generate_fixtures.py` with CBR/VBR MP3 builders and regenerate `fixtures/minimal/*.mp3` — fixtures exist on disk.
10. Add probe + extract insta snapshots and assertions in `crates/xifty-cli/tests/cli_contract.rs` (mirror the MP4 pattern) covering `happy.mp3`, `vbr_xing.mp3`, `vbr_no_xing.mp3` — `cargo test -p xifty-cli` passes; `cargo insta review` accepts the three new snapshots.

## Tests
- `crates/xifty-container-id3/src/lib.rs` — in-module unit tests: CBR duration math, Xing VBR duration math, VBR-no-Xing `Issue`, ID3v2 payload slicing.
- `crates/xifty-meta-id3v2/src/lib.rs` — in-module unit tests: ID3v2.3 frames (TIT2/TPE1/TALB), ID3v2.4 syncsafe sizes.
- `crates/xifty-detect/src/lib.rs` — extend `detects_formats` to include MP3 (with and without ID3 prefix).
- `crates/xifty-normalize/src/lib.rs` — test that `AudioBitDepth` entry surfaces to `audio.bit_depth`.
- `crates/xifty-cli/tests/cli_contract.rs` — new `probe_happy_mp3` and `extract_happy_mp3_normalized` snapshots plus `mp3_normalization_includes_audio_fields` exact-value assertions (duration, sample_rate=44100, channels=2, bit_depth=16) and a `vbr_no_xing` issue assertion.

## Validation
From `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

(No FFI shape change is required — MP3 flows through the existing `xifty_probe_json` / `xifty_extract_json` surface.)

## Risks
- VBR duration accuracy relies on Xing/VBRI; spec-legal VBR files without either header are parse-complete but duration-unknown. The accepted scope (per issue) is to emit an `Issue` (`mp3_vbr_duration_unknown`), not estimate.
- MPEG audio frame detection must tolerate ID3v2 prefix of arbitrary (syncsafe) size — off-by-one on syncsafe decoding is the most likely bug.
- `audio.bit_depth` for MPEG Layer III is conventionally 16-bit output; emitting it as a fixed 16 could be challenged. Note in provenance that it is MPEG decoder convention, not tag-derived.
- `CAPABILITIES.json` may be regenerated from Rust metadata — verify `tools/generate_capabilities.py` ordering (hand-edit if the generator is not the source of truth here).
- Insta snapshot churn is expected; reviewer must `cargo insta review`, not blanket-accept (per SRS §4 drift policy).
- ID3v1 trailer (128 bytes at EOF) is out of scope for phase 1; detection path should still succeed for files that carry only ID3v1. If scope creep becomes tempting, defer to a follow-up.
