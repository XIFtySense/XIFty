<!-- loswf:plan -->
# Plan #54: Audio phase 1 — WAV (RIFF WAVE + fmt + BWF bext + iXML)

## Problem
XIFty currently treats every RIFF container as `WEBP`-only: `xifty-detect` only recognizes the `WEBP` form (`crates/xifty-detect/src/lib.rs:18-20`), `xifty-core::Format` has no `Wav` variant (`crates/xifty-core/src/lib.rs:6-30`), and `xifty-container-riff::parse_bytes` emits a blanket `riff_non_webp_form` info issue for any non-WEBP RIFF form (`crates/xifty-container-riff/src/lib.rs:152-161`). Audio creators (Suno, ElevenLabs, pro musicians) ship WAV as a first-class input — parent issue #5 lists WAV as P0. We need native WAV detection, `fmt ` / `data` parsing, BWF `bext` and iXML interpretation, duration/samplerate/channels/bitdepth surfacing via the CLI, and the decomposer-flagged side effect: the info issue must not fire on legitimate `WAVE` RIFFs.

## Approach
Extend the existing `xifty-container-riff` crate (keeping container parsing self-contained) by adding a `WAVE`-form recognizer that enumerates the same `RiffChunk` vector and surfaces typed views for `fmt `, `data`, `bext`, and `iXML` — mirroring how the same crate exposes `exif_payloads`/`xmp_payloads`/`icc_payloads`/`iptc_payloads` accessors (`crates/xifty-container-riff/src/lib.rs:22-45`). Restrict the `riff_non_webp_form` emission to RIFF forms that are **neither** `WEBP` nor `WAVE`, so the info stays a signal for truly unknown forms. Add two thin interpretation crates — `xifty-meta-bwf` and `xifty-meta-ixml` — that mirror the shape of `xifty-meta-rtmd` (`crates/xifty-meta-rtmd/src/lib.rs:1-60`): they take a typed payload struct and return `Vec<MetadataEntry>` with `Provenance { container: "wav", namespace: "bwf" | "ixml", … }`. Introduce `Format::Wav` in `xifty-core`, branch it from the RIFF magic in `xifty-detect`, and add probe/extract arms in `xifty-cli` that compose the new payload accessors. Surface duration / sample rate / channels / bit depth via `media_scalar_entry`-style synthetic entries (reusing the pattern at `crates/xifty-cli/src/lib.rs:919-940`) under a new `wav` / `quicktime`-analog namespace (we will use namespace `"wav"` for container-derived fields to stay distinct from QuickTime). Finally promote WAV in `CAPABILITIES.json` and add insta snapshots for probe + extract.

## Files to touch
- `Cargo.toml` — register two new workspace members (`xifty-meta-bwf`, `xifty-meta-ixml`).
- `crates/xifty-core/src/lib.rs` — add `Format::Wav` and its `as_str()` arm (edit the enum at lines 6-16 and the `match` at lines 20-28).
- `crates/xifty-detect/src/lib.rs` — branch WAVE form after the RIFF magic check (edit around line 18-20; extend the `detects_formats` test at lines 119-163).
- `crates/xifty-container-riff/src/lib.rs` — add `fmt_chunk()`, `data_chunk()`, `bext_chunk()`, `ixml_chunk()` accessors; add a parsed `WavFormat` helper struct; re-scope the `riff_non_webp_form` issue at lines 152-161 so it fires only when form_type is neither `WEBP` nor `WAVE`; expand tests at lines 171-205 with a synthetic `RIFF/WAVE` fixture.
- `crates/xifty-cli/src/lib.rs` — add a `Format::Wav` arm in `probe_source` (after the `Format::Webp` arm at lines 53-56) and in `extract_source` (after lines 362-442), composing bwf/ixml entries plus synthetic `SampleRate`/`Channels`/`BitDepth`/`DurationSeconds` entries from the parsed `fmt ` / `data` chunks. Register the two new crate deps.
- `crates/xifty-cli/Cargo.toml` — add `xifty-meta-bwf` and `xifty-meta-ixml` path deps (pattern: lines 17-29).
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshot tests for a new `happy.wav` fixture (and a `bext.wav` BWF fixture).
- `CAPABILITIES.json` — add a `"wav"` entry under `containers` (alongside `webp` at lines 61-68) with namespaces `bwf: bounded`, `ixml: bounded`, `wav: bounded`; leave the top-level namespaces map augmented with `bwf` and `ixml` entries (status `bounded`).

