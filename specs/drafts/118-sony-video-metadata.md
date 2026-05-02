<!-- loswf:plan -->
# Plan #118: Sony video metadata — native decoder for PROF/USMT UUID atoms in MP4/MOV

## Problem
Sony pro/prosumer video cameras (FX-series, A7-series video, A1) embed proprietary metadata in ISOBMFF `uuid` boxes under the **Sony User Data Atom UUID family** — a 16-byte usertype where bytes 0..4 are the atom name in ASCII and bytes 4..16 are the constant tail `21D24FCEBB88695CFAC9C740`. `xifty-container-isobmff` already structurally walks `uuid` boxes (see `crates/xifty-container-isobmff/src/lib.rs:1816-1903` `parse_uuid`), but only the Canon CMT UUID is dispatched; every other usertype emits an `isobmff_structure_recognized_uninterpreted` info-issue (line 1846-1853) and is dropped. There is no `xifty-meta-sony-video` crate; `xifty-meta-sony` is JPEG MakerNote and does not apply. Snapshots `extract_real_camera_mp4_normalized` / `extract_real_camera_mp4_interpreted` (`crates/xifty-cli/tests/cli_contract.rs:1185,1194`) currently have `.snap.new` regressions on `fixtures/local/C0242.MP4` because of the new info-issue noise introduced by PR #117.

## Approach
Mirror PR #117's Canon CMT pattern exactly:
1. Add a new `xifty-meta-sony-video` crate (peer of `xifty-meta-canon`/`-fuji`/`-olympus`) with `decode_prof()` + `decode_usmt()` functions and a `SUPPORTED_USERTYPES` registry. Decoders are pure: take `&[u8]`, `base_offset: u64`, `container_name: &str` and return `Vec<MetadataEntry>` with `namespace = "sony_video"`. Field maps reverse-engineered from ExifTool's `Sony.pm` (the `SonyUserData`, `PROF`, `USMT` Tags tables) and `QuickTime.pm` (`UserData` hash) — read for understanding, port to Rust, ship Rust only. Cite ExifTool source line/section in doc comments per derived tag.
2. In `xifty-container-isobmff`, add a `SONY_USERDATA_UUID_TAIL: [u8; 12] = [0x21,0xD2,0x4F,0xCE,0xBB,0x88,0x69,0x5C,0xFA,0xC9,0xC7,0x40]` constant and extend `parse_uuid` (line 1816-1903) so that when `usertype[4..16] == SONY_USERDATA_UUID_TAIL`, an `IsobmffPayload { kind: "sony-video-atom", tag: Some(<atom-name from usertype[0..4] ASCII>), … }` is emitted with the inner data slice (offsets exclude the 16 usertype bytes; data_offset = parsed.data_offset + 16; data_length = parsed.end - inner_start). Add `pub fn sony_video_atoms()` accessor mirroring `canon_cmt_payloads()` (line 150). Refine the residual unrecognized branch to populate a richer issue body (see step 4).
3. In `xifty-cli/src/lib.rs`, plumb dispatch in the ISOBMFF extract paths used by mp4/mov (the `Format::Mp4`/`Format::Mov` branches around the call sites at lines 283-291 and the existing `cr3_extract` at line 772 is the structural template). Iterate `isobmff.sony_video_atoms()`; for each payload match `tag.as_deref()` against `"PROF" | "USMT"` and call the corresponding decoder, with `container_name = "mp4"` or `"mov"` (from the format detected upstream) so provenance reads `container: "mp4"`, `namespace: "sony_video"`.
4. Refine `isobmff_structure_recognized_uninterpreted` (line 1846-1853) so the issue's `message` includes the 16-byte usertype as lowercase hex (e.g. `"recognized uuid box but the usertype 21d24fce…c740 is not handled"`), making future unknown UUIDs actionable.
5. Update `CAPABILITIES.json`: add `"sony_video"` to the top-level `namespaces` map with `status: bounded` and the explicit `supported_tags` list; add `sony_video: bounded` under `containers.mp4.namespaces` and `containers.mov.namespaces`.
6. Regenerate the two failing insta snapshots (`extract_real_camera_mp4_normalized.snap` and `extract_real_camera_mp4_interpreted.snap`) so they show real decoded `sony_video` entries instead of the noise issues; verify by running `cargo insta review` and inspecting diffs.

