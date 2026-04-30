<!-- loswf:plan -->
# Plan #54: Audio phase 1 — WAV (RIFF WAVE + fmt + BWF bext + iXML) (v2)

## Problem
XIFty currently treats every RIFF container as `WEBP`-only: `xifty-detect` only recognizes the `WEBP` form (`crates/xifty-detect/src/lib.rs:18-20`), `xifty-core::Format` has no `Wav` variant (`crates/xifty-core/src/lib.rs:6-30`), and `xifty-container-riff::parse_bytes` emits a blanket `riff_non_webp_form` info issue for any non-WEBP RIFF form (`crates/xifty-container-riff/src/lib.rs:152-161`). Audio creators (Suno, ElevenLabs, pro musicians) ship WAV as a first-class input — parent issue #5 lists WAV as P0. We need native WAV detection, `fmt ` / `data` parsing, BWF `bext` and iXML interpretation, duration/samplerate/channels/bitdepth surfacing via the CLI, and the decomposer-flagged side effect: the info issue must not fire on legitimate `WAVE` RIFFs.

## Approach
Extend `xifty-container-riff` with a `WAVE`-form recognizer that surfaces typed accessors for `fmt `, `data`, `bext`, and `iXML` — mirroring the existing `exif_payloads`/`xmp_payloads`/`icc_payloads`/`iptc_payloads` accessor pattern (`crates/xifty-container-riff/src/lib.rs:22-45`). Re-scope `riff_non_webp_form` so the info stays a signal only for truly unknown forms. Add two interpretation crates — `xifty-meta-bwf` and `xifty-meta-ixml` — mirroring `xifty-meta-rtmd` (`crates/xifty-meta-rtmd/src/lib.rs:1-60`). Introduce `Format::Wav` in `xifty-core`, branch it from the RIFF magic in `xifty-detect`, and add probe/extract arms in `xifty-cli` that compose the new payload accessors. Surface duration / sample rate / channels / bit depth via a new `audio_scalar_entry` helper cloned from `media_scalar_entry` (`crates/xifty-cli/src/lib.rs:919-940`) with every occurrence of `"quicktime"` replaced by `"wav"`. Promote WAV in `CAPABILITIES.json` and add insta snapshots for probe + extract.

## Files to touch
- `Cargo.toml` — register two new workspace members between the existing neighbors (see Step 6 for exact insertion points).
- `crates/xifty-core/src/lib.rs` — add `Format::Wav` and its `as_str()` arm (edit the enum at lines 6-16 and the `match` at lines 20-28).
- `crates/xifty-detect/src/lib.rs` — branch WAVE form after the RIFF magic check (edit around line 18-20; extend `detects_formats` at lines 119-163).
- `crates/xifty-container-riff/src/lib.rs` — add `fmt_chunk()`, `data_chunk()`, `bext_chunk()`, `ixml_chunk()` accessors; add a parsed `WavFormat` helper struct; re-scope the `riff_non_webp_form` issue at lines 152-161 so it fires only when form_type is neither `WEBP` nor `WAVE`; expand tests at lines 171-205 with a synthetic `RIFF/WAVE` fixture.
- `crates/xifty-cli/src/lib.rs` — add a `Format::Wav` arm in `probe_source` (after the `Format::Webp` arm at lines 53-56) and in `extract_source` (after lines 362-442), composing bwf/ixml entries plus synthetic `SampleRate`/`Channels`/`BitDepth`/`DurationSeconds` entries from the parsed `fmt ` / `data` chunks. Add `audio_scalar_entry` helper near `media_scalar_entry` at line 919.
- `crates/xifty-cli/Cargo.toml` — add `xifty-meta-bwf` and `xifty-meta-ixml` path deps (pattern: lines 17-29).
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshot tests for new `happy.wav` / `bext.wav` fixtures.
- `CAPABILITIES.json` — add a `"wav"` entry under `containers` (alongside `webp` at lines 61-68) with namespaces `bwf: bounded`, `ixml: bounded`, `wav: bounded`; augment top-level namespaces with `bwf` and `ixml` entries (status `bounded`).

