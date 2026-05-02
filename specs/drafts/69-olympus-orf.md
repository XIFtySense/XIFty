<!-- loswf:plan -->
# Plan #69: RAW — Olympus ORF (TIFF-based, custom magic)

## Problem
Olympus ORF is a TIFF-shaped RAW container with non-standard magic — `IIRO\x08\x00`, `IIRS\x08\x00`, or `MMOR\x00\x08` instead of `II*\x00`/`MM\x00*` (decimal 42). The current detector at `crates/xifty-detect/src/lib.rs:10` only recognizes the standard TIFF magic, so ORF files never reach `xifty-container-tiff`. Even if routed there, `parse_bytes` rejects any non-42 magic at `crates/xifty-container-tiff/src/lib.rs:43-48`. We need a `Format::Orf` arm, a tolerant TIFF parse path that accepts the ORF magic variants (0x4F52 'OR' / 0x5352 'RS' for II, 0x4F52 'OR' for MM), and an Olympus MakerNote decoder mirroring `xifty-meta-sony`. ExifTool ground truth: Olympus.pm — main IFD0 holds standard EXIF; MakerNote header is one of `OLYMP\0\x01\0`, `OLYMPUS\0II\x03\0`, or older `OLYMP\0` followed by an IFD relative to the MakerNote start (newer Olympus uses absolute file offsets — header form determines which).

## Approach
Mirror the `xifty-meta-sony` precedent: extend the existing `xifty-container-tiff` rather than fork a parallel ORF container, since ORF body is byte-for-byte TIFF after the 4-byte magic. Add a public `parse_bytes_with_magic(bytes, base_offset, root_label, accepted_magic: &[u16])` (or simpler: a `parse_orf` helper that wraps `parse_bytes` after rewriting the magic word in a temporary buffer). Add `Format::Orf` to `xifty-core`, ORF detection to `xifty-detect` (must run before the standard TIFF arm — though the patterns are disjoint, so order is just defensive), wire `Format::Orf => tiff_extract(&source, "orf")` in `xifty-cli` plus a new `xifty-meta-olympus` crate decoded into the chain after the standard EXIF/Apple/Sony decoders. Olympus MakerNote tag is the standard EXIF MakerNote tag `0x927C` inside ExifIFD; the new crate consumes the parsed `TiffContainer` plus existing exif entries (like `xifty-meta-sony::decode_from_tiff` at `crates/xifty-meta-sony/src/lib.rs:30`), gates on `Make == "OLYMPUS*"`, parses the MakerNote header variants, walks the embedded IFD, and emits a small starter set of `MetadataEntry { namespace: "olympus_makernote", … }` records. CAPABILITIES.json gains an `orf` container entry.

## Files to touch
- `crates/xifty-core/src/lib.rs` — add `Format::Orf` variant and `as_str` arm (`/Users/k/Projects/XIFty/crates/xifty-core/src/lib.rs:8-40`).
- `crates/xifty-detect/src/lib.rs` — add ORF magic recognition before/alongside TIFF arm; new unit tests for `IIRO`, `IIRS`, `MMOR`.
- `crates/xifty-container-tiff/src/lib.rs` — relax magic check (or add `parse_bytes_with_accepted_magics` / `parse_orf`) so ORF magic words 0x4F52 / 0x5352 (LE) and 0x4F52 swapped (BE: 0x4D4F) are accepted while preserving existing strict `parse_bytes` callers; tests for ORF magic variants.
- `crates/xifty-cli/src/lib.rs` — add `Format::Orf` arms in `probe_source` (`:50`) and `extract_source` (`:134`), routing through `tiff_extract(source, "orf")` and chaining `decode_olympus_from_tiff` after `decode_from_tiff`/`decode_apple_from_tiff` (analogous to `decode_sony_from_tiff` at `:562`).
- `crates/xifty-cli/Cargo.toml` — add `xifty-meta-olympus` dep.
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshots for the synthetic ORF fixture.
- `Cargo.toml` — register `crates/xifty-meta-olympus` in the workspace `members` list (after `xifty-meta-sony` per `Cargo.toml:18`).
- `CAPABILITIES.json` — add `containers.orf` block (mirrors `containers.dng` at `:119-127`).

