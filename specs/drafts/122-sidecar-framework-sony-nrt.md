<!-- loswf:plan -->
# Plan #122: xifty-sidecar framework + xifty-sidecar-sony-nrt adapter

## Problem
XIFty currently treats every primary file in isolation: containers + namespace decoders only consume bytes from the asset itself. Real-world camera output (notably Sony XAVC: 67 paired clips on the review SD card) ships critical metadata in co-located XML sidecars that the MP4 container never carries — UMID, capture/format FPS, gamma equations, LTC ranges, recording mode, serial number. Phase 1 of the parent epic #121 must (a) introduce a generic, vendor-agnostic sidecar abstraction (`Sidecar` trait, registry, merge policy, missing-target handling, schema-version tolerance) as a peer to containers/namespaces, and (b) prove it with one shipping vendor adapter for Sony NRT XML at `urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.10`/`ver.2.20`. 100% native Rust — no bundling, no shelling out, no FFI to non-XIFty crates. Reverse-engineered from the public Sony XSD; ExifTool `Sony.pm`/`XML.pm` are read-only cross-checks and must be cited per-tag in code comments.

## Approach
Mirror the existing `xifty-meta-*` crate pattern (`xifty-meta-rtmd`, `xifty-meta-ixml`, `xifty-meta-bwf`) but at the sidecar layer. Add two workspace crates: `xifty-sidecar` (pure abstraction, zero parsing) and `xifty-sidecar-sony-nrt` (adapter, depends on `quick-xml` 0.x — already a transitive workspace dep at `Cargo.lock:393`; promote to a `[workspace.dependencies]` entry). Inject the sidecar merge call in `crates/xifty-cli/src/lib.rs::extract_source` immediately after `add_filesystem_timestamp_fallbacks` and before `normalize_with_policy(&entries)` (lib.rs:599-606) so sidecar entries flow through normalize/conflict-detection identically to embedded sources. Sidecar entries stamp `MetadataEntry::namespace = "sony_nrt"` and `Provenance::container = "sidecar"` (mirroring the `filesystem`/`png` pattern at lib.rs:1178/1346). Discovery only runs when `extract_path` is called *and* `--sidecars` is opt-in; `extract_bytes` always sees a no-op.

## Files to touch
- `Cargo.toml` (root) — add `crates/xifty-sidecar` + `crates/xifty-sidecar-sony-nrt` to `[workspace] members` (alphabetical, between `xifty-policy` and `xifty-source`); add `quick-xml = "0.36"` to `[workspace.dependencies]`.
- `crates/xifty-cli/src/lib.rs` — new `use xifty_sidecar::{SidecarRegistry, MergePolicy};` and `use xifty_sidecar_sony_nrt::SonyNrtSidecar;`; thread an `enable_sidecars: bool` bit through `extract_path`/`extract_source`; insert sidecar merge between line 604 and 606; add new `sidecar_*` issue-codes via existing `namespace_issue` helper at lib.rs:1465.
- `crates/xifty-cli/src/main.rs` — add `--sidecars` flag to clap struct for the `extract` subcommand (default `false`).
- `crates/xifty-ffi/src/lib.rs` — add new exported function `xifty_extract_json_with_options` taking a `XiftyExtractOptions` struct (`{ view_mode, enable_sidecars }`); keep `xifty_extract_json` unchanged for ABI stability per `FFI_CONTRACT.md`.
- `crates/xifty-wasm/src/lib.rs` — add a `sidecars: Option<bool>` parameter to `extract_bytes_json` that is silently accepted and ignored (no filesystem); document explicitly in rustdoc.
- `crates/xifty-cli/tests/cli_contract.rs` — new gated integration test for `C0242.MP4 + C0242M01.XML` (uses existing `skip_missing_local_fixture` at line 116).
- `crates/xifty-normalize/src/lib.rs` — add lifters for new sidecar-only normalized fields (`umid`, `recording.mode`, `recording.cache_rec`, `recording.capture_fps`, `recording.format_fps`, `timecode.fps`, `timecode.half_step`, `timecode.ltc.start`, `timecode.ltc.end`, `color.gamma_equation`, `color.coding_equations`, `device.serial_no`) — pattern after `derive_avif_color_fields` (line 87) and `derive_animation_frame_count` (line 127), gated on `entry.namespace == "sony_nrt"`.
- `CAPABILITIES.json` — add top-level `sidecars` block (peer to `containers`/`normalized_fields`); extend `normalized_fields` with the 11 new field names; add `xifty_extract_json_with_options` to `surfaces.ffi.functions`.
- `docs/SCHEMA_POLICY.md` — document `sources[].namespace` semantics for sidecar-derived fields and that this is additive (no schema bump).
- `tools/generate_capabilities.py` — extend to recognize `namespace == "sony_nrt"` from `--sidecars` runs (or document that `--check` ignores sidecars for now).
- `tools/generate_fixtures.py` — add a `build_sony_nrt_pair()` helper emitting deterministic minimal MP4 + NRT XML pair into `fixtures/minimal/sony-nrt/`.
- `README.md` — new "Sidecars" subsection under "What XIFty Supports Today" naming `sony_nrt` with the bounded tag list.
- `demo/web/index.html` — one-line note in the "Try it" copy that browser/WASM mode does not run sidecar discovery.
- `FFI_CONTRACT.md` — document the new `xifty_extract_json_with_options` function and the `XiftyExtractOptions` struct.

