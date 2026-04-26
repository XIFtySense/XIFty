<!-- loswf:plan -->
# Plan #87: DJI drone telemetry — udta classic-text 4cc keys in MP4

## Problem
DJI drone MP4s (Mavic 3 Classic / FC3682 confirmed against real captures at `/Volumes/KHAOS2/DCIM/100MEDIA/DJI_000{1..5}.MP4`) embed flight + gimbal + GPS telemetry as `©`-prefixed 4cc atoms placed **directly under `udta`** (not under `udta/meta/ilst`). They use the classic QuickTime user-data text format (`{u16 BE length}{u16 BE language=0xff7f}{ascii payload}`) — *not* the iTunes `data`-sub-box format that `crates/xifty-meta-quicktime/src/lib.rs:43-58` and `parse_itunes_item` (`crates/xifty-container-isobmff/src/lib.rs:1120-1135`) expect.

Issue #86 hypothesised the metadata lived in an XMP packet inside a `uuid` box (`be7acfcb-97a9-42e8-9c71-999491e3afac`). Direct inspection of all five DJI files turned up zero `uuid` boxes and zero XMP packets — that path exists in older firmware (Mavic 2 / Phantom 4 era) and will be addressed in a follow-up issue, but is not what the current kstore captures need.

Confirmed key inventory under `udta` for FC3682 (DJI_0003.MP4):

| 4cc | Meaning | Sample |
|---|---|---|
| `©xyz` | ISO 6709 location | `+40.7922-73.9584` |
| `©xsp` `©ysp` `©zsp` | speed XYZ (m/s) | `+0.00` |
| `©fpt` `©fyw` `©frl` | flight pitch / yaw / roll (deg) | `+0.90` `+175.50` `-3.90` |
| `©gpt` `©gyw` `©grl` | gimbal pitch / yaw / roll (deg) | `-31.20` `-2.30` `+0.00` |
| `©mdl` | camera model | `FC3682` |
| `©csn` | camera serial number | `53HQN4T0M5B7JW` |
| `©uid` `©aud` `©mdt` `©mux` `©rec` `©res` | binary internal/timing | (out of scope) |

Plus a per-frame telemetry track (`hdlr=DJI.Meta`, `MetaFormat=priv`) and a `DJI.Subtitle` text track — both deferred.

## Approach
Mirror the existing iTunes wedge from #58. Container surfaces raw byte payloads under a new `kind: "quicktime-udta"`; metadata crate interprets the classic udta text shape and emits `MetadataEntry` values under a new `dji` namespace; CLI orchestrator gains a routing loop; policy/normalize gain a `drone.*` group plus a new `device.serial_number` field. ISO 6709 splitting for `©xyz` lives in the meta crate (one input atom → up to three normalized fields: latitude, longitude, optional altitude). No new container crate, no new top-level format (`Format::Mp4`/`Format::Mov` already routes correctly).

`xifty-detect` does **not** need a `Format::Dji` — DJI is a sub-brand of MP4, recognized by the `©mdl=FC*` value during normalization. We surface DJI-ness only via the `dji` namespace on resulting `MetadataEntry`s.