## New files
- `crates/xifty-meta-olympus/Cargo.toml` — copy `xifty-meta-sony/Cargo.toml` shape (deps: xifty-core, xifty-container-tiff, xifty-source).
- `crates/xifty-meta-olympus/src/lib.rs` — `pub fn decode_from_tiff(bytes, base_offset, container_name, tiff: &TiffContainer, exif_entries: &[MetadataEntry]) -> Vec<MetadataEntry>`; gates on `Make == "OLYMPUS*"` or `"OLYMPUS IMAGING CORP."`; recognizes header magic `OLYMP\0\x01\0` (legacy), `OLYMP\0` (very old), and `OLYMPUS\0II\x03\0` (newer); walks embedded IFD; emits a starter tag set: `MakerNoteVersion (0x0000)`, `CameraType (0x0207)`, `SerialNumber (0x0404 / 0x101A)`, `LensType (Equipment.0x0201)`, `FocalLength`, `Quality (0x0201)` — flagged as "starter; expandable". Per issue ambiguity #3, canonical tag list is TBD; document this in a code comment.
- `crates/xifty-meta-olympus/src/tests.rs` (or inline `#[cfg(test)] mod tests`) — synthesize a TIFF byte buffer carrying an Olympus MakerNote with a couple of entries; assert decoder emits expected `MetadataEntry` records and gates on Make.
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_snapshot_happy_orf` and `extract_snapshot_happy_orf_normalized` (mirror DNG block at `:436-447`).
- `fixtures/minimal/happy.orf` — synthesized minimal ORF (built either as a committed binary blob via a small build-helper test, or generated at test runtime via `optional_fixture` if synthesizing a real ORF is too brittle). Preferred: commit a tiny hand-crafted bytes-only ORF (TIFF body with `IIRO\x08\x00` header + IFD0 with `Make=OLYMPUS`, `Model=E-M1`, MakerNote tag pointing at a small Olympus IFD).

## Step-by-step
1. Add `Orf` variant to `Format` enum and `as_str` in `xifty-core` — `cargo check -p xifty-core` clean.
2. In `xifty-detect`, add a check: if first 4 bytes are `IIRO`, `IIRS`, or `MMOR`, return `Format::Orf`. Place the check before the `II*\0`/`MM\0*` TIFF branch; add three new tests in `tests::detects_formats` (or a new `detects_orf` test) — `cargo test -p xifty-detect` passes.
3. In `xifty-container-tiff`, factor the magic check: keep `parse_bytes` strict (magic == 42), add `pub fn parse_bytes_accepting(bytes, base_offset, root_label, accepted_magics: &[u16]) -> …` (or a thin `parse_orf` wrapper that swaps magic to 42 in a `Vec<u8>` clone). Prefer the explicit-accepted-magics variant for clarity; expose it but keep `parse_bytes` semantics unchanged so all existing callers stay strict. Add tests for `IIRO`/`IIRS`/`MMOR` parse paths — `cargo test -p xifty-container-tiff` passes.
4. Create `crates/xifty-meta-olympus` with Cargo.toml + lib.rs scaffolding modeled on `xifty-meta-sony`. Implement `decode_from_tiff` with header detection and IFD walk; emit a starter set of `MetadataEntry` rows under `namespace = "olympus_makernote"`. Inline `#[cfg(test)]` tests synthesize Make=OLYMPUS + MakerNote bytes and assert outputs.
5. Register the new crate in `Cargo.toml` workspace members.
6. In `xifty-cli/src/lib.rs`, add `Format::Orf` to `probe_source` (label `"orf"`, container parser via the new accepting variant; if simpler, normalize to `tiff` parsing using `parse_bytes_accepting`). Add `Format::Orf => orf_extract(&source)?` arm in `extract_source`, where `orf_extract` is a near-clone of `tiff_extract` (`:541-626`) that uses the accepting parser and also chains `decode_olympus_from_tiff` after the existing exif/apple/sony decoders. Add the `xifty-meta-olympus` dep + `use` import.
7. Add the synthesized `fixtures/minimal/happy.orf` (committed bytes) and the two cli snapshot tests. Run `cargo insta review` to accept new snapshots — never blanket-accept (per `.loswf/config.yaml:57`).
8. Update `CAPABILITIES.json` with an `orf` container entry: `{ "exif": "supported", "olympus_makernote": "bounded" }`. Add `olympus_makernote` to the `namespaces` block as `bounded` with the starter tag list.
9. Verify `Format::Orf` is exhaustively covered in any other `match` over `Format` in the workspace (none in `xifty-ffi` since serialization is via `as_str`, but compiler will surface missing arms).