## New files
- `crates/xifty-meta-bwf/Cargo.toml` — new crate manifest mirroring `xifty-meta-rtmd/Cargo.toml`.
- `crates/xifty-meta-bwf/src/lib.rs` — `BwfPayload` struct + `decode_payload(...) -> Vec<MetadataEntry>` parsing BWF `bext`: 256-byte Description, 32-byte Originator, 32-byte OriginatorReference, 10-byte OriginationDate, 8-byte OriginationTime, 8-byte TimeReference (little-endian u64 samples), 2-byte Version, 64-byte UMID, reserved, CodingHistory (variable tail). Emit one `MetadataEntry` per field with `namespace: "bwf"`.
- `crates/xifty-meta-ixml/Cargo.toml` — mirrors `xifty-meta-rtmd/Cargo.toml`.
- `crates/xifty-meta-ixml/src/lib.rs` — `IxmlPayload` + `decode_payload(...)`; validates UTF-8 / starts with `<BWFXML` or `<?xml`, exposes the raw string as an `ixml.RawXml` `TypedValue::String` entry, and parses a bounded set of top-level `<PROJECT>`, `<SCENE>`, `<TAKE>`, `<TAPE>`, `<CIRCLED>`, `<NOTE>` elements via the same attribute-scan pattern used in `xifty-meta-rtmd` (`tag_attr`-style helpers).
- `fixtures/minimal/happy.wav` — minimal 12-byte RIFF/WAVE header plus a synthetic `fmt ` chunk (PCM, 1 channel, 44100 Hz, 16-bit) and a zero-length `data` chunk. Built by hand in the test file if binary fixtures cannot be committed (pattern matches the inline fixtures in `parses_minimal_webp_riff` at line 176).
- `fixtures/minimal/bext.wav` — same RIFF/WAVE shell plus a `bext` chunk with a filled 256-byte Description and an `iXML` chunk with a small `<BWFXML><PROJECT>xifty</PROJECT></BWFXML>` payload. Same inline-construction strategy permitted.

## Step-by-step
1. Add `Format::Wav` to `xifty-core::Format` plus the `as_str()` arm (`"wav"`) — verifiable by `cargo test -p xifty-core`.
2. In `xifty-detect`, branch on `&bytes[8..12] == b"WAVE"` immediately after the existing `WEBP` check in `detect()`; extend the existing `detects_formats` test to assert `Format::Wav` for a `RIFF\0\0\0\0WAVE` sample — verifiable via that test.
3. In `xifty-container-riff`:
   - Re-scope the issue at lines 152-161 to `&form_type != b"WEBP" && &form_type != b"WAVE"` and update the message to mention the actual form.
   - Add `fn fmt_chunk(&self) -> Option<&RiffChunk>` (matches `chunk_id == b"fmt "`), same for `data_chunk` (`b"data"`), `bext_chunk` (`b"bext"`), and `ixml_chunk` (`b"iXML"`).
   - Add `pub struct WavFormat { pub format_tag: u16, pub channels: u16, pub sample_rate: u32, pub byte_rate: u32, pub block_align: u16, pub bits_per_sample: u16 }` plus `pub fn parse_wav_format(payload: &[u8]) -> Option<WavFormat>` (reads little-endian 16 bytes).
   - Add unit tests: synthetic `RIFF/WAVE + fmt + data` round-trips chunk count and `parse_wav_format` decodes PCM 44.1k/16-bit/1ch; assert `riff_non_webp_form` is NOT present in `issues`; assert it IS present for a third-party form such as `AVI `.
4. Create `xifty-meta-bwf` crate (manifest + `src/lib.rs`) and tests: encode a synthetic 602-byte+ `bext` payload and assert that Description/Originator/TimeReference metadata entries are produced with the correct provenance (`container: "wav"`, `namespace: "bwf"`).
5. Create `xifty-meta-ixml` crate + tests: feed a `<BWFXML><PROJECT>xifty</PROJECT><SCENE>test</SCENE></BWFXML>` string, assert the raw-XML entry plus at least `PROJECT` and `SCENE` string entries.
6. Register both new crates in the top-level `Cargo.toml` workspace members list (alphabetical block at lines 3-25).
7. In `xifty-cli/src/lib.rs`:
   - Import the two new crates.
   - Add a `Format::Wav` arm to `probe_source` producing `("wav".to_string(), riff.nodes, riff.issues)`.
   - Add a `Format::Wav` arm to `extract_source`: call `parse_riff`, if `fmt_chunk()` exists decode via `parse_wav_format` and `payload_slice`, synthesize `wav`/`SampleRate`, `Channels`, `BitDepth`, `AudioFormat` entries (`namespace: "wav"`); if `data_chunk()` exists and `byte_rate > 0` compute `DurationSeconds` = `data_length as f64 / byte_rate as f64` and emit; if `bext_chunk()` / `ixml_chunk()` exist, slice the payload bytes and call `xifty_meta_bwf::decode_payload` / `xifty_meta_ixml::decode_payload`; append all entries; return `("wav".to_string(), riff.nodes, entries, issues)`. Emit targeted `wav_fmt_unparseable` / `wav_ixml_not_xml` issues with the `namespace_issue` helper (lines 621-629) when a chunk is present but unusable.