## Files to touch
- `crates/xifty-container-isobmff/src/lib.rs` — Add atom-match arm for the DJI 4cc set under `udta` (lines 387-400 area). New helper `parse_quicktime_udta_text(cursor, &parsed) -> Option<IsobmffPayload>` that emits `IsobmffPayload { kind: "quicktime-udta", tag: Some(label), … }`. Add `quicktime_udta_payloads()` accessor on `IsobmffContainer` (lines 47-78).
- `crates/xifty-meta-quicktime/src/lib.rs` — Add `QuickTimeUdtaPayload<'a>` + `decode_udta_payload(...) -> Vec<MetadataEntry>`. Decoder reads `{u16 len}{u16 lang}{ascii}` (reject `data`-box-shaped payloads — those go through the existing `decode_payload` path). 4cc → tag-name table; ISO 6709 splitter for `©xyz` → `GPSLatitude`/`GPSLongitude`/`GPSAltitude`. Numeric atoms (`©fpt`, `©fyw`, `©frl`, `©gpt`, `©gyw`, `©grl`, `©xsp`, `©ysp`, `©zsp`) emit `TypedValue::Float`; string atoms (`©mdl`, `©csn`) emit `TypedValue::String`. Namespace `"dji"`.
- `crates/xifty-cli/src/lib.rs` — Add a routing loop after the existing `quicktime_payloads()` loop (lines 843-856) that iterates `container.quicktime_udta_payloads()` and calls `decode_udta_payload(...)`.
- `crates/xifty-policy/src/lib.rs` — Add `device.serial_number` reconciliation (mirror existing `device.model` rule). Add nine `drone.*` numeric reconciliations (flight + gimbal + speed).
- `crates/xifty-normalize/src/lib.rs` — Wire `dji` namespace into existing GPS reconciliation so `©xyz` participates alongside EXIF GPS. Surface `drone.*` and `device.serial_number` from policy.
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_snapshot_dji_mavic3`, `extract_snapshot_dji_mavic3_normalized`, `extract_snapshot_dji_mavic3_interpreted` (mirror `extract_real_camera_mp4_*` pattern).
- `tools/generate_fixtures.py` — New `build_dji_mavic3_mp4()` (or trim helper) + register `"dji_mavic3.mp4"` in the output dict.
- `CAPABILITIES.json` — Add `"dji": {"status": "bounded"}` namespace entry; extend `mp4`/`mov` containers with `"dji": "bounded"`.

## New files
- `fixtures/minimal/dji_mavic3.mp4` — Trimmed/synthetic minimal MP4 with `ftyp` + tiny `mdat` + tail `moov` carrying `udta` with the documented 4cc set. Generated via `tools/generate_fixtures.py`.
- (No new crates.)

## Step-by-step
1. **Container parser.** In `crates/xifty-container-isobmff/src/lib.rs`, after the existing `b"\xa9ART" | b"\xa9too" | b"\xa9nam"` arm at line 387, add a branch that fires only when `parsed.path` does **not** contain `"ilst"` (so it never collides with iTunes routing). Match the DJI 4cc set + reuse for any direct-`udta` text atom: `©xyz`, `©xsp`, `©ysp`, `©zsp`, `©fpt`, `©fyw`, `©frl`, `©gpt`, `©gyw`, `©grl`, `©mdl`, `©csn`. Call new helper `parse_quicktime_udta_text(cursor, &parsed)` that:
   - Slices `cursor.bytes()[parsed.data_offset..parsed.end]`.
   - Requires `>= 4` bytes; reads `len = u16::from_be_bytes(...)`, `lang = u16::from_be_bytes(...)` from offsets 0-3.
   - Validates `4 + len <= remaining`. (Lang 0xff7f is QuickTime "no language"; we accept anything but record it.)
   - Returns `Some(IsobmffPayload { kind: "quicktime-udta", tag: Some(itunes_tag_label(parsed.box_type)), data_offset: cursor.absolute_offset(parsed.data_offset), data_length: (parsed.end - parsed.data_offset) as u64, … })`.
   - Returns `None` if the slice instead looks like a `data`-sub-box (i.e. `slice[4..8] == b"data"`) — that path is already handled by `parse_itunes_item`.
   Add `quicktime_udta_payloads()` accessor to `IsobmffContainer` mirroring `quicktime_payloads`. — Outcome: `cargo test -p xifty-container-isobmff` passes; an in-crate test with a synthetic `udta/©fpt` payload surfaces a `quicktime-udta` payload.

2. **Meta decoder.** In `crates/xifty-meta-quicktime/src/lib.rs`:
   ```rust
   pub struct QuickTimeUdtaPayload<'a> {
       pub key: &'a str,           // e.g. "©fpt"
       pub bytes: &'a [u8],        // raw atom data: {u16 len}{u16 lang}{ascii}
       pub container: &'a str,
       pub offset_start: u64,
       pub offset_end: u64,
   }
   pub fn decode_udta_payload(p: QuickTimeUdtaPayload<'_>) -> Vec<MetadataEntry> { … }
   ```
   Decode steps: read len/lang, slice text, trim trailing `\0`. Map key→tag-name+kind:
   - `©fpt` → `("FlightPitchDegree", Float)`, `©fyw` → `FlightYawDegree`, `©frl` → `FlightRollDegree`
   - `©gpt` → `GimbalPitchDegree`, `©gyw` → `GimbalYawDegree`, `©grl` → `GimbalRollDegree`
   - `©xsp` → `SpeedX`, `©ysp` → `SpeedY`, `©zsp` → `SpeedZ` (all Float, m/s)
   - `©mdl` → `("Model", String)`, `©csn` → `("SerialNumber", String)`
   - `©xyz` → expand to multiple entries: parse ISO 6709 (`+40.7922-73.9584` / `+40.7922-73.9584+050.000/`) into `GPSLatitude`, `GPSLongitude`, optional `GPSAltitude` (all Float).
   Unknown keys: emit one entry with the raw key as tag-name + `TypedValue::String(text)` so they're surfaced (lossless) but not normalized. All entries `namespace: "dji"`.
   - Inline `#[cfg(test)]` covering: numeric (©fpt), signed numeric (©frl=-3.90), ISO 6709 lat/lon, ISO 6709 lat/lon/alt, malformed (rejected gracefully). — Outcome: `cargo test -p xifty-meta-quicktime` passes.

3. **CLI routing.** In `crates/xifty-cli/src/lib.rs`, after the `quicktime_payloads()` loop at line 843, add:
   ```rust
   for payload in container.quicktime_udta_payloads() {
       if let (Some(tag), Some(bytes)) = (
           payload.tag.as_deref(),
           payload_slice(bytes, payload.data_offset, payload.data_length as usize),
       ) {
           entries.extend(decode_udta_payload(QuickTimeUdtaPayload { … }));
       }
   }
   ```
   Re-export `decode_udta_payload` from `xifty_meta_quicktime` import line. — Outcome: `cargo build -p xifty-cli` passes.

4. **Policy + normalize.** In `crates/xifty-policy/src/lib.rs` `reconcile()`, add:
   - `device.serial_number` ← `["SerialNumber"]` from any namespace; precedence ExifFirst then `dji`.
   - `drone.flight.pitch_deg` ← `["FlightPitchDegree"]` namespace `dji`.
   - …same shape for `flight.{yaw,roll}_deg`, `gimbal.{pitch,yaw,roll}_deg`, `speed.{x,y,z}_mps` (mapping `SpeedX/Y/Z` → `speed.{x,y,z}_mps`).
   In `crates/xifty-normalize/src/lib.rs`, allow `dji` as an additional source for the existing GPS lat/lon/alt enrichment; if EXIF GPS is absent and `dji` GPS is present, populate `location.{latitude,longitude,altitude}` from `dji`'s `GPSLatitude/Longitude/Altitude`. — Outcome: `cargo test -p xifty-normalize -p xifty-policy` passes.

5. **Fixture.** In `tools/generate_fixtures.py`, add a builder that emits a minimal MP4: `ftyp(major=mp42, compat=[avc1,isom])`, 8-byte stub `mdat`, then `moov` containing `mvhd` + `udta` with the twelve documented DJI atoms (each in classic udta-text format), plus a single trivial video track to keep `parse_isobmff` happy. Register under `"dji_mavic3.mp4"`. Run `python tools/generate_fixtures.py`. — Outcome: `fixtures/minimal/dji_mavic3.mp4` exists; first 12 bytes are `\x00\x00\x00\x1cftyp` + `mp42`.

6. **CLI snapshots.** Add three tests in `crates/xifty-cli/tests/cli_contract.rs`:
   - `probe_snapshot_dji_mavic3` ← `assert_json_snapshot!("probe_dji_mavic3", probe_json("dji_mavic3.mp4"))`
   - `extract_snapshot_dji_mavic3_interpreted` (mode=Interpreted)
   - `extract_snapshot_dji_mavic3_normalized` (mode=Normalized)
   Run `cargo insta review`; accept the three new snapshots. — Outcome: snapshot files land under `crates/xifty-cli/tests/snapshots/`.

7. **Capabilities.** Update `CAPABILITIES.json`: add `"dji"` namespace entry under `namespaces` (status `"bounded"`), and add `"dji": "bounded"` to the `mp4` and `mov` container entries. — Outcome: hygiene check stays green.

8. **Validation.** Run the chain in §Validation. — Outcome: workspace test suite green; `xifty-cli extract /Volumes/KHAOS2/DCIM/100MEDIA/DJI_0003.MP4 --view normalized` shows `drone.flight.*`, `drone.gimbal.*`, `drone.speed.*`, `device.model="FC3682"`, `device.serial_number="53HQN4T0M5B7JW"`, and populated `location.{latitude,longitude}`.

## Tests
- Step 1: in-crate isobmff test feeding a synthetic `udta/©fpt` text atom + `udta/meta/ilst/©cmt` (proves the path-discrimination doesn't double-emit on iTunes atoms).
- Step 2: inline meta-quicktime tests (numeric, signed, ISO 6709 with/without altitude, malformed, unknown-key passthrough).
- Step 4: policy unit tests asserting precedence (EXIF Make beats DJI Model when both present; DJI GPS used iff EXIF GPS absent).
- Step 6: three CLI snapshot tests.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo run -p xifty-cli -- extract /Volumes/KHAOS2/DCIM/100MEDIA/DJI_0003.MP4 --view normalized | rg 'drone\\.|device\\.serial|location\\.'` (smoke against real file — confirms wedge closes the kstore gap)

## Risks
- **Path discrimination between direct-udta and iTunes ilst.** The DJI keys (`©xyz`, `©cmt`, …) overlap with iTunes 4ccs. The disambiguator is parent path: `udta/©xyz` is classic text, `udta/meta/ilst/©cmt` is iTunes `data`-box. Mitigation: branch on `parsed.path.contains("ilst")`; add the synthetic-collision test in step 1.
- **Lang field 0xff7f is non-standard but observed in the wild.** Spec says lang is a Macintosh language code; DJI uses `0xff7f` as "no language". Decoder must accept any value (record but don't validate).
- **ISO 6709 altitude optional.** `©xyz=+40.7922-73.9584` (no altitude) is valid; some firmware emits `+40.7922-73.9584+050.000/`. Parser must tolerate both.
- **Key overlap with standard QuickTime tags.** `©xyz` is also a standard QuickTime location tag; if `xifty-detect` or another path already surfaces it, we'd double-count. Currently no other path surfaces `©xyz` (verified — `parse_quicktime_item` requires `data` sub-box, which DJI's `©xyz` lacks). Note in PR body.
- **Older DJI firmware uses XMP+uuid path.** Not addressed by this issue. Document explicitly so users with Mavic 2 / Phantom 4 captures know to file a follow-up. Recommend a separate issue once we have a sample file.
- **Per-frame telemetry track (`DJI.Meta` `MetaFormat=priv`).** Not addressed. Out of scope; no XMP path in this firmware means we don't lose anything that XMP would have given us.
- **Non-DJI MP4s.** Adding `quicktime-udta` parsing for direct-udta `©`-atoms benefits any classic QuickTime file (e.g. Final Cut udta-text); the `dji` namespace label is a lie for non-DJI. Mitigation: gate the namespace tag on `©mdl` content being recognizably DJI? Decision: keep namespace `"dji"` for the documented DJI 4cc set (`©fpt`/`©fyw`/`©frl`/`©gpt`/`©gyw`/`©grl`/`©xsp`/`©ysp`/`©zsp`/`©csn`) which are DJI-only; emit standard-QuickTime atoms (`©xyz`, `©mdl`) under namespace `"quicktime"` so they generalise. Re-evaluate after seeing first non-DJI fixture.
