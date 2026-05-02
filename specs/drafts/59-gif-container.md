<!-- loswf:plan -->
# Plan #59: GIF container (LSD, frame count, animation duration, palette, XMP App Ext)

## Problem
XIFty has no GIF support. Issue #59 requires a native `xifty-container-gif` crate that parses the GIF header, Logical Screen Descriptor, image/graphic-control blocks to count frames and sum delays, detects the Netscape loop extension, and recognizes the `XMP Data` Application Extension so the existing `xifty-meta-xmp` decoder can surface XMP entries. Probe/extract must detect `GIF87a`/`GIF89a`, surface dimensions + frame count + animation duration + palette size, and the normalize layer must expose a new `animation.frame_count` field. `CAPABILITIES.json` must mark `gif` as `bounded` and declare `animation.frame_count`.

## Approach
Mirror the chunk-oriented pattern used by `crates/xifty-container-png/src/lib.rs` (`parse_bytes` walking blocks, emitting `ContainerNode`s, collecting typed payload descriptors and `Issue`s) and the RIFF chunk approach in `xifty-container-riff` for iterating variable-length blocks. Extend `xifty_core::Format` with `Gif`, add `GIF87a`/`GIF89a` magic detection in `xifty-detect`, wire the new container into `xifty-cli::extract_source` so XMP blocks feed `decode_packet(XmpPacket{…, container: "gif", …})` from `xifty-meta-xmp` (no change required to that crate), and publish media scalar entries (`ImageWidth`, `ImageHeight`, `FrameCount`, `AnimationDurationSeconds`, `GlobalPaletteSize`, `LoopCount`) on a new `gif` namespace. In `xifty-normalize`, add a small pass mapping the `gif`/`FrameCount` entry to `animation.frame_count`. FFI stays unchanged — GIF rides the existing JSON probe/extract surface.

## Files to touch
- `Cargo.toml` (lines 2-30) — register `crates/xifty-container-gif` in workspace members.
- `crates/xifty-core/src/lib.rs` (lines 6-36) — add `Format::Gif` and `as_str => "gif"`.
- `crates/xifty-detect/src/lib.rs` (lines 4-49, tests 111-211) — add `GIF87a`/`GIF89a` magic branch and extend the `detects_formats` test with GIF fixtures.
- `crates/xifty-cli/Cargo.toml` — add dep on `xifty-container-gif`.
- `crates/xifty-cli/src/lib.rs` (imports around lines 1-29, match arms 46-87 and 117-499) — add `Format::Gif` probe and extract arms that parse the GIF, emit media scalar entries, and route `xmp_payloads()` through `decode_packet(XmpPacket{…})`.
- `crates/xifty-cli/tests/cli_contract.rs` (pattern at lines 133-221) — add `probe_snapshot_happy_gif`, `probe_snapshot_animated_gif`, `extract_snapshot_happy_gif_report`, `extract_snapshot_animated_gif_normalized`, `extract_snapshot_xmp_gif_normalized`.
- `crates/xifty-normalize/src/lib.rs` (after dimensions block around lines 12-29) — add `animation.frame_count` enrichment reading the `gif`/`FrameCount` entry.
- `crates/xifty-normalize/tests/*` (or inline `#[cfg(test)]`) — cover frame_count normalization.
- `CAPABILITIES.json` — add `gif` container entry (`xmp: bounded`) and append `animation.frame_count` to `normalized_fields`.

## New files
- `crates/xifty-container-gif/Cargo.toml` — new crate manifest matching `xifty-container-png/Cargo.toml` (depends on `xifty-core`, `xifty-source`).
- `crates/xifty-container-gif/src/lib.rs` — `GifContainer { nodes, blocks, global_color_table_size, frame_count, animation_duration_centiseconds, loop_count, xmp_payloads, issues }` plus `parse` / `parse_bytes`.
- `fixtures/minimal/happy.gif` — static 1x1 GIF89a (GCT, single image descriptor, trailer `;`).
- `fixtures/minimal/animated.gif` — 2-frame GIF89a with graphic-control delays (e.g. 50 + 50 = 1.00s) and Netscape `NETSCAPE2.0` loop extension.
- `fixtures/minimal/xmp.gif` — GIF89a with XMP Application Extension (`XMP Data` identifier + magic trailer) carrying a minimal XMP packet.
- `crates/xifty-cli/tests/snapshots/cli_contract__probe_happy_gif.snap` (+ sibling snapshots, written by `cargo insta review`).

