<!-- loswf:plan -->
# Plan #136: PNG `Raw profile type *` decoder family — extend to APP1/exif/xmp/icc/icm/APP13 (revised)

## Problem
PNG files transcoded from JPEG by ImageMagick / libvips / GraphicsMagick (and Google's NotebookLM pipeline) embed the original JPEG profile segments inside `tEXt`/`zTXt`/`iTXt` chunks keyed `Raw profile type <name>`. The body is hex-encoded ASCII wrapping the raw segment bytes. XIFty already decodes the `iptc` and `8bim` variants of this family — see `decode_png_iptc_payload` at `crates/xifty-cli/src/lib.rs:1424` and the test fixture routing at `crates/xifty-container-png/src/lib.rs:188`. The remaining variants (`APP1`, `exif`, `xmp`, `icc`, `icm`, `APP13`) are silently dropped, so EXIF/XMP/ICC payloads are invisible on `fixtures/local/unnamed-6.png` and on any JPEG-→-PNG round-trip through ImageMagick.

## Approach
Adopt option **(B)** from the issue thread: relocate the `Raw profile type *` family from `xifty-cli` into `xifty-container-png` so non-CLI consumers (WASM, FFI, Node binding) inherit the dispatch. Two firm boundaries shape this plan:

1. **Container crate stays container-only.** `xifty-container-png` will gain a new `profile` module that does **hex-frame decoding only** and returns **raw bytes** in a typed `RawProfilePayload` enum. It will **not** depend on, import, or call `xifty-meta-exif`, `xifty-meta-xmp`, `xifty-meta-iptc`, `xifty-meta-icc`, or `xifty-container-tiff`. All `Cargo.toml` deltas to `xifty-container-png` are limited to `flate2.workspace = true`. This is the explicit `.loswf/config.yaml` guardrail: container parsing and metadata interpretation must remain in separate crates.
2. **Meta dispatch stays in `xifty-cli`.** The existing PNG arm in `crates/xifty-cli/src/lib.rs:339–491` already wires `eXIf` → `xifty_container_tiff::parse_bytes` → `xifty_meta_exif::decode_from_tiff`, `iCCP` → `xifty_meta_icc::decode_payload`, and `iptc_payloads()` → `xifty_meta_iptc::decode_payload`. The new dispatch reuses those exact call sites by feeding them `RawProfilePayload` variants. No new crate boundary is crossed.

The mirrored pattern is the JPEG container at `crates/xifty-container-jpeg/src/lib.rs:20–62`: `exif_payload`, `icc_payloads`, `iptc_payloads`, `xmp_payloads` each return raw bytes with the carrier-specific signature stripped, leaving the CLI to dispatch.

## Files to touch
- `crates/xifty-container-png/Cargo.toml` — add `flate2.workspace = true`. No `xifty-meta-*` or `xifty-container-tiff` deps.
- `crates/xifty-container-png/src/lib.rs` — declare `pub mod profile;` and re-export `RawProfileKind`, `RawProfilePayload`, `classify_raw_profile_keyword`, `decode_raw_profile`. Extend the existing `routes_text_chunks_for_iptc` test with sibling cases.
- `crates/xifty-cli/src/lib.rs` — replace the ad-hoc PNG `Raw profile type *` loops in lines 339–491 with a single dispatcher driven by `xifty_container_png::decode_raw_profile`. Delete the now-redundant private helpers at lines 1287 (`decode_png_iccp_payload`), 1313 (`decode_png_text_payload`), 1424 (`decode_png_iptc_payload`), 1490 (`decode_imagemagick_raw_profile`), 1513 (`hex_digit`).
- `CAPABILITIES.json` — bump `formats.png.namespaces.iptc` from `bounded` to `supported`.

## New files
- `crates/xifty-container-png/src/profile.rs` — `RawProfileKind`, `RawProfilePayload`, `classify_raw_profile_keyword`, `decode_raw_profile`. **Returns raw bytes only. No meta crate calls. No TIFF parser calls.**
- `tools/gen_raw_profile_fixtures.py` — Python stdlib-only generator (`zlib`, `struct`, `hashlib`/CRC) that builds the synthetic `Raw profile type APP1` PNG. One-off, not wired into CI.
- `fixtures/minimal/raw_profile_app1.png` — committed output of the script above; APP1 EXIF blob wrapped in hex framing inside a `zTXt` chunk keyed `Raw profile type APP1`.
- `crates/xifty-cli/tests/png_raw_profile.rs` — integration test asserting the synthetic fixture surfaces an EXIF entry with `provenance.container = "png"`, `provenance.path` starting `png_profile_app1`, and `notes` mentioning `Raw profile type`.

## Step-by-step

1. **Add `flate2` to `xifty-container-png/Cargo.toml`.** Verifiable: `grep flate2 crates/xifty-container-png/Cargo.toml` matches; `cargo build -p xifty-container-png` succeeds. **Do not add any `xifty-meta-*` or `xifty-container-tiff` dep.**

2. **Create `crates/xifty-container-png/src/profile.rs` (raw bytes only).** Define:
   ```
   pub enum RawProfileKind { App1, Exif, Xmp, Icc, Icm, Iptc, App13, EightBim }
   pub enum RawProfilePayload {
       Exif(Vec<u8>),       // TIFF stream — Exif\0\0 prefix already stripped
       Xmp(Vec<u8>),        // XMP packet  — http://ns.adobe.com/xap/1.0/\0 prefix already stripped
       Iptc(Vec<u8>),       // IIM or Photoshop 3.0/8BIM IRB — passed through as-is
       Icc(Vec<u8>),        // ICC profile bytes — passed through as-is
       App1Unknown(Vec<u8>),// APP1 with unrecognised signature
   }
   pub fn classify_raw_profile_keyword(keyword: &str) -> Option<RawProfileKind>;
   pub fn decode_raw_profile(keyword: &str, hex_body: &[u8]) -> Option<RawProfilePayload>;
   ```
   The module imports only `flate2` (already imported elsewhere) and stdlib. **It does not import any `xifty_meta_*` or `xifty_container_tiff` symbol.** Behaviour:
   - `decode_raw_profile` strips the `\n<keyword>\n  <len>\n` framing, hex-decodes the body, then for `App1` sniffs the first bytes:
     - if it `starts_with(b"Exif\0\0")` → `RawProfilePayload::Exif(bytes[6..].to_vec())` (mirrors `crates/xifty-container-jpeg/src/lib.rs:22-23`).
     - if it `starts_with(b"http://ns.adobe.com/xap/1.0/\0")` (29 bytes) → `RawProfilePayload::Xmp(bytes[29..].to_vec())` (mirrors `crates/xifty-container-jpeg/src/lib.rs:52-57`).
     - otherwise → `RawProfilePayload::App1Unknown(bytes)`.
   - `Exif`/`Icc`/`Icm`/`Iptc`/`App13`/`EightBim` keywords map directly to the matching `RawProfilePayload` variant with the hex-decoded bytes as-is (no signature stripping — those carriers don't have a JPEG-style marker prefix in the raw profile body).
   - Unit tests: APP1+EXIF sniff (verifies `Exif\0\0` is stripped before return), APP1+XMP sniff (verifies the 29-byte adobe namespace prefix is stripped), APP1+unknown (`App1Unknown`), `exif`/`xmp`/`iptc`/`icc`/`icm`/`APP13`/`8bim` direct mappings, ImageMagick framing (`\n<len>\n`), libvips framing (`\n  <len>\n` with leading spaces), odd-length hex rejected, unknown keyword returns `None`. Verifiable: `cargo test -p xifty-container-png` passes new tests.

3. **Re-export from `crates/xifty-container-png/src/lib.rs`.** Add `pub mod profile;` and `pub use profile::{RawProfileKind, RawProfilePayload, classify_raw_profile_keyword, decode_raw_profile};`. Extend `routes_text_chunks_for_iptc` with sibling assertions for each new keyword. Verifiable: `cargo check -p xifty-container-png`.

4. **Refactor `crates/xifty-cli/src/lib.rs:339–491` PNG arm to dispatch on `RawProfilePayload`.** The new loop iterates the PNG container's text chunks once; for each, calls `xifty_container_png::decode_raw_profile(keyword, body)` and matches:
   - `RawProfilePayload::Exif(bytes)` — feed into `xifty_container_tiff::parse_bytes(&bytes, 0)` then `xifty_meta_exif::decode_from_tiff(...)` (this is the same call shape as the existing `eXIf` chunk handling at lines 342–354). The bytes returned here are already a clean TIFF stream because step 2 stripped the `Exif\0\0` prefix. Provenance: `container = "png"`, `path = "png_profile_<keyword>"`, `notes` includes `decoded from PNG zTXt 'Raw profile type <keyword>' chunk`.
   - `RawProfilePayload::Xmp(bytes)` — call `xifty_meta_xmp::decode_packet(&bytes, ...)` directly (the 29-byte adobe namespace prefix is already stripped in step 2, so the bytes are a raw XMP packet starting with `<?xpacket` or `<x:xmpmeta`). Provenance: as above.
   - `RawProfilePayload::Iptc(bytes)` — call `xifty_meta_iptc::decode_payload(&bytes, ...)` (handles both bare IIM and `Photoshop 3.0`/8BIM IRB transparently — see `crates/xifty-meta-iptc/src/lib.rs:13`). Provenance: as above.
   - `RawProfilePayload::Icc(bytes)` — call `xifty_meta_icc::decode_payload(&bytes, ...)`. Provenance: as above.
   - `RawProfilePayload::App1Unknown(bytes)` — emit a `png_profile_app1_unknown` `Issue` at `Severity::Info` and drop the bytes; do not silently swallow.

   All meta dispatch lives here in `xifty-cli`, exactly as the existing `eXIf`/`iCCP`/`iptc_payloads` paths do. **No meta crate is added to `xifty-container-png`.** Verifiable: existing snapshot suite (including `iptc.png`) stays green under `cargo test -p xifty-cli`; `cargo insta review` only shows additive entries on the new fixture.

5. **Delete the now-redundant private helpers** at lines 1287, 1313, 1424, 1490, 1513 of `crates/xifty-cli/src/lib.rs`. Verifiable: `cargo build --workspace` clean, no `dead_code` warnings.

6. **Write `tools/gen_raw_profile_fixtures.py`** — Python stdlib only (`zlib`, `struct`, `binascii`). Reads the EXIF segment bytes from `fixtures/minimal/happy.jpg` (skip past JPEG `FFD8` and the APP1 marker; payload starts with `Exif\0\0` and the TIFF header), wraps them as `\nAPP1\n  <decimal-len>\n<hex>\n`, zlib-compresses, and writes a minimal valid PNG (`IHDR` 1×1 grayscale + `zTXt` keyword `Raw profile type APP1` + `IDAT` + `IEND`) with correct CRC32s on every chunk. Document at the top of the script: "One-off generator for issue #136 fixture; not wired into CI; rerun manually if `happy.jpg` changes." Verifiable: `python3 tools/gen_raw_profile_fixtures.py` writes `fixtures/minimal/raw_profile_app1.png`.

7. **Commit `fixtures/minimal/raw_profile_app1.png`** as the canonical CI fixture. Verifiable: `xifty inspect fixtures/minimal/raw_profile_app1.png` surfaces EXIF entries.

8. **Add `crates/xifty-cli/tests/png_raw_profile.rs`** integration test: load `fixtures/minimal/raw_profile_app1.png`, run `probe_source`, assert at least one entry has `namespace = "exif"`, `provenance.container = "png"`, and `provenance.path` starting with `png_profile_app1`, and `provenance.notes.contains("Raw profile type")`. Verifiable: `cargo test -p xifty-cli png_raw_profile` passes.

9. **Update `CAPABILITIES.json`**: `formats.png.namespaces.iptc` `bounded` → `supported`. The `exif`/`xmp`/`icc` entries already read `supported` and cover the new path. Verifiable: `cargo test --workspace` stays green; any insta deltas are intentional.

10. **Run full validation** (Validation section).

## Tests
- `crates/xifty-container-png/src/profile.rs` — unit tests covering each `RawProfileKind`, hex framing edge cases, **explicit assertion that APP1+`Exif\0\0` returns bytes with the prefix stripped**, **explicit assertion that APP1+adobe-namespace returns bytes with the 29-byte XMP prefix stripped**, ImageMagick vs libvips framing, odd-length hex rejection.
- `crates/xifty-container-png/src/lib.rs` — extend `routes_text_chunks_for_iptc` with sibling cases for `APP1`, `exif`, `xmp`, `icc`, `icm`, `APP13`, `8bim`.
- `crates/xifty-cli/tests/png_raw_profile.rs` — integration test on synthetic `raw_profile_app1.png`: EXIF tag count > 0, namespace `exif`, provenance container `png`, path prefix `png_profile_app1`.
- Existing snapshot suite (`crates/xifty-cli/src/snapshots/`) — must remain green for `iptc.png` (regression guard for the in-place migration).

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

(All three from `.loswf/config.yaml` `validate[]`.)

## Risks
- **Snapshot drift on `iptc.png`** — the dispatch call site moves; provenance strings must remain byte-identical. Mitigation: keep the existing `path` strings (`"png_iptc"`, `"png_8bim"`, etc.) when emitting via the new container API; reviewer runs `cargo insta review` deliberately.
- **APP1 prefix stripping correctness** — if step 2 forgets to strip `Exif\0\0` (6 bytes) or the 29-byte adobe XMP namespace prefix, downstream `xifty_container_tiff::parse_bytes` and `xifty_meta_xmp::decode_packet` will fail on otherwise-valid data. Unit tests in step 2 explicitly cover both strips.
- **APP1 unknown signatures** — non-EXIF/non-XMP APP1 segments (Flashpix, Stim) currently have no decoder. Plan emits an `Info`-severity `png_profile_app1_unknown` issue rather than silently dropping. No data loss because the bytes are inspectable via `xifty inspect --raw`.
- **`flate2` workspace dep** is already declared at `Cargo.toml:59` (workspace root). Promoting it into `xifty-container-png` is additive and slightly grows the WASM/FFI surface — acceptable, every text-chunk decoder needs zlib.
- **Local fixture `fixtures/local/unnamed-6.png`** is git-ignored per `.loswf/config.yaml index.exclude` (`fixtures/local/`) and cannot be the canonical CI fixture. The synthetic `fixtures/minimal/raw_profile_app1.png` is. Local fixture remains a manual smoke test.
- **Hex framing variants** — libvips uses `\n  <len>\n` (leading spaces); ImageMagick uses `\n<len>\n`. Step 2 unit tests cover both.

