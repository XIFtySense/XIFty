<!-- loswf:plan -->
# Plan #64: RAW — Nikon NEF (TIFF-based, encrypted maker-note graceful)

## Problem
Nikon's NEF RAW format is a TIFF container with a Nikon-specific MakerNote (EXIF tag `0x927C`) under IFD0/ExifIFD. Newer Nikon bodies (D2X+) ship a partially-encrypted MakerNote: the wrapper IFD parses, but several payload regions are obfuscated by a serial-number-keyed XOR scheme that XIFty must NOT attempt to decrypt at v1. We need detection (`Format::Nef`), routing through the existing TIFF parser, plain-EXIF surfacing, a new `xifty-meta-nikon` decoder for plain MakerNote tags, an explicit non-fatal `Issue` for encrypted regions, and CLI/CAPABILITIES wiring — all without breaking the FFI shape.

## Approach
Mirror the existing Sony precedent (`crates/xifty-meta-sony/src/lib.rs:1-100`, `crates/xifty-cli/src/lib.rs:550-568`) end-to-end, with one **intentional divergence** from Sony: the Nikon decoder must be able to surface `Issue`s for encrypted regions, which Sony does not need.

**Resolved decision (was a blocker in prior plan): Option B — decoder returns `Vec<MetadataEntry>` only. Encrypted-region `Issue`s are emitted by the CLI caller (`tiff_extract`) using the existing `namespace_issue(...)` pattern at `crates/xifty-cli/src/lib.rs:1033`.** This keeps the metadata-crate signature identical to Sony and confines `Issue` plumbing to the CLI layer, matching how ICC/IPTC/XMP empty-decode issues are emitted at lines 578-584, 597-602, 616-621. To make this work, the decoder exposes a second small public function — `encrypted_regions(...) -> Vec<EncryptedRegion>` — returning a list of `{ tag_id: u16, absolute_offset: u64 }` records that the CLI iterates to push one Issue per region. (Alternative considered: an out-param `&mut Vec<EncryptedRegion>` on `decode_from_tiff`. Rejected: keeps signature out of parity with Sony's `&[MetadataEntry]`-style read-only inputs and forces the CLI to allocate the vec up front.)

End-to-end flow:
1. Add `Format::Nef` to `xifty-core` and detect via TIFF magic + IFD0 `Make` tag (`0x010F`) value beginning with `NIKON CORPORATION` or `NIKON` (extending the same IFD0 scan that already recognizes DNG via tag `0xC612`, see `crates/xifty-detect/src/lib.rs:64-112`).
2. Reuse `xifty-container-tiff` unchanged — it already exposes IFD0/ExifIFD entries and lets a metadata crate find the MakerNote tag (Sony does the same at `crates/xifty-meta-sony/src/lib.rs:48`).
3. Create `xifty-meta-nikon` as a metadata-only crate (boundary rule from `.loswf/config.yaml` guardrails) that:
   - Detects the Nikon MakerNote signature (`"Nikon\0"` legacy or `"Nikon\x00\x02..."` TIFF-in-TIFF v2/v3 header).
   - Parses the inner IFD with bounds checks (Sony-style helpers).
   - Decodes a small bounded set of *plain* tags only (Quality `0x0004`, FocusMode `0x0007`, FlashSetting `0x0008`, WhiteBalance `0x0005`, Sharpness `0x0006`, LensType `0x0083` BYTE bitfield — final list narrowed at build time, kept conservative for v1).
   - Internal `is_encrypted_tag(tag_id, version_bytes)` skips encrypted blocks (no partial decode, never reads past version preamble) and records them in an internal vector exposed via the public `encrypted_regions(...)` function.
4. CLI: route `Format::Nef` through `tiff_extract(&source, "nef")` (the helper at `crates/xifty-cli/src/lib.rs:541`) and call `xifty_meta_nikon::decode_nikon_from_tiff(...)` alongside the existing Sony/Apple decoders. Then call `xifty_meta_nikon::encrypted_regions(...)` and push one `namespace_issue("nikon_makernote_encrypted_region", ...)` per region into the local `issues` vec **before** the `Ok(...)` return at line 625.
5. Detect: extend the TIFF branch to inspect IFD0 for `Make` tag and choose `Format::Nef` over `Format::Tiff`. Keep DNG precedence (DNG check first — DNG can technically also have NIKON `Make` for old bodies, and DNG is more specific than NEF).
6. Capabilities + workspace + FFI: add `nikon` namespace to `CAPABILITIES.json`; register `xifty-meta-nikon` in `Cargo.toml` workspace members; FFI shape is unchanged (no new C symbols, no enum value exposed across the boundary — `Format` is internal).

## Files to touch
- `crates/xifty-core/src/lib.rs` — add `Format::Nef` variant and `"nef"` mapping in `as_str()` (lines 8-40).
- `crates/xifty-detect/src/lib.rs` — extend the TIFF branch (lines 10-15) to read IFD0 `Make` tag and return `Format::Nef`; add a helper `is_nef_tiff(bytes: &[u8]) -> bool` mirroring `is_dng_tiff` (lines 64-112); add detection unit tests beside the existing DNG ones (lines 282-326).
- `crates/xifty-cli/src/lib.rs` —
  - Add import alias at line 26 (next to existing Sony alias): `use xifty_meta_nikon::{decode_from_tiff as decode_nikon_from_tiff, encrypted_regions as nikon_encrypted_regions};` — **explicitly aliased** to avoid collision with the unqualified `decode_from_tiff` from `xifty_meta_exif` already imported at line 17, matching the existing `decode_sony_from_tiff` (line 26) and `decode_apple_from_tiff` (line 16) precedent.
  - Add `Format::Nef` arms in `probe_source` (lines 50-99) and `extract_source` (lines 134-216), routing through `tiff_extract(&source, "nef")`.
  - In `tiff_extract` (line 541): after the Sony call (lines 562-568), call `decode_nikon_from_tiff(...)` and `extend(entries)` with its result; then call `nikon_encrypted_regions(...)` and for each `EncryptedRegion`, push a `namespace_issue("nikon_makernote_encrypted_region", "Nikon MakerNote tag 0x{:04X} is encrypted; skipping per v1 policy", region.absolute_offset, "ifd0_makernote")` into `issues` (the local vec already aggregated for the existing namespace issues — this is the same pattern as lines 578-584).
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshot tests for the NEF fixture (mirrors `extract_snapshot_big_endian_tiff_normalized` at line 190).
- `Cargo.toml` — add `crates/xifty-meta-nikon` to workspace members (insert after `xifty-meta-sony`, line 18).
- `CAPABILITIES.json` — add a `"nikon"` namespace block under `namespaces` with `status: "partial"` and a short note about encrypted regions; advertise NEF as a recognized format.

## New files
- `crates/xifty-meta-nikon/Cargo.toml` — same shape as `crates/xifty-meta-sony/Cargo.toml` (deps: `xifty-core`, `xifty-container-tiff`, `xifty-source`).
- `crates/xifty-meta-nikon/src/lib.rs` — public surface:
  - `pub fn decode_from_tiff(bytes: &[u8], base_offset: u64, container: &str, tiff: &TiffContainer, exif_entries: &[MetadataEntry]) -> Vec<MetadataEntry>` — **signature matches Sony exactly** (`crates/xifty-meta-sony/src/lib.rs:30`). Returns plain-tag entries only. Silent on failure (returns empty).
  - `pub struct EncryptedRegion { pub tag_id: u16, pub absolute_offset: u64 }` — public so the CLI can iterate it.
  - `pub fn encrypted_regions(bytes: &[u8], base_offset: u64, tiff: &TiffContainer) -> Vec<EncryptedRegion>` — re-walks the MakerNote IFD with the same bounds-checked parser, returning encrypted-tag locations without decoding anything. Silent on failure (returns empty).
  - Doc comment must explicitly state v1 stop point: "encrypted MakerNote regions are surfaced via `encrypted_regions(...)`; the CLI emits `nikon_makernote_encrypted_region` Issues. XIFty does not attempt decryption."
  - Internal helpers: `parse_nikon_header`, `parse_inner_ifd`, `decode_plain_tag_*`, `is_encrypted_tag(tag_id, version_bytes) -> bool`.
- `crates/xifty-meta-nikon/src/lib.rs` unit tests (inline `#[cfg(test)] mod tests`): synthetic minimal Nikon MakerNote IFDs covering: (a) plain tag round-trip, (b) `encrypted_regions` returns exactly one record for a synthetic encrypted block and `decode_from_tiff` skips it (no panic), (c) truncated MakerNote returns empty entries and empty regions with no panic.
- `crates/xifty-meta-nikon/tests/integration.rs` — integration test that constructs a TIFF with a Nikon MakerNote containing both a plain tag and a synthetic "encrypted" tag block, asserts the plain entry is decoded AND `encrypted_regions` reports the block at the expected absolute offset.
- `fixtures/minimal/happy.nef` — minimal synthetic NEF (same construction style as `happy.dng`, with `Make=NIKON CORPORATION`, `Model=NIKON D850`, plain MakerNote header + one encrypted block placeholder). Hand-curated bytes checked in, mirroring how other minimal fixtures are pre-built. **Must exist on disk before step 9 runs `cargo test -p xifty-cli`**, otherwise the new snapshot tests will fail to load the fixture.
- `crates/xifty-cli/tests/snapshots/cli_contract__probe_happy_nef.snap` and `..__extract_happy_nef_full.snap` — emitted on first `cargo insta accept`; reviewer must hand-review per guardrail.

## Step-by-step
1. Add `Format::Nef` to `xifty-core::Format` enum and `as_str()` mapping. Verify with `cargo build -p xifty-core`.
2. Implement `is_nef_tiff(bytes)` in `xifty-detect` — read IFD0 entries, find `Make` tag (`0x010F`, type 2 ASCII), resolve out-of-line if needed, compare prefix (case-insensitive) against `b"NIKON"`. Update `detect()` to return `Format::Nef` after the DNG check. Add three unit tests: NEF detection succeeds, plain TIFF still returns `Format::Tiff`, malformed Make-tag offset still falls back to `Format::Tiff` (matches `malformed_tiff_ifd_offset_falls_back_to_tiff` at line 314).
3. Scaffold `crates/xifty-meta-nikon` with `Cargo.toml` and `src/lib.rs`. Add to workspace `Cargo.toml`. `cargo build -p xifty-meta-nikon` succeeds.
4. **Prerequisite (research-needed before coding `is_encrypted_tag`)**: Cross-reference ExifTool's `lib/Image/ExifTool/Nikon.pm` (the authoritative open-source Nikon MakerNote reference) to confirm or correct the encrypted tag-ID list. **Confirmed encrypted (well-established on D2X+)**: `0x0091` (ShotInfo) and `0x0097` (LensData, version-byte gated). **Uncertain (must verify before inclusion)**: `0x0098` (LensData distinct ID — may not exist; ExifTool's encryption logic for LensData is gated on the `0x0097` version byte, not a separate `0x0098` tag) and `0x00A8` (FlashInfo at this exact ID — needs ExifTool confirmation). **Default plan**: ship v1 with the **confirmed pair only** (`0x0091`, `0x0097` when version byte `>= 0x02`). If ExifTool research during the build confirms `0x0098`/`0x00A8` as real encrypted tags, add them with a code comment citing the ExifTool source line. If unconfirmed, leave them out — adding a non-encrypted tag to the skip-list would silently hide plain data; missing a real encrypted tag would at worst surface garbage as an unrecognized payload, which is preferable.
5. Implement Nikon MakerNote header detection. Recognize:
   - Legacy v1: `b"Nikon\x00\x01\x00"` followed by IFD at offset +8.
   - TIFF-in-TIFF v2/v3: `b"Nikon\x00\x02"` + version + `\x00\x00` + a fresh `II*\0` / `MM\0*` header at +10. Inner offsets are relative to that inner TIFF header — record `inner_base_offset` correctly so `value_offset_absolute` math works.
   - Anything else: return empty (decoder is silent on failure, mirroring Sony). Encrypted-region tracking also returns empty.
