<!-- loswf:plan -->
# Plan #63: RAW — Canon CR3 (ISOBMFF-based, CMT1/CMT2 boxes)

## Problem
Canon's CR3 RAW format replaces the TIFF-based CR2 with an ISOBMFF container that carries EXIF data inside Canon-specific UUID boxes (`CMT1`/`CMT2`/`CMT3`/`CMT4`) under `moov/uuid`. Today `crates/xifty-detect/src/lib.rs` has no `crx ` brand branch and `crates/xifty-core/src/lib.rs` has no `Format::Cr3`, so a CR3 file either fails detection or misroutes through the generic `Format::Mp4`/`Format::Mov` ISOBMFF path which never surfaces CMT* payloads. We need a first-class `Format::Cr3`, a CR3 brand check ordered before `is_mp4_brand` (because `crx ` ftyp boxes routinely list `isom` as a compatible brand), and box-walk extensions in `xifty-container-isobmff` that emit the CMT TIFF payloads so they can be routed to `xifty-meta-exif` and `xifty-meta-canon` (introduced by #62, which this issue depends on).

## Approach
Extend the existing `xifty-container-isobmff` crate rather than spinning up a new container crate — CR3 is plain ISOBMFF box-walking with extra knowledge of (1) the `uuid` box header (16-byte extended type after the standard 8-byte box header) and (2) one Canon-specific UUID whose payload contains `CMT1`/`CMT2`/`CMT3`/`CMT4` sub-boxes that are TIFF-shaped (Endian + IFD0). The 16-byte Canon CMT UUID is `85 c0 b6 87 82 0f 11 e0 81 11 f4 ce 46 2b 6a 48` per ExifTool's `lib/Image/ExifTool/Canon.pm` (cited reverse-engineering reference). Dispatch is split deliberately:

- **CMT1** is emitted as `IsobmffPayload { kind: "exif", tag: Some("CMT1"), ... }` so the existing `exif_payloads()` filter (line 48) picks it up and the existing CLI dispatch reuses `xifty_container_tiff::parse_bytes` + `decode_from_tiff` (mirrors HEIF/MP4 at `crates/xifty-cli/src/lib.rs:454`).
- **CMT2/3/4** are emitted as `IsobmffPayload { kind: "canon-cmt", tag: Some("CMT2"|"CMT3"|"CMT4"), ... }`. A new helper `pub fn canon_cmt_payloads(&self) -> impl Iterator<Item = &IsobmffPayload>` is added alongside `exif_payloads()` filtering on `kind == "canon-cmt"`. The CLI iterates these and calls the existing single-entry `decode_from_tiff` from `xifty-meta-canon` (imported as `decode_canon_from_tiff` mirroring lines 16 and 26) once per payload, threading a distinct `container_name` ("cr3-cmt2"/"cr3-cmt3"/"cr3-cmt4") so downstream provenance distinguishes each IFD.

The existing `decode_canon_from_tiff` API from #62 is reused unchanged — no new multi-IFD overload. Detection adds an `is_cr3_brand` check inside the `ftyp` block (`crates/xifty-detect/src/lib.rs:40-53`) **before** the `is_mov_brand`/`is_mp4_brand` calls because `crx ` ftyp commonly lists `isom` as a compatible brand and would otherwise get swallowed by `is_mp4_brand` (which matches `isom` at line 163).

This plan is **blocked on #62**; it must not begin building until `xifty-meta-canon` exists and is wired into `Cargo.toml` with `decode_from_tiff` exposed.