## New files
- `crates/xifty-sidecar/Cargo.toml` — crate manifest; depends only on `xifty-core`.
- `crates/xifty-sidecar/src/lib.rs` — `Sidecar` trait, `SidecarRef`, `SidecarPayload { entries: Vec<MetadataEntry>, issues: Vec<Issue> }`, `MergePolicy { Override, Complement }`, `MergeContext`, `SidecarRegistry { adapters: Vec<Box<dyn Sidecar>> }`, `discover_for(Option<&Path>) -> Vec<(adapter_idx, SidecarRef)>`, `merge_into(&mut Vec<MetadataEntry>, &mut Vec<Issue>)`. Defines the three issue codes as `pub const`s: `SIDECAR_TARGET_MISSING`, `SIDECAR_NO_INDEX_ENTRY`, `SIDECAR_UNKNOWN_SCHEMA_VERSION` (info severity).
- `crates/xifty-sidecar/tests/registry.rs` — registry priority order, MergePolicy::Override conflict routing, MergePolicy::Complement coexistence, missing-target info-issue emission, buffer-only no-op.
- `crates/xifty-sidecar-sony-nrt/Cargo.toml` — depends on `xifty-core`, `xifty-sidecar`, `quick-xml`.
- `crates/xifty-sidecar-sony-nrt/src/lib.rs` — `SonyNrtSidecar` impl. XML walker driven by `quick-xml::reader::Reader`. Schema-version detection from root `xmlns`. 14 tag-mapping functions, each with a `// per Sony NRT XSD ver.2.20 §X.Y` or `// cross-check ExifTool Sony.pm:N` comment.
- `crates/xifty-sidecar-sony-nrt/tests/discover.rs` — discovery glob behavior (`<base>M01.XML`, `M02.XML`, sort-descending, ENOENT → empty).
- `crates/xifty-sidecar-sony-nrt/tests/parse_v210.rs` — parse minimal 2.10 payload, assert each of the 14 fields surfaces.
- `crates/xifty-sidecar-sony-nrt/tests/parse_v220.rs` — parse minimal 2.20 payload (adds Gyroscope/Accelerometor — best-effort empty).
- `crates/xifty-sidecar-sony-nrt/tests/unknown_version.rs` — unknown xmlns emits `sidecar_unknown_schema_version` info, parses known elements, no panic.
- `fixtures/minimal/sony-nrt/clip.mp4` + `clip.M01.XML` — synthetic minimal pair generated by `tools/generate_fixtures.py`.