### Bounded ship list (Phase 1, this PR)

Cinematographer-relevant subset, explicitly bounded:

**PROF atom** (Picture Profile):
- `PictureProfile` (string, e.g. "PP10")
- `GammaCategory` (enum: Movie / Still / Cine1 / Cine2 / ITU709 / ITU709(800) / S-Log2 / S-Log3 / HLG / HLG1/2/3)
- `ColorSpace` (enum: S-Gamut / S-Gamut3 / S-Gamut3.Cine / BT.709 / BT.2020)
- `BlackLevel` (i16)
- `KneePoint` (u8 percent)
- `KneeSlope` (i8)
- `DetailLevel` (i8)
- `ColorMode` (enum)

**USMT atom** (User SMPTE-style metadata):
- `ShootingMode` (enum: Program / Aperture / Shutter / Manual / Movie / etc.)
- `AFMode` (enum: Manual / AF-S / AF-C / AF-A / DMF)
- `WhiteBalanceMode` (enum: Auto / Daylight / Shade / Cloudy / Tungsten / Fluorescent / Flash / ColorTemp / Custom)
- `ColorTemperature` (u16 Kelvin, when WB mode = ColorTemp)

PRPF/MTDT/AOLY are deferred to follow-up issues (recognise the usertype but emit `kind = "sony-video-atom"` with the tag — decoder routes them through; absent decoder branch is a no-op, no info-issue, since they're declared as known). The issue list in `parse_uuid` distinguishes "unknown usertype family" (info-issue with hex) from "known Sony family but no decoder yet" (no issue, since dispatch is silent).

## Files to touch
- `Cargo.toml` (workspace) — register new `crates/xifty-meta-sony-video` member.
- `crates/xifty-container-isobmff/src/lib.rs` — add `SONY_USERDATA_UUID_TAIL`, extend `parse_uuid` (line 1816-1903) with Sony branch, add `sony_video_atoms()` accessor near line 150, enrich the uninterpreted issue body (line 1849).
- `crates/xifty-container-isobmff/Cargo.toml` — no change expected.
- `crates/xifty-cli/Cargo.toml` — add `xifty-meta-sony-video = { path = "../xifty-meta-sony-video" }`.
- `crates/xifty-cli/src/lib.rs` — `use xifty_meta_sony_video::{decode_prof, decode_usmt};`; wire dispatch in the mp4/mov extract path (the same area where quicktime/rtmd entries are accumulated, search for `.quicktime_payloads()` / non_real_time_meta).
- `crates/xifty-cli/tests/cli_contract.rs` — no functional change; snapshot tests at lines 1185/1194 will regenerate.
- `crates/xifty-cli/tests/snapshots/cli_contract__extract_real_camera_mp4_normalized.snap` — regenerate via `cargo insta review`.
- `crates/xifty-cli/tests/snapshots/cli_contract__extract_real_camera_mp4_interpreted.snap` — regenerate via `cargo insta review`.
- `CAPABILITIES.json` — add `sony_video` namespace; mark mp4/mov containers.

## New files
- `crates/xifty-meta-sony-video/Cargo.toml` — peer of `xifty-meta-canon/Cargo.toml`; deps: `xifty-core` only (decoders consume `&[u8]`, no TIFF/source needed since UUID payloads are flat structured big-endian blobs).
- `crates/xifty-meta-sony-video/src/lib.rs` — `decode_prof`, `decode_usmt`, `SUPPORTED_USERTYPES: &[&str]`, internal field-map tables, unit tests.
- `crates/xifty-meta-sony-video/src/prof.rs` — PROF decoder + `GammaCategory`/`ColorSpace`/`ColorMode` enum lookup tables (cite ExifTool `Sony.pm` `%Image::ExifTool::Sony::PictureProfile` / `ProcessSonyPIC` references in comments).
- `crates/xifty-meta-sony-video/src/usmt.rs` — USMT decoder + `ShootingMode`/`AFMode`/`WhiteBalanceMode` lookup tables (cite ExifTool `Sony.pm` `SonyUserData` and `Sony::Tag9050b` mappings).
- `fixtures/minimal/sony_video.mp4` — synthetic minimal ISOBMFF: `ftyp(isom...) + moov(uuid<PROF>{minimal PROF blob} + uuid<USMT>{minimal USMT blob})`. Built with the same `boxed()` helper pattern in the existing isobmff tests (see line 2122-2130 for the Canon synthesis precedent). Committed (synthetic, no copyrighted bytes).

## Step-by-step
1. **Read ExifTool reference** — fetch the relevant tables in `lib/Image/ExifTool/Sony.pm` (`SonyUserData`, `PictureProfile`, `Tag9050b` for AF/WB enums) and `lib/Image/ExifTool/QuickTime.pm` (`UserData` hash) from github.com/exiftool/exiftool. Document the exact line ranges in each decoder's module-level comment so reviewers can audit. **Verifiable**: doc comments cite `Sony.pm` lines.
2. **Create `xifty-meta-sony-video` skeleton** — Cargo.toml mirroring `xifty-meta-canon`'s; empty `lib.rs` exposing `decode_prof`, `decode_usmt`, `SUPPORTED_USERTYPES`; register in workspace `Cargo.toml`. **Verifiable**: `cargo check -p xifty-meta-sony-video`.
3. **Implement PROF decoder** — parse the PROF blob's TLV/fixed-offset layout per ExifTool reference; populate `MetadataEntry` for each tag in the bounded ship list with `namespace="sony_video"`, `tag_id=<atom>:<offset_or_id>`, typed values, and provenance (container name from arg, `path = "moov/uuid/PROF"`). **Verifiable**: unit test in `prof.rs` decodes a hand-built PROF buffer and asserts each field type.
4. **Implement USMT decoder** — same shape; USMT is a SMPTE-keyed structure (4-byte FourCC keys + length + value) per ExifTool. Walk the keys, dispatch the bounded subset, ignore unknowns. **Verifiable**: unit test in `usmt.rs`.
5. **Add `SONY_USERDATA_UUID_TAIL` const + extend `parse_uuid`** — when `usertype[4..16] == SONY_USERDATA_UUID_TAIL`, decode `usertype[0..4]` as ASCII (validate printable), build payload with `kind="sony-video-atom"`, `tag=Some(name)`. The existing Canon branch stays first; the Sony branch sits before the residual `isobmff_structure_recognized_uninterpreted` else-arm. **Verifiable**: existing unknown-UUID test still passes with all-zero usertype; new test asserts a synthetic Sony-PROF UUID surfaces a payload.
6. **Add `sony_video_atoms()` accessor** on `IsobmffParse` near line 150, filtering `payload.kind == "sony-video-atom"`. **Verifiable**: doc-tested or used by the new isobmff unit test.
7. **Enrich the uninterpreted issue body** — lowercase-hex-format the 16-byte usertype into the message (or `context`). Keep code stable (`isobmff_structure_recognized_uninterpreted`). **Verifiable**: existing unknown-UUID test updated to check the hex appears in `message`.
8. **Wire CLI dispatch** in `xifty-cli/src/lib.rs` — for the mp4/mov extract code path, after quicktime entries are collected, iterate `isobmff.sony_video_atoms()`; for each, slice `payload_bytes` (using existing `payload_slice` helper, see line 791), match on `payload.tag.as_deref()`, call decoder, extend `entries`. Pass `container_name = "mp4"` (or `"mov"` for the mov path). **Verifiable**: snapshot tests show `sony_video` entries.
9. **Synthesize `fixtures/minimal/sony_video.mp4`** — emit ftyp + moov with two synthetic UUID boxes (PROF + USMT). Use the same `boxed()` helper from isobmff tests as a model. Add a CLI snapshot test `extract_snapshot_synthetic_sony_video` that exercises end-to-end dispatch without depending on `fixtures/local/`. **Verifiable**: new snapshot file committed.
10. **Regenerate failing snapshots** — run `cargo test -p xifty-cli`, then `cargo insta review` on the two `*real_camera_mp4*` snapshots. Diff should drop the noise info-issues and add `sony_video` entries. **Verifiable**: snapshot diff inspected line-by-line in PR description; no `.snap.new` left over.
11. **Update `CAPABILITIES.json`** — add the `sony_video` namespace with bounded status and the explicit `supported_tags` list; add `sony_video: bounded` to `containers.mp4.namespaces` and `containers.mov.namespaces`. **Verifiable**: `Docs And Contract` hygiene check passes.
12. **Run full validation suite** — see Validation section.

## Tests
- `crates/xifty-meta-sony-video/src/prof.rs` `#[cfg(test)] mod tests` — hand-built PROF byte buffer covering each bounded field; assert one `MetadataEntry` per field, correct typed value, correct enum decoding, correct provenance.
- `crates/xifty-meta-sony-video/src/usmt.rs` `#[cfg(test)] mod tests` — hand-built USMT key/length/value buffer; assert ShootingMode/AFMode/WB decode; assert unknown keys are ignored without panic.
- `crates/xifty-container-isobmff/src/lib.rs` `tests` — extend with `recognizes_sony_userdata_uuid_family()` (synthetic Sony PROF UUID box → one `sony-video-atom` payload with tag `PROF`); update `ignores_unknown_uuid_box()` to assert the issue `message` now contains the usertype hex.
- `crates/xifty-cli/tests/cli_contract.rs` — new `extract_snapshot_synthetic_sony_video` (uses committed `fixtures/minimal/sony_video.mp4`; not gated on `local/`); existing `extract_snapshot_real_camera_mp4_*` regenerated to show real decoded fields.

## Validation
Per `.loswf/config.yaml`:
1. `cargo fmt --all -- --check`
2. `cargo test --workspace --all-features`
3. `cargo test -p xifty-ffi --all-features`

CI gating: `Rust Core`, `Runtime Artifact (macos-arm64)`, `Runtime Artifact (linux-x64)`, `Lambda Node Example`, `Docs And Contract` (this last one validates `CAPABILITIES.json` shape).

Snapshot drift policy: use `cargo insta review` deliberately on the two regenerated `extract_real_camera_mp4_*` snapshots; reviewer must inspect diffs (per SRS §3 invariant 6 and `.loswf/config.yaml` guardrail).

## Risks
- **PROF/USMT field-map accuracy** — without official Sony docs, ExifTool is the de-facto reference. Mitigation: cite source line per tag, keep Phase 1 to the well-attested cinematographer subset, and verify against `C0242.MP4` extracted values cross-referenced with `exiftool C0242.MP4` output (read-only, comparison only — no shipping dep).
- **USMT structure variance across firmware** — different FX bodies emit different USMT layouts. Mitigation: tolerant walker (skip-on-unknown-key, skip-on-bad-length); never panic; emit a per-atom info-issue if the structure is structurally invalid.
- **Atom-name validation in `parse_uuid`** — usertype[0..4] could be non-printable on a malformed UUID that happens to share the tail. Mitigation: require ASCII alphanumeric (3-4 chars), else fall through to the uninterpreted branch with hex.
- **Synthetic fixture realism** — minimal byte buffers may not exercise the real layout. Mitigation: keep `fixtures/local/C0242.MP4` snapshot test as the integration truth; synthetic fixture exists for unit-level dispatch only.
- **Provenance container name** — current ISOBMFF dispatch doesn't always thread the detected format down. Mitigation: pass `"mp4"` vs `"mov"` from the `Format` enum match arm explicitly (the cr3_extract precedent at line 772 hard-codes `"cr3"`; we follow the same shape).
- **Issue body change is a soft contract change** — any downstream consumer keying on the exact message string of `isobmff_structure_recognized_uninterpreted` would break. Code stays stable; only message text changes. Acceptable per the issue's acceptance criteria.