## Tests
- `crates/xifty-detect/src/lib.rs` (`#[cfg(test)] mod tests`) — three new asserts that `IIRO\x08\x00`, `IIRS\x08\x00`, `MMOR\x00\x08` headers detect as `Format::Orf` and that bare `II*\0` still yields `Format::Tiff`.
- `crates/xifty-container-tiff/src/lib.rs` tests — extend `build_single_entry_tiff` style helpers to emit ORF magic; assert `parse_bytes_accepting` succeeds on each ORF variant and that strict `parse_bytes` still rejects them.
- `crates/xifty-meta-olympus/src/lib.rs` inline tests — synthesize a TIFF with Make=OLYMPUS + MakerNote bytes (header `OLYMP\0\x01\0` and a one-entry IFD), assert decode emits expected `olympus_makernote` entries; assert the Make-gate skips non-Olympus input.
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_snapshot_happy_orf` + `extract_snapshot_happy_orf_normalized` snapshots; reviewed via `cargo insta review`.
- No FFI shape change: `xifty-ffi` test suite must still pass unchanged (`cargo test -p xifty-ffi --all-features`).

## Validation
Per `.loswf/config.yaml:37-43`, in this order:
1. `cargo fmt --all -- --check`
2. `cargo test --workspace --all-features`
3. `cargo test -p xifty-ffi --all-features`

Plus, per project guardrail (`.loswf/config.yaml:57`): manually review insta diffs with `cargo insta review` before accepting; never blanket-accept.

## Risks
- **Magic-check refactor in `xifty-container-tiff` could break strict callers** if `parse_bytes` semantics change. Mitigation: add a new accepting variant; leave `parse_bytes` byte-identical (still requires magic == 42). All existing callers keep importing `parse` / `parse_bytes`.
- **MakerNote offset semantics differ between Olympus header variants.** Legacy `OLYMP\0\x01\0` IFD offsets are relative to the MakerNote start; newer `OLYMPUS\0II\x03\0` uses absolute file offsets. Mitigation: branch on header signature and choose the offset base accordingly; degrade to "skip" with a `Severity::Warning` issue when bytes don't match a known header (no panic).
- **Synthesized vs real ORF fixture.** A hand-rolled minimal ORF may not exercise the full Olympus MakerNote header variants we care about. Mitigation: cover both `OLYMP\0\x01\0` (legacy) and `OLYMPUS\0II\x03\0` (newer) in inline `xifty-meta-olympus` tests via fully-synthesized buffers, even if the committed `happy.orf` only exercises one variant.
- **Issue notes "ambiguity #3: canonical tag list is TBD."** Plan ships a small starter set under `bounded` status in CAPABILITIES.json; future issues can grow the tag list without changing the crate shape.
- **Detector ordering.** ORF magic is disjoint from `II*\0`/`MM\0*`, so ordering in `xifty-detect` doesn't change correctness, but tests must lock in current behavior to prevent a future refactor from accidentally classifying ORF as plain TIFF (unreachable today, but a regression in the magic check could mask it).

