<!-- loswf:plan -->
# Plan #132 (v2): Tier S — C2PA content provenance manifests (image + video)

## Problem
C2PA is the dominant standard for AI-generation provenance and edit history. Microsoft Copilot, Google NotebookLM/Gemini, Adobe Firefly, OpenAI, Leica M11-P, and Sony Alpha 1 II all embed signed JUMBF manifests in their outputs. XIFty has fixtures on disk (`fixtures/local/Copilot_20260407_185229.png`, `fixtures/local/unnamed-6.png`) carrying real `caBX` JUMBF chunks containing `c2pa.actions.v2` assertions with IPTC `digitalSourceType=trainedAlgorithmicMedia`, but no parser exists. Without a `c2pa` namespace XIFty cannot answer "is this AI-generated?", "who signed this?", "what's the edit history?" — the load-bearing creator-wedge questions today.

## Approach
Ship Phase A (parse-only) of C2PA across four containers, mirroring `xifty-meta-ai-gen` (PR #142 — pure-bytes meta crate, `xifty-core + serde_json + ciborium` only, no container deps), `xifty-container-png::profile` (PR #140 — container exposes raw chunk bytes; CLI dispatches to the meta crate), and `xifty-sidecar-gopro` (PR #141). Cryptographic verification stays at level A (`signature.verified = "unknown"`); the field shape is forward-compatible with level B/C. Hand-roll JUMBF (ISO 19566-5 box walker) plus CBOR via `ciborium`. Defer `c2pa-rs` to the level-B follow-up.

**Key revisions vs. plan v1 (per plan-review):**
1. Multi-APP11 EnID-ordered reassembly is performed inside `xifty-container-jpeg`, not the CLI. The container method returns one contiguous reassembled JUMBF byte payload per logical manifest. The CLI iterates ready-to-decode payloads.
2. The cross-cutting derived AI fields are `ai.source_type` and `ai.generator` (bare top-level `ai.*` namespace). `c2pa.ai_generated` stays under `c2pa.*` because it is C2PA-derived and consumers must disambiguate it from XMP-derived AI signals. `CAPABILITIES.json` declares both `c2pa` and `ai` as top-level namespaces; container blocks list both.
3. Synthetic JUMBF fixture is fully specified — explicit C2PA box-type UUIDs (per C2PA Specification 2.0, "Box Type Identifiers"), explicit COSE_Sign1 CBOR shape, byte-counted fixture-generator layout.
4. `python3 tools/generate_capabilities.py --check` is **not** in `.loswf/config.yaml` `validate[]` and we are not adding it (separate scope). The builder runs it locally pre-push; CI's `hygiene.yml` line 41 already enforces it on PR.

## Files to touch
- `Cargo.toml` (workspace root) — register `xifty-meta-c2pa` and `xifty-sidecar-c2pa` as workspace members; add `ciborium = "0.2"` to `[workspace.dependencies]`.
- `CAPABILITIES.json` — add top-level `c2pa` namespace block (claim/signature/assertions/ingredients fields including `c2pa.ai_generated`); add new top-level `ai` namespace block (`ai.source_type`, `ai.generator`, `ai.synthid_disclosed`); add `c2pa_sidecar` namespace block (alias of `c2pa` field set, sidecar provenance). In each carrier container's `containers.{png,jpeg,isobmff}.namespaces`, list both `c2pa` and `ai` as bounded.
- `crates/xifty-container-png/src/lib.rs` — add `pub fn c2pa_payloads(&self) -> impl Iterator<Item = (u64, &[u8])>` filtering chunks where `chunk_type == b"caBX"`. Mirror the shape of `exif_payloads()` exactly (cf. existing `caBX`-adjacent helpers; PR #140 pattern).
- `crates/xifty-container-jpeg/src/lib.rs` — see "JPEG APP11 reassembly" section below. Insert the new `pub fn jumbf_app11_payloads(&self) -> Vec<Vec<u8>>` method on `JpegContainer` after `xmp_payloads()` (currently lines 50–62 in the file). Add `#[cfg(test)] mod tests` at end of file (file is 216 lines; insert at EOF) with two unit tests: single-segment and multi-segment reassembly.
- `crates/xifty-container-isobmff/src/lib.rs` — extend `parse_uuid` (lines 1845–1902) to recognise the C2PA UUID `{0xD8, 0xFE, 0xC5, 0xD8, 0x76, 0xCC, 0xB9, 0xEE, 0x9C, 0xC0, 0x50, 0x4D, 0x8A, 0x16, 0xB1, 0x13}` (ISO 19566-5 §6, also the value in `c2pa-rs`) and emit `IsobmffPayload { kind: "c2pa", tag: Some("uuid_c2pa"), … }`. Add a `c2pa_payloads()` accessor mirroring `canon_cmt_payloads()` (line 161). C2PA branch goes ahead of the Sony fall-through.
- `crates/xifty-cli/src/lib.rs` — import `xifty_meta_c2pa::{decode_jumbf, C2paPayload}` alongside other meta imports (lines 21–53). Wire dispatch:
  - PNG branch (after the `text_payloads` loop, ~line 428): `for (off, bytes) in png.c2pa_payloads() { decode_jumbf(...) }`.
  - JPEG branch: `for jumbf_bytes in jpeg.jumbf_app11_payloads() { decode_jumbf(...) }` — **no reassembly logic in the CLI**.
  - ISOBMFF branch: `for (off, bytes) in parsed.c2pa_payloads() { decode_jumbf(...) }` for `Format::{Mp4, Mov, Heif, Avif, M4a}`.
  - Register `C2paSidecar::new()` in `default_sidecar_registry()` (lines 238–244).

## New files
- `crates/xifty-meta-c2pa/Cargo.toml` — mirrors `crates/xifty-meta-ai-gen/Cargo.toml`. Deps: `xifty-core` (path), `serde_json` (workspace), `ciborium` (workspace). **No container deps.**
- `crates/xifty-meta-c2pa/src/lib.rs` — public surface: `pub struct C2paPayload<'a> { bytes: &'a [u8], container: &'a str, path: &'a str, offset_start: u64, offset_end: u64 }`, `pub fn decode_jumbf(payload: C2paPayload<'_>) -> (Vec<MetadataEntry>, Vec<Issue>)`, `pub use keys::{NS, NS_AI, SUPPORTED_FIELDS, SUPPORTED_FIELDS_AI}`. Modules: `keys`, `jumbf`, `cbor`, `claim`, `assertion`, `signature`, `derive`. Provenance helpers mirror `xifty-meta-ai-gen` (lines 115–145).
- `crates/xifty-meta-c2pa/src/keys.rs` — `NS = "c2pa"`, `NS_AI = "ai"`, `SUPPORTED_FIELDS` listing every flat tag template under `c2pa.*`, `SUPPORTED_FIELDS_AI` listing `ai.*` derived fields. See "Field surface" below.
- `crates/xifty-meta-c2pa/src/jumbf.rs` — pure ISO 19566-5 box walker. Box header `<4-byte LBox big-endian><4-byte TBox>`; LBox=1 means 64-bit XLBox follows. Recognises `jumb` (super-box), `jumd` (description box — UUID + label + toggles), `bfdb` (binary), `cbor` (CBOR data box), `json` (JSON data box). Returns `JumbfBox { ty: [u8;4], label: Option<String>, uuid: Option<[u8;16]>, payload: BoxPayload }` where `BoxPayload = Cbor(&[u8]) | Json(&[u8]) | Children(Vec<JumbfBox>) | Bytes(&[u8])`. **Box identification is UUID-driven** (the label inside `jumd` is advisory; the 16-byte UUID is authoritative). Errors degrade to `Issue { code: "c2pa_jumbf_truncated" | "c2pa_jumbf_malformed", severity: Warning }`. Depth-limited (16) and length-bounded.
- `crates/xifty-meta-c2pa/src/cbor.rs` — thin wrapper over `ciborium::de::from_reader` returning a borrowed-shape `CborValue` enum; provides a CBOR → `serde_json::Value` bridge for assertion-shape access (the signature module reads raw CBOR directly to preserve tag 18).
- `crates/xifty-meta-c2pa/src/claim.rs` — extracts `claim_generator`, `format`, `instance_id`, `assertions[]` index from the `c2pa.claim` / `c2pa.claim.v2` CBOR map (UUID-identified per the table in "Synthetic fixture"). Emits `c2pa.claim_generator`, `c2pa.format`, `c2pa.instance_id`.
- `crates/xifty-meta-c2pa/src/assertion.rs` — walks the assertion store box. For each entry in `actions[]` flattens `label`, `action`, `description`, `digitalSourceType`, `softwareAgent`, `when`. Flat shape with `<i>` numeric infix, mirroring `xifty-meta-ai-gen`'s lora list.
- `crates/xifty-meta-c2pa/src/signature.rs` — extracts COSE_Sign1 (CBOR tag 18) envelope from the `c2pa.signature` box. Parses the protected header bstr → CBOR map → `alg` (RFC 9053 integer labels: -7=es256, -8=eddsa, -35=es384, -36=es512, -37=ps256, -257=rs256). Lifts X.509 leaf cert from unprotected `x5chain` header (label 33). Surfaces `c2pa.signature.alg`, `c2pa.signature.issuer` (CN parsed from DER — minimal walker, no full X.509 crate), and always `c2pa.signature.verified = "unknown"` (TypedValue::String — forward-compatible with level B `"verified" | "invalid" | "untrusted"`). **No crypto verification.**
- `crates/xifty-meta-c2pa/src/derive.rs` — derives `c2pa.ai_generated: bool` from C2PA assertions, plus the cross-cutting `ai.source_type: String`, `ai.generator: String`, and `ai.synthid_disclosed: bool` (when an action description matches `/synthid/i`). IPTC newscode set `{trainedAlgorithmicMedia, compositeWithTrainedAlgorithmicMedia, algorithmicMedia, trainedAlgorithmicData, compositeOfTrainedAlgorithmicMedia}` triggers `c2pa.ai_generated = true` and populates `ai.source_type`. Generator-string heuristic recognises `Google C2PA Core Generator Library`, `Microsoft *`, `Adobe Firefly`, `OpenAI *`, `Leica M11-P`, `Sony Alpha *` and emits short `ai.generator` labels. Confidence: 0.9 with explicit `digitalSourceType`, 0.7 generator-only.
- `crates/xifty-meta-c2pa/tests/synthetic.rs` — unit tests for JUMBF walker + full pipeline against the committed synthetic fixture.
- `crates/xifty-sidecar-c2pa/Cargo.toml` — mirrors `crates/xifty-sidecar-gopro/Cargo.toml`; deps: `xifty-core`, `xifty-sidecar`, `xifty-meta-c2pa`. **No container deps** (unlike GoPro which pulls ISOBMFF for LRV — this adapter delegates straight to `decode_jumbf`).
- `crates/xifty-sidecar-c2pa/src/lib.rs` — `C2paSidecar` implementing `Sidecar`. `discover` returns `<basename>.c2pa` next to the primary (case-insensitive extension, mirroring `xifty-sidecar-gopro::discover_in_dir`). `parse` calls `decode_jumbf` with `container="sidecar"` and re-namespaces returned entries from `c2pa` → `c2pa_sidecar` (entries under `ai.*` keep that namespace; sidecar-stamping touches only the `c2pa.*` ones). `merge_policy() = Complement`, `priority() = 100`.
- `crates/xifty-sidecar-c2pa/tests/discovery.rs` — temp dir with `clip.jpg + clip.c2pa`; assert one `SidecarRef`.
- `fixtures/minimal/c2pa_synthetic.png` — synthetic minimal PNG with one `caBX` chunk wrapping the JUMBF tree spelled out in "Synthetic fixture" below. Bytes are committed.
- `tools/gen_c2pa_fixtures.py` — Python generator producing the synthetic PNG; mirrors `tools/gen_ai_gen_fixtures.py`. Includes the byte layout below as inline literals.
- `crates/xifty-cli/tests/cli_contract.rs` — three new tests appended (existing file): `c2pa_synthetic_png_minimal_fixture` (insta snapshot, always runs), `c2pa_copilot_png_real_fixture` (skip-if-missing, substring assertions, no snap), `c2pa_notebooklm_png_real_fixture` (skip-if-missing, substring assertions, no snap). Skip-if-missing pattern mirrors existing `optional_fixture` helper (used at lines ~1467 and ~2181).

## JPEG APP11 reassembly — implementation contract for `xifty-container-jpeg`
Per ISO 19566-5 §A.4, the JPEG APP11 JUMBF carrier is:
```
<2-byte CI = 'JP' (0x4A 0x50)> | <2-byte En — Entity Identifier, big-endian> | <4-byte Z — sequence number, big-endian> | <4-byte LBox> | <4-byte TBox> | <box payload bytes>
```
Manifests > ~64 KB span multiple APP11 segments: each segment carries the same `En` (manifest identifier) and a strictly increasing `Z` (sequence number, starting at 1). The first segment's `LBox/TBox` describe the full reassembled box; the box payload is split across segments after that 8-byte header in segment 1, and continues raw in segments 2..N.

`pub fn jumbf_app11_payloads(&self) -> Vec<Vec<u8>>`:
1. Iterate `self.segments` filtering `marker == 0xEB`.
2. For each, validate `payload[0..2] == b"JP"`, read `En = u16::from_be_bytes(payload[2..4])`, `Z = u32::from_be_bytes(payload[4..8])`. Slice `box_chunk = &payload[8..]`.
3. Group by `En` into a map `BTreeMap<u16, BTreeMap<u32, Vec<u8>>>` (BTreeMap on Z guarantees enumerator-order).
4. For each `En` group: concatenate all `box_chunk` bytes in ascending `Z` order into a single `Vec<u8>` — that is the contiguous reassembled JUMBF byte stream (LBox/TBox header + payload — exactly what `xifty-meta-c2pa::decode_jumbf` consumes).
5. On malformed input (signature mismatch, gap in Z, length overflow), emit an `Issue` via `self.issues` (mutate via `&self`? no — store as a side `Vec<Issue>` returned alongside, OR append on construction; for an immutable accessor, return `(Vec<Vec<u8>>, Vec<Issue>)` and update CLI to consume both. **Decision: signature mismatches are silently skipped (the segment is not C2PA); only true sequence/length defects emit issues. Since the accessor is `&self`, defects discovered here are returned as `Vec<Issue>` alongside.**

**Final signature:** `pub fn jumbf_app11_payloads(&self) -> (Vec<Vec<u8>>, Vec<Issue>)` — payload-per-manifest plus any reassembly issues. CLI extends its issues vec with the second tuple element before iterating the first.

Tests in `xifty-container-jpeg`'s own test module (synthetic, no external fixtures):
- `app11_single_segment_reassembly`: build SOI + APP11 segment with `JP|En=1|Z=1|<8-byte JUMBF header for a 'jumb' box>|<24 bytes payload>` + EOI. Assert `jumbf_app11_payloads().0` has length 1 and the bytes match the full expected JUMBF stream.
- `app11_multi_segment_reassembly`: same but split across two APP11 segments, En=1, Z=1 then Z=2; intentionally insert them in reverse insertion order. Assert reassembled output equals the concatenation in Z-ascending order.
- `app11_signature_mismatch_skipped`: APP11 with non-JP CI returns empty payload list, no panic, no issue.

## Synthetic JUMBF fixture — explicit box hierarchy with UUIDs
Spec source: **C2PA Specification 2.0, section "Box Type Identifiers"** (https://c2pa.org/specifications/specifications/2.0/specs/_attachments/C2PA_Specification.html). C2PA boxes are JUMBF superboxes (`jumb`) whose identity is fixed by the 16-byte UUID stored in the child `jumd` description box. Labels (`c2pa`, `c2cl`, `c2as`, `c2cs`) are advisory; UUIDs are authoritative.

UUIDs (canonical, per C2PA 2.0 Box Type Identifiers, hex form):
- C2PA top-level superbox: `6332 7061 0011 0010 8000 00aa 0038 9b71` (label `c2pa`)
- Manifest superbox: `6332 6d61 0011 0010 8000 00aa 0038 9b71` (label `c2ma`)
- Claim box (CBOR): `6332 636c 0011 0010 8000 00aa 0038 9b71` (label `c2cl`, content `c2pa.claim` or `c2pa.claim.v2`)
- Assertion store superbox: `6332 6173 0011 0010 8000 00aa 0038 9b71` (label `c2as`)
- Signature box (CBOR / COSE_Sign1): `6332 6373 0011 0010 8000 00aa 0038 9b71` (label `c2cs`, content `c2pa.signature`)
- Generic CBOR data box (assertion content): `6362 6f72 0011 0010 8000 00aa 0038 9b71` (label `cbor`)
- Generic JSON data box: `6a73 6f6e 0011 0010 8000 00aa 0038 9b71` (label `json`)

(Per the C2PA spec the first 4 bytes of each superbox UUID encode the ASCII label — `c`,`2`,`p`,`a` etc. — followed by the JUMBF-base UUID tail `0011 0010 8000 00AA 0038 9B71`. Builder MUST cross-check these constants against `c2pa-rs/sdk/src/jumbf_io.rs` or the spec text before committing; a sanity test in `jumbf.rs` will compare the constants against the reference values.)

Box hierarchy in the synthetic fixture:
```
jumb (top-level C2PA superbox)
├── jumd — UUID = C2PA top-level UUID, label "c2pa"
└── jumb (manifest)
    ├── jumd — UUID = manifest UUID, label "urn:uuid:xifty-test-manifest-0001"
    ├── jumb (claim)
    │   ├── jumd — UUID = claim UUID, label "c2pa.claim.v2"
    │   └── cbor (claim payload)
    │       └── CBOR map: {
    │               "claim_generator": "XIFty Test Suite/0.1.0",
    │               "format": "image/png",
    │               "instance_id": "urn:uuid:00000000-0000-0000-0000-000000000001",
    │               "assertions": [{"url":"self#jumbf=c2pa.assertions/c2pa.actions.v2"}]
    │           }
    ├── jumb (assertion store)
    │   ├── jumd — UUID = assertion-store UUID, label "c2pa.assertions"
    │   └── jumb (one assertion)
    │       ├── jumd — UUID = generic-CBOR UUID, label "c2pa.actions.v2"
    │       └── cbor
    │           └── CBOR map: {
    │                   "actions":[{
    │                       "action":"c2pa.created",
    │                       "digitalSourceType":"http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
    │                       "softwareAgent":"XIFty Test Suite/0.1.0",
    │                       "when":"2026-01-01T00:00:00Z",
    │                       "description":"Created by XIFty Test Suite for synthetic fixture"
    │                   }]
    │               }
    └── jumb (signature)
        ├── jumd — UUID = signature UUID, label "c2pa.signature"
        └── cbor
            └── COSE_Sign1 (CBOR tag 18, array of 4):
                  [
                    protected_bstr,    // bstr containing CBOR-encoded map { 1: -8 }   // alg = EdDSA (RFC 9053)
                    {},                // unprotected map: empty (no x5chain in the synthetic — issuer extraction is exercised against the real Google fixture)
                    h'',               // payload: zero-length bstr (detached signature shape)
                    h'00 * 64'         // signature: 64 zero bytes (Ed25519 sig length; level A does NOT verify)
                  ]
```

Byte layout for `tools/gen_c2pa_fixtures.py` (reproducible):
- Each `jumb` superbox = 4-byte LBox big-endian (total length incl. header) + 4-byte TBox `b"jumb"` + concatenated children.
- Each `jumd` description box = 4-byte LBox + 4-byte TBox `b"jumd"` + 16-byte UUID + 1-byte toggles (0x03 — "label present, requestable") + null-terminated UTF-8 label.
- Each `cbor` data box = 4-byte LBox + 4-byte TBox `b"cbor"` + raw CBOR bytes (encoded with Python's `cbor2`).
- Outer PNG: 8-byte signature `89 50 4E 47 0D 0A 1A 0A` + IHDR (1×1 RGB, fixed) + `caBX` chunk wrapping the top-level `jumb` (length-prefixed, CRC32 of type+data) + IDAT (1 deflated zero pixel) + IEND.
- The script writes byte counts as comments next to each block so reviewers can audit. Total fixture size target: < 1.5 KB.

The script also encodes the COSE_Sign1 array using `cbor2.dumps(cbor2.CBORTag(18, [...]))` so the on-disk bytes start with `D8 12` (tag 18 for COSE_Sign1) followed by a 4-element array. This guarantees `signature.rs` can locate and decode the protected header even though no real key signs the content.

## Step-by-step
1. Add `ciborium = "0.2"` to root `Cargo.toml` `[workspace.dependencies]`. Register `xifty-meta-c2pa` and `xifty-sidecar-c2pa` as workspace members. Outcome: `cargo metadata` succeeds.
2. Create `xifty-meta-c2pa` skeleton: `Cargo.toml`, `src/lib.rs` (re-exports), `src/keys.rs` (`NS = "c2pa"`, `NS_AI = "ai"`, `SUPPORTED_FIELDS`, `SUPPORTED_FIELDS_AI`). Outcome: `cargo build -p xifty-meta-c2pa` succeeds; `decode_jumbf` returns `(vec![], vec![])`.
3. Implement `jumbf.rs` (recursive box walker, depth-limit 16, length-bounded). Outcome: `cargo test -p xifty-meta-c2pa jumbf` passes (6 tests: nested `jumb`, 64-bit XLBox, truncated header, unknown TBox preserved as `Bytes`, depth-limit, length-overflow). Includes a UUID-constants sanity test.
4. Implement `cbor.rs` (`ciborium` + `serde_json::Value` bridge). Outcome: `cargo test -p xifty-meta-c2pa cbor` passes (round-trip + tag-preservation tests).
5. Implement `claim.rs` and `assertion.rs` against inline synthetic JUMBF bytes. Outcome: `cargo test -p xifty-meta-c2pa` covers `c2pa.claim_generator`, `c2pa.format`, `c2pa.assertions.0.label`, `c2pa.assertions.0.action`, `c2pa.assertions.0.digitalSourceType`.
6. Implement `signature.rs` (COSE_Sign1 tag-18 array decode + protected-header alg map per RFC 9053 + minimal DER CN walker — handles UTF8String/PrintableString/BMPString tags). Outcome: `cargo test -p xifty-meta-c2pa signature` passes (alg-mapping table test + DER walker test against a hand-crafted Subject SEQUENCE; failures degrade to `c2pa.signature.issuer = "<DER Subject, N bytes>"` + Issue).
7. Implement `derive.rs` with table-driven tests over IPTC newscode set + generator-string set + SynthID detection. Outcome: `cargo test -p xifty-meta-c2pa derive` passes; emits `c2pa.ai_generated`, `ai.source_type`, `ai.generator`, `ai.synthid_disclosed`.
8. Wire `decode_jumbf` end-to-end. Add `crates/xifty-meta-c2pa/tests/synthetic.rs` loading `fixtures/minimal/c2pa_synthetic.png` (file may not exist yet — test gated on existence, becomes mandatory after step 14).
9. Extend `xifty-container-png::PngContainer` with `c2pa_payloads()` (shape mirrors `exif_payloads()`). Add a chunk-walk test: synthesized `caBX` chunk surfaces in the iterator. Outcome: `cargo test -p xifty-container-png` passes.
10. Extend `xifty-container-jpeg::JpegContainer` with `pub fn jumbf_app11_payloads(&self) -> (Vec<Vec<u8>>, Vec<Issue>)` per the contract above. Insert after `xmp_payloads()` (end of impl block, currently line 62). Add `#[cfg(test)] mod tests` at EOF with three tests listed above (single-segment, multi-segment ordered concat, signature-mismatch skip). Outcome: `cargo test -p xifty-container-jpeg` passes; the multi-segment test asserts the **combined** output, not per-segment bytes.
11. Extend `xifty-container-isobmff::parse_uuid` (lines 1845–1902) to recognise the C2PA UUID before the Sony fall-through; add `c2pa_payloads()` accessor mirroring `canon_cmt_payloads()` (line 161). Add a synthetic ISOBMFF test mirroring the Canon CMT test (~line 2170): build `ftyp + uuid(C2PA UUID + minimal JUMBF)`; assert one `c2pa_payloads()` entry. Outcome: `cargo test -p xifty-container-isobmff` passes.
12. Create `xifty-sidecar-c2pa` (Cargo.toml + lib.rs + tests/discovery.rs). Re-namespacing pass changes only `c2pa.*` entries → `c2pa_sidecar.*`; `ai.*` entries stay as-is. Outcome: `cargo test -p xifty-sidecar-c2pa` passes.
13. Wire dispatch in `xifty-cli/src/lib.rs`:
    - PNG branch (after text-chunk loop, ~line 428): consume `png.c2pa_payloads()` → `decode_jumbf`.
    - JPEG branch: `let (payloads, issues) = jpeg.jumbf_app11_payloads(); cli_issues.extend(issues); for bytes in payloads { decode_jumbf(...) }` — **no reassembly logic in the CLI**.
    - ISOBMFF branch (`Mp4 | Mov | Heif | Avif | M4a`): consume `parsed.c2pa_payloads()` → `decode_jumbf`.
    - Register `C2paSidecar::new()` in `default_sidecar_registry()` (lines 238–244).
    Outcome: `cargo build -p xifty-cli` passes.
14. Run `python3 tools/gen_c2pa_fixtures.py` → writes `fixtures/minimal/c2pa_synthetic.png`. Commit both the script and the resulting bytes. Outcome: file exists; the deferred `tests/synthetic.rs` from step 8 now finds it.
15. Append three tests to `crates/xifty-cli/tests/cli_contract.rs`:
    - `c2pa_synthetic_png_minimal_fixture` (always runs, insta snapshot via `cargo insta review` — accept the new snap).
    - `c2pa_copilot_png_real_fixture` (`optional_fixture("Copilot_20260407_185229.png")`; substring assertions only — no snap).
    - `c2pa_notebooklm_png_real_fixture` (`optional_fixture("unnamed-6.png")`; substring assertions only).
    Outcome: workspace tests pass; snap accepted.
16. Update `CAPABILITIES.json`: add top-level `c2pa` namespace block (with the `supported_tags` template list under "Field surface" below); add new top-level `ai` namespace block (`ai.source_type`, `ai.generator`, `ai.synthid_disclosed`); add `c2pa_sidecar` namespace block (alias of `c2pa` field set, `provenance: "sidecar"`). In `containers.png.namespaces`, `containers.jpeg.namespaces`, and `containers.isobmff.namespaces`, list both `c2pa` and `ai` as `bounded`.
17. Run validation: `cargo fmt --all -- --check`, `cargo test --workspace --all-features`, `cargo test -p xifty-ffi --all-features`. Pre-push, also run `python3 tools/generate_capabilities.py --check` locally — CI's `hygiene.yml` line 41 enforces it on PR open.

## Tests
- Step 3: `crates/xifty-meta-c2pa/src/jumbf.rs` `#[cfg(test)] mod tests` — 6 box-walker tests + UUID-constants sanity test.
- Step 4: `cbor.rs` round-trip + tag-preservation tests.
- Step 5: `claim.rs` + `assertion.rs` unit tests using inline synthetic bytes. Two-action assertion test locks the `<i>` numeric infix.
- Step 6: `signature.rs` DER walker test (hand-crafted Subject SEQUENCE with one CN RDN) + COSE alg-mapping table test.
- Step 7: `derive.rs` parametric tests over `digitalSourceType` set + generator-string set + SynthID-description detection.
- Step 8/14: `crates/xifty-meta-c2pa/tests/synthetic.rs` — full pipeline against the committed synthetic PNG.
- Step 9: `xifty-container-png` chunk-walk test for `caBX`.
- Step 10: `xifty-container-jpeg` three tests (single-segment, multi-segment combined output, signature-mismatch).
- Step 11: `xifty-container-isobmff` C2PA UUID recognition test.
- Step 12: `xifty-sidecar-c2pa/tests/discovery.rs` discover-only.
- Step 15: CLI contract — synthetic insta snap + two real-fixture substring tests:
  - Copilot: `c2pa.claim_generator` matches Microsoft generator, `c2pa.signature.issuer` matches Microsoft chain, `c2pa.ai_generated == true`.
  - NotebookLM: `c2pa.claim_generator == "Google C2PA Core Generator Library"`, signature issuer substring `"Google C2PA Media Services 1P ICA G3"`, `c2pa.assertions.<i>.action` set contains both `c2pa.created` and `c2pa.edited`, `c2pa.ai_generated == true`, `ai.source_type == "trainedAlgorithmicMedia"`, `ai.synthid_disclosed == true`.

## Validation
Authoritative gates from `.loswf/config.yaml`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

Developer-side pre-push check (NOT in `validate[]` — out of scope to add here; CI's `hygiene.yml` line 41 already enforces it on PR):
- `python3 tools/generate_capabilities.py --check`

## Field surface
Flat per `TypedValue` constraint. Two namespaces emitted by this crate:

`c2pa.*` (bounded — C2PA-specific):
- `c2pa.claim_generator`
- `c2pa.format`
- `c2pa.instance_id`
- `c2pa.signature.alg`
- `c2pa.signature.issuer`
- `c2pa.signature.verified` (always `"unknown"` at level A; forward-compat with `"verified" | "invalid" | "untrusted"`)
- `c2pa.assertions.<i>.label`
- `c2pa.assertions.<i>.action`
- `c2pa.assertions.<i>.description`
- `c2pa.assertions.<i>.digitalSourceType`
- `c2pa.assertions.<i>.softwareAgent`
- `c2pa.assertions.<i>.when`
- `c2pa.ingredients.<i>.title`
- `c2pa.ingredients.<i>.format`
- `c2pa.ingredients.<i>.relationship`
- `c2pa.ai_generated` (bool, derived; stays under `c2pa.*` because it is C2PA-asserted — disambiguates from XMP-derived AI signals)

`ai.*` (bounded — cross-cutting; future XMP `Iptc4xmpExt:DigitalSourceType` will populate the same names):
- `ai.source_type` (IPTC newscode value)
- `ai.generator` (short label)
- `ai.synthid_disclosed` (bool)

`c2pa_sidecar.*` re-uses the same `c2pa.*` template list under `Provenance::namespace == "c2pa_sidecar"`, `Provenance::container == "sidecar"`. `ai.*` entries from a sidecar keep the `ai.*` namespace (cross-cutting).

## Cryptographic verification — level decision
Phase A (parse-only) ships in this PR. `c2pa.signature.verified = "unknown"` (TypedValue::String) is forward-compatible with level B `"verified" | "invalid" | "untrusted"` without a schema bump. A follow-up issue will pick between hand-rolling COSE_Sign1 verification vs. depending on `c2pa-rs`.

## `c2pa-rs` decision
Not depended on. JUMBF + CBOR + claim parsing is well-specified; `ciborium` (workspace dep, pure-Rust, MIT/Apache, no_std-capable) handles CBOR. Level-A signature surface needs only protected-header decode + tiny DER CN walker.

## Risks
- **JPEG multi-APP11 reassembly correctness** — the canonical risk for this PR. Mitigation: `xifty-container-jpeg::jumbf_app11_payloads` owns reassembly; explicit two-segment test verifies combined output; out-of-order Z values are sorted by `BTreeMap`.
- **C2PA UUID constants** — the synthetic fixture and the walker share constants. Mitigation: a `jumbf.rs` test compares the constants against the values listed in this plan; if `c2pa-rs` source disagrees the builder MUST flag the discrepancy in a comment on the issue before committing.
- **`caBX` chunk type only** — `cAEX` is out of scope (no real fixture uses it). Documented for follow-up.
- **DER CN extraction edge cases** — UTF8String / PrintableString / BMPString covered; multi-RDN subjects: walker takes the first CN AttributeTypeAndValue inside the Subject SEQUENCE; non-CN-only subjects degrade to a string with byte length and emit an Issue.
- **CBOR tag handling** — `ciborium` preserves tags; `signature.rs` reads raw CBOR (not the JSON bridge) so tag 18 is reachable.
- **CAPABILITIES drift** — synthetic fixture must surface every templated `c2pa.*` and `ai.*` field at least once via the `<i>` prefix matcher. Mitigation: synthesizer covers one assertion (covers `c2pa.assertions.<i>.*`), one signature (covers `c2pa.signature.*`), and emits `ai.source_type` + `ai.generator` (covers `ai.*`). `c2pa.ingredients.<i>.*` templates are listed but the synthetic fixture omits ingredients; `generate_capabilities.py --check` matches templates by prefix so unused templates are tolerated. The two real fixtures still exercise breadth on local machines.
- **Empty `caBX` chunk** — `decode_jumbf` returns `(vec![], vec![Issue { code: "c2pa_jumbf_empty", severity: Info }])`.
- **macOS case-insensitive sidecar** — `clip.JPG` next to `clip.c2pa` matches via case-insensitive extension comparison.