## New files
- `crates/xifty-meta-bwf/Cargo.toml` — mirrors `crates/xifty-meta-rtmd/Cargo.toml`.
- `crates/xifty-meta-bwf/src/lib.rs` — `BwfPayload` struct + `decode_payload(...) -> Vec<MetadataEntry>` parsing BWF `bext`: 256-byte Description, 32-byte Originator, 32-byte OriginatorReference, 10-byte OriginationDate, 8-byte OriginationTime, 8-byte TimeReference (little-endian u64 samples), 2-byte Version, 64-byte UMID, reserved, CodingHistory (variable tail). Emit one `MetadataEntry` per field with `namespace: "bwf"`, `provenance.container: "wav"`, `provenance.namespace: "bwf"`.
- `crates/xifty-meta-ixml/Cargo.toml` — mirrors `crates/xifty-meta-rtmd/Cargo.toml`.
- `crates/xifty-meta-ixml/src/lib.rs` — `IxmlPayload` + `decode_payload(...)`; validates UTF-8 / starts with `<BWFXML` or `<?xml`, exposes the raw string as an `ixml.RawXml` `TypedValue::String` entry, and parses a bounded set of top-level `<PROJECT>`, `<SCENE>`, `<TAKE>`, `<TAPE>`, `<CIRCLED>`, `<NOTE>` elements via the same attribute-scan pattern used in `xifty-meta-rtmd` (`tag_attr`-style helpers).
- `fixtures/minimal/happy.wav` — minimal 12-byte RIFF/WAVE header plus a synthetic `fmt ` chunk (PCM, 1 channel, 44100 Hz, 16-bit) and a zero-length `data` chunk. If binary commits are undesirable, construct inline in the test file (pattern: inline byte literals in `parses_minimal_webp_riff` at line 176).
- `fixtures/minimal/bext.wav` — same RIFF/WAVE shell plus a `bext` chunk with a filled 256-byte Description and an `iXML` chunk containing `<BWFXML><PROJECT>xifty</PROJECT></BWFXML>`. Same inline-construction allowance.

## Step-by-step
1. Add `Format::Wav` to `xifty-core::Format` plus the `as_str()` arm (`"wav"`) — verifiable by `cargo test -p xifty-core`.
2. In `xifty-detect`, branch on `&bytes[8..12] == b"WAVE"` immediately after the existing `WEBP` check in `detect()`; extend `detects_formats` to assert `Format::Wav` for a `RIFF\0\0\0\0WAVE` sample.
3. In `xifty-container-riff/src/lib.rs`:
   - Re-scope the issue at lines 152-161 to `&form_type != b"WEBP" && &form_type != b"WAVE"` and update the message to mention the actual form.
   - Add `fn fmt_chunk(&self) -> Option<&RiffChunk>` (matches `chunk_id == b"fmt "`; note the trailing space), `data_chunk` (`b"data"`), `bext_chunk` (`b"bext"`), `ixml_chunk` (`b"iXML"`).
   - Add `pub struct WavFormat { pub format_tag: u16, pub channels: u16, pub sample_rate: u32, pub byte_rate: u32, pub block_align: u16, pub bits_per_sample: u16 }` plus `pub fn parse_wav_format(payload: &[u8]) -> Option<WavFormat>` (little-endian 16-byte read; returns `None` when payload length < 16).
   - Unit tests: `parses_minimal_wave_riff`, `suppresses_non_webp_info_for_wave`, `emits_non_webp_info_for_avi`, `parses_wav_format`.