## Step-by-step
1. Add `Format::Gif` to `xifty-core` and update `as_str` — `cargo check -p xifty-core` clean.
2. Add GIF magic detection + tests in `xifty-detect` for both `GIF87a` and `GIF89a` — `cargo test -p xifty-detect` passes.
3. Scaffold `crates/xifty-container-gif` (Cargo.toml + lib.rs skeleton) and register it in workspace `Cargo.toml` — `cargo build -p xifty-container-gif` compiles.
4. Implement `parse_bytes`: validate signature/version, read the 7-byte Logical Screen Descriptor (width u16le, height u16le, packed byte, bg index, aspect), derive `global_color_table_size = 3 * 2^((packed & 0x07) + 1)` when GCT flag set, skip over the GCT bytes, then walk blocks recognizing:
   - `0x2C` Image Descriptor (10 header bytes; skip local color table if flag set; skip LZW min-code-size byte; consume sub-blocks until `0x00` terminator; increment frame count).
   - `0x21` Extension: sub-type byte — `0xF9` graphic control (delay in centiseconds at offset 2..4 little-endian, accumulate into animation duration), `0xFF` application extension (11-byte identifier; dispatch on `NETSCAPE2.0` to read loop count from sub-block and on `XMP Data\x01` to capture the concatenated sub-block payload as an XMP packet — remember the GIF XMP magic trailer of 258 bytes must be stripped), `0xFE`/`0x01` comment/plain-text (skip sub-blocks).
   - `0x3B` Trailer — terminate.
   - Emit `ContainerNode`s labeled `lsd`, `gct`, `image_descriptor`, `graphic_control_ext`, `app_ext:<id>`, `trailer`. Record byte-range issues (truncated header, unterminated sub-blocks, bad signature) rather than panicking. Verifiable via unit tests in the new crate covering: minimal static GIF, 2-frame animated GIF, GIF with XMP App Ext, GIF with unterminated sub-block (issue surfaced).
5. Wire `Format::Gif` through `xifty-cli::probe_source` and `xifty-cli::extract_source`, producing `gif` namespace `MetadataEntry`s for `ImageWidth`, `ImageHeight`, `FrameCount`, `GlobalPaletteSize`, and when `frame_count > 1` also `AnimationDurationSeconds` (centiseconds / 100.0) and `LoopCount`; route XMP sub-block bytes through `decode_packet(XmpPacket{…, container: "gif"})` — verified by new CLI insta snapshots.
6. Add an `animation.frame_count` normalization step in `xifty-normalize` that lifts the `gif`/`FrameCount` integer entry into a `NormalizedField` with provenance from the underlying entry — verified by normalize unit test and CLI normalized snapshot.
7. Add fixtures under `fixtures/minimal/` (`happy.gif`, `animated.gif`, `xmp.gif`) constructed as inline byte vectors in a generator or committed directly; keep under ~1 KB each.
8. Extend `crates/xifty-cli/tests/cli_contract.rs` with the new snapshot tests following the patterns at lines 134-221; accept snapshots via `cargo insta review` (document in PR).
9. Update `CAPABILITIES.json` — add `containers.gif = { namespaces: { xmp: "bounded" } }` and append `"animation.frame_count"` to `normalized_fields`.
10. Run the full validation gate locally.

## Tests
- `crates/xifty-container-gif/src/lib.rs` `#[cfg(test)] mod tests` — static GIF parse, animated GIF frame/delay accounting, Netscape loop decode, XMP App Ext extraction with magic-trailer stripping, truncated/malformed input issue emission.
- `crates/xifty-detect/src/lib.rs` tests — extend `detects_formats` to cover both `GIF87a` and `GIF89a`.
- `crates/xifty-normalize` tests — `animation.frame_count` emitted when `gif`/`FrameCount` entry present, absent otherwise.
- `crates/xifty-cli/tests/cli_contract.rs` — insta snapshots for probe + extract on `happy.gif`, `animated.gif`, `xmp.gif` (report + normalized views).

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- XMP App Ext sub-block reassembly: the 258-byte "magic trailer" required by Adobe's spec must be stripped before handing to `xifty-meta-xmp`; otherwise the XMP parser may silently reject the packet. Cover with explicit unit test.
- Crafting minimal-but-valid animated and XMP GIF fixtures by hand is fiddly (sub-block terminators, packed LSD byte); use inline byte vectors first, then commit the resulting files so fixtures stay reproducible.
- Snapshot drift across platforms: ensure fixture paths are scrubbed (existing `scrub_path` in `cli_contract.rs`) and that we do not leak absolute offsets that depend on fixture size changes.
- Local color tables, interlace flag, and plain-text extensions are uncommon but present in the wild — skip-only handling is fine for `bounded` status but must not panic.
- `animation.frame_count` is a new normalized field; `schemas/xifty-analysis-*.schema.json` is permissive about `normalized.fields` shape, but confirm the hygiene schema check still passes (the Validation gate will catch it).
