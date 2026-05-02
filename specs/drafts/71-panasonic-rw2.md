<!-- loswf:plan -->
# Plan #71: RAW — Panasonic RW2 (TIFF-like, nonstandard tag IDs)

## Problem
Panasonic RW2 is a TIFF-shaped RAW container with a modified magic (`IIU\x00`, where `U` = 0x55 replaces TIFF's 0x2A) and a **nonstandard IFD0 tag numbering scheme** — Panasonic tags 0x0001..0x002d collide with standard TIFF tag IDs (0x0001..0x002d are unassigned in TIFF, but the same numeric range is used elsewhere by EXIF/TIFF and by the existing `xifty-container-tiff` + `xifty-meta-exif` pipeline, which assumes standard TIFF semantics for any IFD it walks). Consequently, RW2 cannot reuse the standard TIFF route: detection must produce a distinct `Format::Rw2`, and the container parser must segregate Panasonic tags so they reach a new `xifty-meta-panasonic` crate without ever passing through `xifty-meta-exif::decode_from_tiff`. Today (`crates/xifty-detect/src/lib.rs:6-13`) only recognizes `II*\0` / `MM\0*`, and `crates/xifty-container-tiff/src/lib.rs:43-48` rejects any magic other than `42`.

## Approach
Mirror the existing TIFF / Sony pattern but with a strict separation. (1) Add `Format::Rw2` and `Format::as_str() => "rw2"` in `xifty-core` (`crates/xifty-core/src/lib.rs:8-40`). (2) Add an `IIU\x00` little-endian magic check in `xifty-detect` placed **before** the standard TIFF branch so RW2 is never misrouted (sibling pattern at `crates/xifty-detect/src/lib.rs:10-15` for DNG-vs-TIFF). (3) Create `xifty-container-rw2`: a near-clone of `xifty-container-tiff::parse_bytes` that accepts magic 0x55, walks IFD0 entries with the same 12-byte entry layout, but tags every walked entry with `ifd_name: "panasonic_ifd0"` and **does not follow** the standard 0x8769 (ExifIFD) / 0x8825 (GPS IFD) sub-IFD pointers from IFD0 (since those tag IDs do not mean "EXIF" in Panasonic's namespace). It still surfaces `Exif Sub-IFD` only when an explicit Panasonic-specified pointer is found (TBD per ambiguity #3 — for this task we treat IFD0 as Panasonic-only). (4) Create `xifty-meta-panasonic` whose `decode_from_rw2(bytes, base_offset, container_name, &Rw2Container) -> Vec<MetadataEntry>` interprets a small bounded set of Panasonic tags (0x0001 PanasonicRawVersion, 0x0002 SensorWidth, 0x0003 SensorHeight, 0x0004 SensorTopBorder, 0x0005 SensorLeftBorder, 0x0006 SensorBottomBorder, 0x0007 SensorRightBorder, 0x0009 CFAPattern, 0x000a BlackLevelRed, 0x0024 WBRedLevel, 0x0025 WBGreenLevel, 0x0026 WBBlueLevel, 0x002e JpgFromRaw — bounded list, doc-commented). (5) CLI gets `Format::Rw2` arms in `probe_source` and `extract_source` paralleling the `Tiff/Dng` arms (`crates/xifty-cli/src/lib.rs:55-62, 216-217, 541-626`); a new `rw2_extract` helper mirrors `tiff_extract` but routes only through `xifty-meta-panasonic`, never `xifty-meta-exif`. (6) `CAPABILITIES.json` gets a `containers.rw2` entry with `panasonic: bounded`.

The architectural rule "container parsing and metadata interpretation must remain in separate crates" is upheld: the container crate emits `Rw2Entry` records; the meta crate interprets them.

## Files to touch
- `Cargo.toml` — add `crates/xifty-container-rw2` and `crates/xifty-meta-panasonic` to `[workspace] members` (insertion alongside lines 4-30).
- `crates/xifty-core/src/lib.rs` — add `Format::Rw2` variant (line 8-21) and the `"rw2"` string in `as_str()` (line 23-39).
- `crates/xifty-detect/src/lib.rs` — add `IIU\x00` check before the standard TIFF branch at line 10; extend `detects_formats` test (line 194) and add a "must NOT misroute to TIFF" regression test.
- `crates/xifty-detect/Cargo.toml` — no dep change required (detection stays magic-only; we do not parse Panasonic IFD entries from inside detect).
- `crates/xifty-cli/src/lib.rs` — add `Format::Rw2` arms in `probe_source` (around line 50-99), in `extract_source` match (around line 134-217 — add `Format::Rw2 => rw2_extract(&source)?`), import `decode_from_rw2`, and add a new `fn rw2_extract` mirroring `fn tiff_extract` at line 541. **Do NOT** call `decode_from_tiff` (the EXIF decoder) on RW2 bytes.
- `crates/xifty-cli/Cargo.toml` — add `xifty-container-rw2` and `xifty-meta-panasonic` deps.
- `crates/xifty-cli/tests/cli_contract.rs` — add probe + extract snapshot tests for an RW2 fixture.
- `CAPABILITIES.json` — add `containers.rw2` entry with `{"panasonic": "bounded"}` after the `dng` block (line 119-127).

## New files
- `crates/xifty-container-rw2/Cargo.toml` — workspace member; deps: `xifty-core`, `xifty-source` (mirror `xifty-container-tiff/Cargo.toml`).
- `crates/xifty-container-rw2/src/lib.rs` — `Rw2Entry`, `Rw2Container`, `parse_bytes(bytes, base_offset, root_label)`, `parse(source)`, internal `walk_panasonic_ifd0`. Magic check rejects anything other than `IIU\x00` (little-endian only — RW2 has not been observed in big-endian form; reject big-endian with a typed `XiftyError::Parse`). Reuses the same 12-byte entry shape and `value_size()` table as TIFF. Does NOT recurse into `0x8769` / `0x8825` (those tag IDs are not "ExifIFD/GPS" in Panasonic's namespace).
- `crates/xifty-container-rw2/src/lib.rs` — module-level `//!` doc comment explicitly states the **tag-ID collision policy**: "Panasonic IFD0 tag IDs in the range 0x0001..0x002d collide with standard TIFF tag IDs and MUST NOT be passed to `xifty-meta-exif`. The container exposes `Rw2Entry` records that consumers (e.g. `xifty-meta-panasonic`) interpret with Panasonic semantics."
- `crates/xifty-meta-panasonic/Cargo.toml` — workspace member; deps: `xifty-core`, `xifty-container-rw2`, `xifty-source`.
- `crates/xifty-meta-panasonic/src/lib.rs` — `decode_from_rw2(bytes, base_offset, container_name, &Rw2Container) -> Vec<MetadataEntry>`; bounded Panasonic tag table; emits entries with `namespace = "panasonic"` so they cannot be confused with `exif` namespace; module-level doc reiterating the collision policy and listing the bounded tag set.
- `crates/xifty-meta-panasonic/src/lib.rs` — unit tests building a synthetic RW2 byte buffer (mirroring `build_single_entry_tiff` at `crates/xifty-container-tiff/src/lib.rs:261-288` but with magic byte 0x55) and asserting decoded entries.
- `fixtures/minimal/panasonic.rw2` — minimal synthetic RW2 fixture (header + IFD0 with a couple of Panasonic tags). If creating a real-camera fixture is infeasible, a synthetic hand-built RW2 is acceptable — sibling DNG/TIFF tests already do this.
- `crates/xifty-cli/tests/snapshots/cli_contract__probe_panasonic_rw2.snap` — generated by insta on first run.
- `crates/xifty-cli/tests/snapshots/cli_contract__extract_panasonic_rw2_interpreted.snap` — generated by insta on first run.

## Step-by-step
1. **Add `Format::Rw2`** in `crates/xifty-core/src/lib.rs:8-40` — `cargo build -p xifty-core` succeeds; `Format::Rw2.as_str() == "rw2"`.
2. **Wire detection** in `crates/xifty-detect/src/lib.rs` before the `II*\0`/`MM\0*` branch (line 10) — `detect()` returns `Format::Rw2` for buffers starting with `IIU\x00` and still returns `Format::Tiff` for `II*\0`. Add unit tests: `detects_rw2_magic`, `rw2_does_not_misroute_to_tiff`.
3. **Create `xifty-container-rw2` crate** mirroring `crates/xifty-container-tiff/src/lib.rs:1-78` — `parse_bytes()` returns an `Rw2Container` with one IFD ("panasonic_ifd0"). Reject anything other than little-endian `IIU\x00` via `XiftyError::Parse`. Unit test: synthetic RW2 with one entry parses; standard TIFF magic fails.
4. **Create `xifty-meta-panasonic` crate** with bounded tag table and module-level collision policy doc — unit tests assert specific tags decode to expected `TypedValue` and `namespace == "panasonic"`.
5. **Register both crates** in workspace `Cargo.toml` `[workspace] members` — `cargo metadata` lists them.
6. **CLI wiring** — `Format::Rw2` arm in `probe_source` (`crates/xifty-cli/src/lib.rs:50-99`) calls `xifty_container_rw2::parse`; arm in `extract_source` calls a new `rw2_extract` helper mirroring `tiff_extract` (line 541) that wires **only** `decode_from_rw2`. Add the import `use xifty_meta_panasonic::decode_from_rw2;`. CLI `cargo build -p xifty-cli` succeeds.
7. **CLI snapshot tests** in `crates/xifty-cli/tests/cli_contract.rs` — add `probe_panasonic_rw2` + `extract_panasonic_rw2_interpreted` cases against `fixtures/minimal/panasonic.rw2`. Author-run `cargo insta review` once to accept; verify the snapshot includes `panasonic` namespace entries and contains zero `exif` namespace entries (proves no misroute).
8. **Update `CAPABILITIES.json`** — add `containers.rw2: { "namespaces": { "panasonic": "bounded" } }` after the `dng` block.
9. **Run validate** — `cargo fmt --all -- --check`, `cargo test --workspace --all-features`, `cargo test -p xifty-ffi --all-features`. All green.

## Tests
- `crates/xifty-detect/src/lib.rs` — `detects_rw2_magic`, `rw2_does_not_misroute_to_tiff` (regression: feeding a TIFF magic still produces `Format::Tiff`, feeding an RW2 magic produces `Format::Rw2`).
- `crates/xifty-container-rw2/src/lib.rs` — `parses_minimal_rw2`, `rejects_standard_tiff_magic`, `rejects_big_endian_rw2`, `walks_ifd0_with_panasonic_tag_ids` (asserts `ifd_name == "panasonic_ifd0"`).
- `crates/xifty-meta-panasonic/src/lib.rs` — `decodes_panasonic_raw_version_tag`, `decodes_sensor_dimensions_tags`, `entries_use_panasonic_namespace` (regression for the collision policy).
- `crates/xifty-cli/tests/cli_contract.rs` — `probe_panasonic_rw2` (insta JSON snapshot of probe output), `extract_panasonic_rw2_interpreted` (insta snapshot — must contain `panasonic` namespace entries; must contain zero `exif` namespace entries from the Panasonic IFD).
- The RW2 fixture is committed under `fixtures/minimal/panasonic.rw2` (synthetic hand-built bytes; clearly documented in the test as not a real camera file).

## Validation
Per `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

The third gate is informational here — there is no FFI shape change. `Format::Rw2` is a new enum variant; per `FFI_CONTRACT.md` semantics it should not break the C ABI as long as JSON serialization of `detected_format` is the only externally visible surface (it is — `Format::as_str()` produces a snake_case string).

## Risks
- **Tag-ID collision** is the central risk. Mitigation: synthetic test that constructs an RW2 with tag 0x010F (which means "Make" in TIFF / EXIF but is a Panasonic-private value here) and asserts the resulting `MetadataEntry` is in the `panasonic` namespace, never `exif`. If this test ever flips, the collision policy has been violated.
- **Canonical Panasonic tag list is TBD** (called out in the issue as plan ambiguity #3). Mitigation: the task is sized S, so we ship a bounded subset (PanasonicRawVersion, sensor borders, basic WB levels) and document the policy in the module doc comment. Future tasks expand the list without changing the boundary.
- **Real-camera fixture availability** — synthetic RW2 fixtures are sufficient for unit + snapshot tests; a real fixture is desirable but may not be available locally. The plan does not require one.
- **Detect ordering** — `IIU\x00` must come before any TIFF-like magic check that might match `II*` (none currently does — `II*\0` is exact). Future-proofed by ordering `IIU\x00` first.
- **Big-endian RW2** has not been observed in the wild; we reject it explicitly with `XiftyError::Parse` rather than silently misparsing.
- **No EXIF Sub-IFD recursion** — if real-world RW2 files do place a standard EXIF Sub-IFD via tag 0x8769, this plan does not surface it. That is acceptable for an [S] task; a follow-up can wire EXIF Sub-IFD recursion through `decode_from_tiff` only when an explicit Panasonic-defined pointer tag is reached, never via IFD0's own 0x8769 (because in IFD0 0x8769 may mean something else under Panasonic semantics). Document this limitation in the module doc.