## Step-by-step
1. **Workspace plumbing** — edit root `Cargo.toml`: add the two new members, add `quick-xml = "0.36"` to `[workspace.dependencies]`. Outcome: `cargo check --workspace` succeeds with empty stub crates.
2. **`xifty-sidecar` crate scaffold** — create `Cargo.toml` + `lib.rs` with empty `Sidecar` trait, `SidecarRegistry`, `MergePolicy`, `SidecarRef`, `SidecarPayload`, `MergeContext`, `pub const SIDECAR_*` issue codes. Outcome: `cargo test -p xifty-sidecar` runs (no tests yet).
3. **Registry merge logic** — implement `SidecarRegistry::discover_for(Option<&Path>) -> Vec<DiscoveredSidecar>` and `merge_into(&mut Vec<MetadataEntry>, &mut Vec<Issue>)`. `Override` writes the sidecar entry and downgrades the embedded one to a conflict candidate; `Complement` appends additively (let the existing conflict-detector flag overlap). Outcome: unit tests at `tests/registry.rs` pass.
4. **`xifty-sidecar-sony-nrt` scaffold + discovery** — implement `Sidecar::discover` with the `<basename>M\d+.XML` glob (highest-numbered wins on hits, but return all and let registry use `priority()`). Mirror the same-directory-only convention. Buffer (no-path) → empty. Outcome: `tests/discover.rs` passes.
5. **NRT XML parser — schema-version detection** — read root element, capture `xmlns` attribute, map `ver.2.10` / `ver.2.20` / unknown. Unknown emits `sidecar_unknown_schema_version` info issue with the encountered xmlns and continues. Outcome: `tests/unknown_version.rs` passes.
6. **NRT XML parser — bounded tag mappings** — implement 14 tag → field maps from issue body table. Each tag function carries a `// per Sony NRT XSD …` citation comment and is accompanied by a unit-test assertion. `Device@serialNo` → `device.serial_no`; `CreationDate@value` (TZ-aware) → `captured_at`; `RecordingMode@type` → `recording.mode`; `RecordingMode@cacheRec` → `recording.cache_rec`; `VideoFrame@captureFps`/`@formatFps` → `recording.capture_fps`/`recording.format_fps`; `VideoFrame@videoCodec` → cross-check entry; `VideoLayout@*` → cross-check entries; `TargetMaterial@umidRef` → `umid`; `LtcChangeTable@tcFps`/`@halfStep` + first/last `LtcChange` → `timecode.fps`/`timecode.half_step`/`timecode.ltc.start`/`timecode.ltc.end`; `AcquisitionRecord/.../CaptureGammaEquation` → `color.gamma_equation`; `CaptureColorPrimaries` → `color.primaries`; `CodingEquations` → `color.coding_equations`; `AudioFormat@numOfChannel` + `AudioRecPort@audioCodec` → cross-check entries; `Duration@value` → cross-check entry. Outcome: `tests/parse_v210.rs` + `tests/parse_v220.rs` green.
7. **Wire into xifty-cli** — add `enable_sidecars: bool` to `extract_path`, thread to `extract_source`; build `SidecarRegistry::default()` (registers `SonyNrtSidecar`); call `registry.merge_into(&mut entries, &mut issues)` between lib.rs:604 and lib.rs:606. Outcome: existing tests still pass; new flag still no-op without fixture.
8. **CLI flag** — add `--sidecars` (default false) to extract subcommand in `crates/xifty-cli/src/main.rs`. Outcome: `cargo run -p xifty-cli -- extract --help` shows the flag.
9. **Normalize lifters** — in `crates/xifty-normalize/src/lib.rs`, add a `derive_sony_nrt_fields(&entries, &mut fields)` helper called from `normalize_with_policy` (mirror existing pattern at line 60). Lift each new field name; carry `entry.provenance` into `NormalizedField::sources` so the merged field shows `sources[].namespace = "sony_nrt"`. Outcome: snapshot test asserts the lifted fields.
10. **FFI surface** — add `XiftyExtractOptions { view_mode, enable_sidecars }` and `xifty_extract_json_with_options`; preserve `xifty_extract_json` exactly as-is. Add unit test in `crates/xifty-ffi/src/lib.rs` `mod tests`. Outcome: `cargo test -p xifty-ffi --all-features` passes; cbindgen header regen reflects the new struct.
11. **WASM no-op** — extend `extract_bytes_json` signature to accept (and ignore) `sidecars`. Outcome: `cargo test -p xifty-wasm` passes.
12. **Synthetic fixture generation** — extend `tools/generate_fixtures.py` with `build_sony_nrt_pair()` that emits a minimal `fixtures/minimal/sony-nrt/clip.mp4` + `clip.M01.XML` covering all 14 mappings, using known-good XSD-shape XML. Commit the artifacts. Outcome: re-running the script is deterministic; `git diff` is empty.
13. **Real-fixture integration test** — append a `cli_contract.rs` test using `skip_missing_local_fixture("C0242.MP4")` + a sibling `skip_missing_local_fixture("C0242M01.XML")` guard; assert all sidecar-derived normalized fields surface with `sources[].namespace = "sony_nrt"`. Outcome: test runs locally only when both fixtures exist (gracefully skipped in CI).
14. **CAPABILITIES + schema policy** — add top-level `"sidecars": { "sony_nrt": { "schema_versions": ["2.10","2.20"], "supported_tags": [...], "merge_policy": "complement" } }`; extend `normalized_fields`; add `xifty_extract_json_with_options` to `surfaces.ffi.functions`. Update `docs/SCHEMA_POLICY.md`. Outcome: `python3 tools/generate_capabilities.py --check` passes.
15. **Docs** — update `README.md` "What XIFty Supports Today" with a Sidecars subsection; one-line note in `demo/web/index.html`; update `FFI_CONTRACT.md` with the new function/struct rationale.