8. Add a small `audio_scalar_entry(namespace, tag, value, note)` helper (parallel to `media_scalar_entry` at lines 919-940) so provenance carries `container: "wav"`, `namespace: "wav"`.
9. Extend `crates/xifty-cli/tests/cli_contract.rs` with:
   - `fn probe_happy_wav()` — snapshot of `probe_path("happy.wav")`.
   - `fn extract_happy_wav_normalized()` — snapshot of `extract_path` at `ViewMode::Normalized`.
   - `fn extract_bext_wav_interpreted()` — snapshot of `extract_path("bext.wav", Interpreted)` to lock in BWF + iXML fields.
10. Flip `wav` in `CAPABILITIES.json`: add `"wav"` under `containers` with `{ "bwf": "bounded", "ixml": "bounded", "wav": "bounded" }`; add `bwf` and `ixml` top-level namespace entries with `status: "bounded"`.
11. Run the full validation suite; `cargo insta review` the new snapshots (never blanket-accept per guardrail).

## Tests
- Step 1: unit test in `xifty-core` verifying `Format::Wav.as_str() == "wav"` round-trips (extend existing tests if present).
- Step 2: extend `detects_formats` in `xifty-detect/src/lib.rs:119-163`.
- Step 3: new unit tests inside `xifty-container-riff/src/lib.rs` `mod tests`: `parses_minimal_wave_riff`, `suppresses_non_webp_info_for_wave`, `emits_non_webp_info_for_avi`, `parses_wav_format`.
- Step 4: new `crates/xifty-meta-bwf/src/lib.rs` `#[cfg(test)]` module with `decodes_bext_description`, `decodes_time_reference`.
- Step 5: new `crates/xifty-meta-ixml/src/lib.rs` `#[cfg(test)]` module with `exposes_raw_xml`, `parses_project_and_scene`.
- Step 7-9: `crates/xifty-cli/tests/cli_contract.rs` insta snapshots (`probe_happy_wav`, `extract_happy_wav_normalized`, `extract_bext_wav_interpreted`). Use the existing inline-fixture approach or add files under `fixtures/minimal/`.

## Validation
Per `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

No FFI shape change is expected (the new `Format::Wav` variant flows through JSON only); still run the FFI tests per the pipeline.

## Risks
- **Serde compatibility of `Format`**: `Format` derives `Serialize` (`crates/xifty-core/src/lib.rs:6-8`); adding a variant is additive and should not break existing consumers, but any downstream enum match (e.g., the FFI / WASM layers) might need a `Wav` arm. Search for `Format::` exhaustively before build; currently only `crates/xifty-cli/src/lib.rs` and `crates/xifty-detect/src/lib.rs` match on it (grep confirms).
- **Riff chunk ID padding**: `fmt ` chunk ID contains a trailing space — must use `b"fmt "` literally; easy to typo to `b"fmt"`.
- **BWF size variance**: historic `bext` chunks sometimes omit `CodingHistory` or truncate; the decoder must bounds-check every field and degrade to emitting only the fields that fit.
- **iXML XML parsing scope**: we deliberately do NOT pull in a full XML parser crate; we parse a bounded set of elements via substring scanning (same pattern as `xifty-meta-rtmd`). Anything richer is future work; document this in the crate docstring.
- **Non-PCM WAV codec tags**: `format_tag != 1` (e.g., IEEE float, A-law, extensible WAVEFORMATEXTENSIBLE `0xFFFE`) will still parse the header, but `BitDepth` / `Duration` may be misleading. Emit an info issue `wav_non_pcm_format` for non-PCM tags so users are warned.
- **`riff_non_webp_form` consumers**: grep confirms no CLI snapshot or test outside this crate asserts on that code, so re-scoping it is safe without snapshot churn.
- **Fixture bytes**: if binary fixtures cannot be committed under `fixtures/minimal/`, keep the WAV fixtures inline in the test file (pattern matches the inline byte literals already used in `routes_iccp_chunks`).