## Files to touch
- `crates/xifty-core/src/lib.rs` — add `Format::Cr3` variant and `as_str()` mapping at lines 8-21 and 24-39.
- `crates/xifty-detect/src/lib.rs` — add `Format::Cr3` branch in the `ftyp` block at line 40, gated by a new `is_cr3_brand` (matching `b"crx "` major brand). Branch must precede `is_mov_brand` (line 44) and `is_mp4_brand` (line 50). Add unit test alongside `detects_formats` (line 196).
- `crates/xifty-container-isobmff/src/lib.rs` — extend `parse_box_header` (line 494) to optionally read the 16-byte `usertype` for `uuid` boxes; extend the box dispatch in `parse_children` (line 320, after `b"iref"` arm at line 473) with a `b"uuid"` arm that recognizes the Canon CMT UUID and walks its inner CMT* sub-boxes. Add `canon_cmt_payloads()` helper alongside `exif_payloads()` (line 48). Add `"canon-cmt"` to the documented set of `kind` discriminants on `IsobmffPayload` (line 8). Add box-level unit tests next to existing `parses_minimal_heif` tests.
- `crates/xifty-cli/src/lib.rs` — add `Format::Cr3` arms to `probe_source` (line 71) and `extract_source` (line 454). Import `xifty_meta_canon::decode_from_tiff as decode_canon_from_tiff` next to lines 16/26. Container label `"cr3"`.
- `crates/xifty-cli/Cargo.toml` — depend on `xifty-meta-canon` (added in #62).
- `crates/xifty-cli/tests/cli_contract.rs` — add `probe_snapshot_happy_cr3` and `extract_snapshot_happy_cr3_normalized` tests (mirroring `probe_snapshot_happy_heic` line 149 and `extract_snapshot_happy_mov_normalized` line 381).
- `CAPABILITIES.json` — add a `cr3` namespace entry with `status: "planned"` (deferred to `bounded` per #62's kstore Canon RAW tag list gate).

## New files
- `fixtures/minimal/happy.cr3` — synthetic minimal CR3 fixture: `ftyp` with major brand `crx ` + compatible brands `crx ` and `isom`, `moov` containing one `uuid` box with the Canon CMT UUID and tiny `CMT1` (TIFF II*\0 + IFD0 with `Make=Canon`, `Model`, `DateTimeOriginal`) and `CMT2` (TIFF with at least one ExifIFD-style entry, e.g. `ExposureTime`). Built using the existing `boxed`/`full_box` helpers in `crates/xifty-container-isobmff/src/lib.rs:1576-1589`.
- `crates/xifty-cli/tests/snapshots/cli_contract__probe_happy_cr3.snap` — generated by insta first run, reviewed manually.
- `crates/xifty-cli/tests/snapshots/cli_contract__extract_happy_cr3_normalized.snap` — generated by insta.

## Step-by-step
1. **Block until #62 lands.** Confirm `crates/xifty-meta-canon/src/lib.rs` exists and exports `pub fn decode_from_tiff(...)`. Verifiable: `cargo metadata --format-version 1 | jq '.packages[].name' | grep xifty-meta-canon` returns a hit.
2. Add `Format::Cr3` to `crates/xifty-core/src/lib.rs` (variant + `as_str() = "cr3"`). Verifiable: `cargo check -p xifty-core`.
3. Add `is_cr3_brand` and a CR3 branch to `crates/xifty-detect/src/lib.rs`, **placed before** `is_mov_brand` (line 44) and `is_mp4_brand` (line 50). Rationale: `crx ` ftyp commonly lists `isom` as a compatible brand which would otherwise be matched by `is_mp4_brand` (line 163 includes `isom`). The brand match accepts major brand `b"crx "` (ASCII space at byte 11). Add `detects_cr3` and `detects_cr3_with_isom_compat` tests. Verifiable: `cargo test -p xifty-detect`.
4. Extend `crates/xifty-container-isobmff/src/lib.rs::parse_box_header` (line 494) to read the extra 16-byte `usertype` for `b"uuid"` boxes (data_offset advances by 16). Add a `usertype: Option<[u8; 16]>` field to `ParsedBox`. Add a `b"uuid"` arm in `parse_children` (after `b"iref"` at line 473) that, when `usertype == CANON_CMT_UUID`, walks the inner box layout: each CMT* sub-box (FourCC `CMT1`/`CMT2`/`CMT3`/`CMT4`) wraps a TIFF payload. Dispatch:
   - `CMT1` → `IsobmffPayload { kind: "exif", tag: Some("CMT1"), ... }` — picked up by existing `exif_payloads()` filter at line 48 with no further changes.
   - `CMT2`/`CMT3`/`CMT4` → `IsobmffPayload { kind: "canon-cmt", tag: Some("CMT2"|"CMT3"|"CMT4"), ... }` — picked up by the new `canon_cmt_payloads()` helper.
   Add `pub fn canon_cmt_payloads(&self) -> impl Iterator<Item = &IsobmffPayload>` alongside `exif_payloads()` at line 48 filtering `kind == "canon-cmt"`. Cite ExifTool `Image::ExifTool::Canon` in the file's module-level doc comment. Verifiable: a synthetic CR3 byte vector parses; `payloads.iter().any(|p| p.kind == "exif" && p.tag.as_deref() == Some("CMT1"))` and `payloads.iter().any(|p| p.kind == "canon-cmt" && p.tag.as_deref() == Some("CMT2"))` both true.
5. Add `Format::Cr3` arms to `crates/xifty-cli/src/lib.rs`:
   - In `probe_source` (line 71 area): `parse_isobmff(&source)?` and label container `"cr3"`.
   - Add import `use xifty_meta_canon::decode_from_tiff as decode_canon_from_tiff;` alongside lines 16 and 26.
   - In `extract_source` (line 454 area): call `parse_isobmff`, then for each payload from `exif_payloads()` (this naturally surfaces CMT1) run `xifty_container_tiff::parse_bytes(...)` + `decode_from_tiff(..., "cr3", &tiff)` + `decode_canon_from_tiff(payload_bytes, payload.data_offset, "cr3", &tiff, &exif_entries)` (mirrors how `decode_sony_from_tiff` is invoked). For each payload from `canon_cmt_payloads()` (CMT2/3/4) parse as TIFF and call `decode_canon_from_tiff(payload_bytes, payload.data_offset, container_name, &tiff, &exif_entries)` once per payload, where `container_name` is `"cr3-cmt2"` / `"cr3-cmt3"` / `"cr3-cmt4"` derived from `payload.tag`. No new multi-IFD overload — one `decode_from_tiff` call per CMT payload.
   Verifiable: extract on `happy.cr3` returns nonzero entries with `namespace == "exif"` and `namespace == "canon"`.
6. Build the synthetic `fixtures/minimal/happy.cr3` using the `boxed`/`full_box` test helpers at lines 1576-1589 of `crates/xifty-container-isobmff/src/lib.rs`. Verifiable: `xifty probe fixtures/minimal/happy.cr3` outputs `detected_format: cr3`.
7. Add insta snapshot tests to `crates/xifty-cli/tests/cli_contract.rs` (probe + extract normalized) mirroring lines 149 and 381. Verifiable: `cargo insta review` shows new snapshots; manually accept after reviewing.
8. Update `CAPABILITIES.json`: add `"cr3": { "status": "planned", "notes": "ISOBMFF-based; CMT1 EXIF surfaces; Canon maker-note structurally parsed" }`.
9. Confirm no FFI shape change — `xifty-ffi` does not enumerate `Format` variants beyond `as_str()` strings, so adding `Cr3` is ABI-neutral. Verifiable: `cargo test -p xifty-ffi --all-features`.
10. Run full validate suite and review snapshot diffs deliberately (no blanket `--accept`).

## Tests
- `crates/xifty-detect/src/lib.rs` — `detects_cr3` (major brand `crx `) and `detects_cr3_with_isom_compat` (major `crx ` + compat `isom` to assert ordering does not fall through to `is_mp4_brand`).
- `crates/xifty-container-isobmff/src/lib.rs` — `parses_minimal_cr3_with_cmt1_payload` (asserts CMT1 emits one `kind == "exif"`, `tag == Some("CMT1")` payload at the correct `data_offset`/`data_length`); `parses_minimal_cr3_with_cmt2_payload` (CMT2 → `kind == "canon-cmt"`, `tag == Some("CMT2")`); `ignores_unknown_uuid_box` (non-Canon UUID payload skipped without parse error, only `isobmff_structure_recognized_uninterpreted` info issue logged).
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_snapshot_happy_cr3`, `extract_snapshot_happy_cr3_normalized`.
- Regression: confirm `extract_snapshot_happy_mov_normalized` and `extract_snapshot_happy_heic_normalized` snapshots are unchanged (validates UUID-box parser does not regress non-CR3 ISOBMFF).

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- **Dependency order.** Builder must not start until #62 has merged on `main` and `xifty-meta-canon::decode_from_tiff` is on `main`. Subagents run in isolated worktrees branched from `main`; if #62 is still in flight, builder halts with needs-clarification rather than stub the dispatch.
- **Synthetic fixture realism.** The fixture has only CMT1 + CMT2 with one or two IFD0 entries each. Real CR3 files include CMT3/CMT4 plus a substantial mdat that we never need to decode. The fixture is the smallest legal `crx ` ftyp + `moov` + `uuid` (Canon UUID) + `CMT1` + `CMT2`. Adding CMT3/CMT4 is optional but cheap; include them if the builder finds it tractable.
- **UUID-box parser regression risk for non-CR3 ISOBMFF files.** Many MP4/MOV files carry `uuid` boxes for unrelated payloads (XMP-via-uuid, Sony A55 UUID, GoPro `uuid`s). The new parser must (a) read the 16-byte usertype safely with bounds checks, (b) only act on the Canon CMT UUID, (c) leave existing brand routing untouched. Snapshot regression on `happy.mov`/`happy.mp4`/`happy.heic` is the gate.
- **CMT box layout reverse-engineering.** ExifTool documents CMT1-4 as TIFF-formatted sub-boxes inside the Canon UUID box payload. The boundary between "uuid box payload IS a TIFF" vs "uuid box payload contains CMT* sub-boxes which each are TIFF" is the latter per ExifTool `Canon.pm`. Builder must verify against `Canon.pm` before writing the inner walker; halt if a real CR3 sample disagrees.
- **CAPABILITIES.json status gate.** Per #62 plan-reviewer ruling, `bounded` requires a kstore-defined canonical Canon RAW tag list. Ship as `planned`; promotion deferred to kstore.
- **No `decode_canon_from_tiff` multi-IFD overload.** All CMT2/3/4 dispatch happens via the single existing `decode_from_tiff` entry point per payload. If the builder discovers that CMT3 (preview IFD) or CMT4 (additional Canon-specific IFD) require structural handling beyond what `xifty-meta-canon`'s single-IFD entry provides, halt and file a follow-up issue rather than expanding the API in this scope.