6. Walk the inner IFD with the same bounds-checked pattern as `xifty-container-tiff::walk_ifd` (lines 84-185). Collect `MakerEntry` records mirroring `xifty-meta-sony` lines 22-28.
7. For each entry, dispatch:
   - **Plain decoders (v1 set, conservative)**: Quality `0x0004` (ASCII), WhiteBalance `0x0005` (ASCII), Sharpness `0x0006` (ASCII), FocusMode `0x0007` (ASCII), FlashSetting `0x0008` (ASCII), LensType `0x0083` (BYTE bitfield). Emit `MetadataEntry { namespace: "nikon", ... }` with `Provenance` matching the Sony pattern.
   - **Encrypted blocks (skip + record)**: For tags returning `true` from `is_encrypted_tag(tag_id, version_bytes)` (set finalized in step 4), do not partially decode encrypted bytes — never read past the version preamble. Push `EncryptedRegion { tag_id, absolute_offset }` into the internal regions vec (returned by `encrypted_regions(...)`).
   - Unknown tag id: ignored silently (matches Sony behavior).
8. **Build the synthetic minimal NEF fixture (`fixtures/minimal/happy.nef`) FIRST, before CLI wiring tests run.** Hand-curated bytes (preferred, since other minimal fixtures are pre-built). Construction: little-endian TIFF, IFD0 with `Make="NIKON CORPORATION\0"`, `Model="NIKON D850\0"`, `DateTime`, ExifIFD pointer, and a MakerNote tag (`0x927C`) referencing a tiny inline MakerNote payload that contains: Nikon TIFF-in-TIFF header + inner IFD with one plain tag (Quality=`"FINE\0"`) and one synthetic "encrypted" tag (`0x0091` ShotInfo with version preamble `0x0204`). **Step 9 cannot proceed until this file exists on disk.**
9. Wire the decoder into the CLI. Add the import alias at line 26: `use xifty_meta_nikon::{decode_from_tiff as decode_nikon_from_tiff, encrypted_regions as nikon_encrypted_regions};`. In `tiff_extract` (line 541), after the existing Sony call (line 562-568), call `decode_nikon_from_tiff(source.bytes(), 0, container_label, &tiff, &entries)` and extend `entries`. Then call `nikon_encrypted_regions(source.bytes(), 0, &tiff)` and push one `namespace_issue(...)` per region into `issues`. Add `Format::Nef => tiff_extract(&source, "nef")?` to the `extract_source` match and `Format::Nef => { let parsed = parse_tiff(&source)?; ("nef".to_string(), parsed.nodes, parsed.issues) }` to `probe_source`. Add CLI snapshot tests, then run `cargo test -p xifty-cli` once, then `cargo insta accept` on the new snapshots after manual diff inspection (per guardrail "never blanket-accept").
10. Add `xifty-meta-nikon` integration test asserting plain entry decode + encrypted region detection.
11. Update `CAPABILITIES.json` to add the `"nikon"` namespace block and a top-level format note. Mirror the existing namespace status conventions (`bounded`, `partial`).
12. Verify FFI surface is untouched: `cargo test -p xifty-ffi --all-features` passes without snapshot regression. No `FFI_CONTRACT.md` change needed (no new C symbols, no enum-on-the-wire change — `detected_format` is a string field already).
13. Run the full validation gate from `.loswf/config.yaml`.