## Tests
- Step 3: `crates/xifty-sidecar/tests/registry.rs` — registry priority, both merge policies, missing-target info-issue, buffer no-op.
- Step 4: `crates/xifty-sidecar-sony-nrt/tests/discover.rs` — glob, sort, missing path.
- Steps 5–6: `tests/parse_v210.rs`, `tests/parse_v220.rs`, `tests/unknown_version.rs`.
- Step 9: `crates/xifty-normalize/src/lib.rs` `mod tests` — assert lift of each new field with `sources[].namespace == "sony_nrt"`.
- Step 10: `crates/xifty-ffi/src/lib.rs` `mod tests` — `extract_json_with_options_passes_through_sidecars_flag`.
- Step 13: `crates/xifty-cli/tests/cli_contract.rs` — gated real-fixture assertion (skipped without `fixtures/local/C0242M01.XML`).

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`
- `python3 tools/generate_capabilities.py --check`
- (informational) `cargo run -p xifty-cli -- extract fixtures/minimal/sony-nrt/clip.mp4 --sidecars --view normalized` returns the 11 lifted fields.

## Risks
- **`fixtures/local/C0242M01.XML` is not currently present** in `fixtures/local/` (only `C0242.MP4` is). The real-fixture test must be `skip_missing_local_fixture`-gated on the XML sibling, not just the MP4. If the operator can drop the XML next to the MP4 the test runs end-to-end; otherwise it skips like the other local-fixture tests.
- **`quick-xml` not yet a workspace dep** — promoting from transitive to declared adds a small surface-area commitment. Pinning to `0.36` matches the version already resolved in `Cargo.lock:393`.
- **MergePolicy::Override + existing conflict-dedupe interplay** — `crates/xifty-cli/src/conflict_dedupe.rs` runs after both reports merge. The plan's Override branch must produce the same shape the dedupe expects, or downgraded conflicts will double-report. Step 3 unit tests must include a regression for this.
- **Sony NRT `CreationDate@value` TZ semantics** — NRT carries TZ-aware timestamps where the embedded MP4 only has UTC. `MergePolicy::Complement` means both surface; conflict-detection will flag them. That's *correct* and intended (see issue #122 scope), but reviewers may push back. Plan documents this in `SCHEMA_POLICY.md` to preempt.
- **`generate_capabilities.py --check` drift** — the tool currently re-derives the namespaces map from CLI raw output; it must learn that `sony_nrt` only appears when `--sidecars` is enabled, or be told to ignore the sidecar namespace for now. Step 14 picks the simpler path: skip sidecar-derived namespaces in `--check` and rely on the new top-level `sidecars` block as the source of truth.
- **`schema_version` bump** — confirmed additive only; do *not* bump `SCHEMA_VERSION` in `xifty-core`.
- **FFI ABI** — net-add only (`xifty_extract_json` unchanged); no breaking change. `FFI_CONTRACT.md` update is a doc-only delta, not a contract break.
- **Phase 2 coupling** — `SIDECAR_TARGET_MISSING` and `SIDECAR_NO_INDEX_ENTRY` are defined in this phase but only `SIDECAR_UNKNOWN_SCHEMA_VERSION` is emitted by the Sony adapter. That's deliberate so #123 (mediapro) inherits the codes without a churn.