4. Create `xifty-meta-bwf` crate (manifest + `src/lib.rs`) and tests: encode a synthetic 602-byte+ `bext` payload and assert Description/Originator/TimeReference entries with the correct provenance.
5. Create `xifty-meta-ixml` crate + tests: feed `<BWFXML><PROJECT>xifty</PROJECT><SCENE>test</SCENE></BWFXML>`, assert the raw-XML entry plus `PROJECT` and `SCENE` string entries.
6. Register both new crates in the top-level `Cargo.toml` workspace members list. **Exact insertion neighbors** (current state: `xifty-meta-apple` at line 14, `xifty-meta-sony` at line 15):
   - Insert `"crates/xifty-meta-bwf",` **after** `"crates/xifty-meta-apple",` (line 14) and **before** `"crates/xifty-meta-sony",`.
   - Insert `"crates/xifty-meta-ixml",` **after** `"crates/xifty-meta-bwf",` and **before** `"crates/xifty-meta-sony",`.
   - Final order in that block: `xifty-meta-apple`, `xifty-meta-bwf`, `xifty-meta-ixml`, `xifty-meta-sony`. Do NOT append at the end of the member list.
7. In `xifty-cli/src/lib.rs`:
   - Import the two new crates.
   - Add a `Format::Wav` arm to `probe_source` producing `("wav".to_string(), riff.nodes, riff.issues)`.
   - Add a `Format::Wav` arm to `extract_source`:
     - Call `parse_riff`.
     - **Duration fallback rule (explicit):** If `fmt_chunk()` is present AND `parse_wav_format` returns `Some(WavFormat)` AND `byte_rate > 0` AND `data_chunk()` is present, compute `DurationSeconds = data_length as f64 / byte_rate as f64` and emit it via `audio_scalar_entry`. Otherwise:
       - If `fmt_chunk()` is absent while `data_chunk()` is present: DO NOT compute duration; emit an info issue with `code: "wav_fmt_missing"` via the `namespace_issue` helper (lines 621-629) and skip `DurationSeconds`.
       - If `fmt_chunk()` is present but `parse_wav_format` returns `None`: emit `code: "wav_fmt_unparseable"` and skip `DurationSeconds`.
       - If `byte_rate == 0`: emit `code: "wav_fmt_zero_byterate"` and skip `DurationSeconds`.
     - When `fmt_chunk()` parses successfully, synthesize `SampleRate`, `Channels`, `BitDepth`, `AudioFormat` entries via `audio_scalar_entry`.
     - If `bext_chunk()` / `ixml_chunk()` exist, slice the payload and call `xifty_meta_bwf::decode_payload` / `xifty_meta_ixml::decode_payload`; append all entries. Emit targeted `wav_ixml_not_xml` when iXML payload is not valid UTF-8/XML.
     - Return `("wav".to_string(), riff.nodes, entries, issues)`.
8. Add a new `audio_scalar_entry(container_name: &str, tag_name: &str, value: TypedValue, note: &str) -> MetadataEntry` helper adjacent to `media_scalar_entry` (insert after line 940). **Explicit substitution list — the new helper must be a copy of `media_scalar_entry` with every `"quicktime"` literal replaced by `"wav"`. There are exactly two hardcoded `"quicktime"` occurrences in `media_scalar_entry` (verified against `crates/xifty-cli/src/lib.rs:919-940`):**
   - Line 926: `namespace: "quicktime".into(),` (the `MetadataEntry.namespace` field) → must become `namespace: "wav".into(),`.
   - Line 932: `namespace: "quicktime".into(),` (the `Provenance.namespace` field inside the `provenance: Provenance { … }` literal) → must become `namespace: "wav".into(),`.
   - The `provenance.container` value at line 931 is already parameterized via `container_name: &str` and is NOT a hardcoded `"quicktime"` string; leave its parameterization unchanged — callers will pass `"wav"`.
   - The `tag_id` and `tag_name` at lines 927-928 are parameterized; unchanged.
   - No other `"quicktime"` literals exist inside `media_scalar_entry`; do NOT introduce new ones.
   - Rename the function to `audio_scalar_entry`; keep the same signature shape. A grep of the new helper body for the string `quicktime` MUST return zero hits before the builder moves on.