## Tests
- `crates/xifty-detect/src/lib.rs` `#[cfg(test)] mod tests` — three new unit tests for NEF detection (positive, negative, malformed Make).
- `crates/xifty-meta-nikon/src/lib.rs` inline tests — header parsing variants, plain tag round-trip, `encrypted_regions` returns one record for a synthetic encrypted block while `decode_from_tiff` skips it, truncated payload returns empty for both functions.
- `crates/xifty-meta-nikon/tests/integration.rs` — end-to-end via `xifty-container-tiff::parse_bytes` plus `decode_from_tiff` + `encrypted_regions` against a synthesized TIFF (no real RAW required for CI).
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_snapshot_happy_nef` and `extract_snapshot_happy_nef_full` (insta JSON snapshots), per acceptance criterion "Insta snapshots for CLI probe + extract". **Depends on `fixtures/minimal/happy.nef` existing (step 8).**
- Existing snapshot tests must not change — new format is purely additive.

## Validation
Per `.loswf/config.yaml` `validate[]`, in order:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- **Encrypted tag-ID research outcome (step 4)**: if ExifTool confirms `0x0098`/`0x00A8` are not standalone encrypted tags, the v1 ship list stays at `0x0091` + `0x0097`. If a real-world NEF has an additional encrypted tag we miss, plain-decoding it would yield garbage values; this is acceptable for v1 (non-fatal) and recoverable in a follow-up issue. The opposite (treating plain as encrypted) hides data behind a Warning Issue — also non-fatal but worse UX, so we err on the conservative side.
- **Nikon MakerNote header variants**: there are at least three known headers (`"Nikon\0\x01..."`, `"Nikon\0\x02..."` with inner TIFF, and a few legacy headerless layouts). v1 handles the two common ones; anything else returns empty silently (matching Sony's silent-failure mode). This is acceptable per the issue's "skip-and-continue" rule.
- **Inner-TIFF base offset arithmetic**: NEF v2/v3 stores offsets relative to the inner Nikon TIFF header, not the outer file. Getting this wrong would land payloads at wrong absolute offsets. Mitigation: explicit `inner_base_offset = outer_maker_note_offset + 10` plumbed through every read; integration test asserts at least one decoded payload offset matches the expected absolute byte and that `EncryptedRegion.absolute_offset` is computed in outer-file coordinates (not inner-relative).
- **Fixture availability**: no NEF in `fixtures/minimal/`. Synthesizing a byte-accurate NEF without a real camera body has the usual risk of "this passes our test but a real NEF reveals a missed code path". Mitigation: structure the synthetic fixture from documented Nikon MakerNote layouts; opportunistically run the decoder against any real `.NEF` users provide via `fixtures/local/` (gitignored).
- **DNG-vs-NEF detection precedence**: a Nikon-shot DNG (rare, e.g. via Adobe DNG Converter) has both `DNGVersion` and `Make=NIKON`. Plan keeps DNG check first so those files stay `Format::Dng` — matches the existing precedent that DNG is a more specific TIFF flavor than any vendor RAW.
- **CAPABILITIES `status`**: Nikon namespace is partial (decoded plain tags only). Use `status: "partial"` with a `notes` field describing the encrypted-region policy so downstream tooling can render it correctly.