9. Extend `crates/xifty-cli/tests/cli_contract.rs` with `probe_happy_wav`, `extract_happy_wav_normalized`, `extract_bext_wav_interpreted` insta snapshots.
10. Flip `wav` in `CAPABILITIES.json`: add `"wav"` under `containers` with `{ "bwf": "bounded", "ixml": "bounded", "wav": "bounded" }`; add `bwf` and `ixml` top-level namespace entries with `status: "bounded"`.
11. Run the full validation suite; `cargo insta review` the new snapshots (never blanket-accept per guardrail).

## Tests
- Step 1: unit test in `xifty-core` verifying `Format::Wav.as_str() == "wav"`.
- Step 2: extend `detects_formats` in `xifty-detect/src/lib.rs:119-163`.
- Step 3: new unit tests inside `xifty-container-riff/src/lib.rs`: `parses_minimal_wave_riff`, `suppresses_non_webp_info_for_wave`, `emits_non_webp_info_for_avi`, `parses_wav_format`.
- Step 4: `crates/xifty-meta-bwf/src/lib.rs` `#[cfg(test)]` tests: `decodes_bext_description`, `decodes_time_reference`.
- Step 5: `crates/xifty-meta-ixml/src/lib.rs` `#[cfg(test)]` tests: `exposes_raw_xml`, `parses_project_and_scene`.
- Step 7 duration fallback: add a `wav_fmt_missing_emits_issue` test (synthetic WAV with `data` but no `fmt ` chunk) asserting the info issue code is present and no `DurationSeconds` entry is produced.
- Step 8 helper: a direct unit test calling `audio_scalar_entry("wav", "SampleRate", TypedValue::U64(44100), "note")` asserting both `entry.namespace == "wav"` and `entry.provenance.namespace == "wav"` (guards against a future regression where someone copies the `"quicktime"` literal back in).
- Steps 7-9: `cli_contract.rs` insta snapshots.

## Validation
Per `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

No FFI shape change expected; still run FFI tests per the pipeline.

## Risks
- **Serde compatibility of `Format`**: `Format` derives `Serialize` (`crates/xifty-core/src/lib.rs:6-8`); adding a variant is additive. Exhaustive `match format` exists only in `crates/xifty-cli/src/lib.rs` (two blocks) and `crates/xifty-detect/src/lib.rs`; FFI / WASM do not pattern-match `Format`.
- **Riff chunk ID padding**: `fmt ` chunk ID contains a trailing space — must use `b"fmt "` literally.
- **BWF size variance**: historic `bext` chunks sometimes omit or truncate `CodingHistory`; the decoder must bounds-check every field and degrade.
- **iXML XML parsing scope**: deliberately no full XML parser crate; parse a bounded element set via substring scanning (same pattern as `xifty-meta-rtmd`). Document in crate docstring.
- **Non-PCM WAV codec tags**: `format_tag != 1` (IEEE float, A-law, WAVEFORMATEXTENSIBLE `0xFFFE`) will still parse the header, but `BitDepth` / `Duration` may be misleading. Emit `wav_non_pcm_format` info issue.
- **`riff_non_webp_form` consumers**: grep confirms no CLI snapshot or external test asserts on that code; re-scoping is safe.
- **Fixture bytes**: if binary fixtures cannot be committed under `fixtures/minimal/`, keep WAV fixtures inline (pattern matches `routes_iccp_chunks`).
- **Helper-copy hazard (addressed in Step 8)**: copying `media_scalar_entry` without substitution would silently emit `wav`-derived audio metadata under `"quicktime"` namespace — mitigated by the explicit substitution list in Step 8 and the direct helper unit test in the Tests section.
- **Duration without `fmt ` (addressed in Step 7)**: a `data`-only RIFF/WAVE would yield NaN/Inf duration; mitigated by the explicit fallback rule and `wav_fmt_missing` issue.
