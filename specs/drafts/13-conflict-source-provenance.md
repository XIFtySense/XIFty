<!-- loswf:plan -->
# Plan #13: Conflict type carries no source provenance

## Problem
`xifty_core::Conflict` (crates/xifty-core/src/lib.rs:58-61) is just `{ field, message }`. The vision (specs/VISION.md:27-29, specs/VISION.md:98-100) makes "where it came from" and "which values conflict" first-class, but today callers receiving `report.conflicts[*]` cannot see which namespaces/containers/tag_ids collided, nor the competing typed values — all of that information is stuffed into a human-readable `message` string. `xifty-policy` (crates/xifty-policy/src/lib.rs:265,305,341,386,422) already has the winning `MetadataEntry`, its `&[&MetadataEntry] matches`, and `has_material_difference` on hand at each conflict emission site, but throws that provenance away. This blocks downstream callers (CLI, SDK, bindings) from acting on conflicts programmatically.

## Approach
Extend `Conflict` with a `sources: Vec<ConflictSide>` field (additive), where `ConflictSide` captures one participant: `{ namespace, tag_id, tag_name, value: TypedValue, provenance: Provenance }`. The winner is always `sources[0]` (deterministic, matches the existing "selected X from Y" narrative). Disagreeing entries follow. Update the five `conflicts.push(Conflict {...})` sites in `xifty-policy` to populate `sources` from the already-available `winner` + `matches` (filtered to those with material difference from the winner). Keep `field` and `message` unchanged for backward-compat. Bump the JSON schema additively: add `sources` to the `conflict` definition in `schemas/xifty-analysis-0.1.0.schema.json` as an optional array (docs/SCHEMA_POLICY.md:49 — adding optional object properties is additive, no `SCHEMA_VERSION` bump). The FFI/C ABI surface (include/xifty.h, crates/xifty-ffi/src/lib.rs) is JSON-passthrough and does not expose `Conflict` typed, so no FFI_CONTRACT.md change is required — note this explicitly in the plan.

## Files to touch
- `crates/xifty-core/src/lib.rs` — add `ConflictSide` struct, add `sources: Vec<ConflictSide>` to `Conflict` (lines 57-61), keep `field`/`message`.
- `crates/xifty-policy/src/lib.rs` — populate `sources` at each of the five `conflicts.push` sites (lines 265, 305, 341, 386, 422); use the existing `matches`/`winner` pair. Likely introduce a small `build_conflict_sides(matches, winner)` helper to mirror `conflict_note` (lines 520-530) so all five call sites share logic.
- `schemas/xifty-analysis-0.1.0.schema.json` — extend `$defs.conflict` (lines 232-240) with a `sources` property referencing a new `$defs.conflict_side` that reuses `$defs.provenance` and `$defs.typed_value`.
- `crates/xifty-cli/tests/cli_contract.rs` — strengthen `overlap_editorial_jpeg_prefers_xmp_for_editorial_fields` (lines 372-415) to assert the new `sources[*].provenance.namespace` shows both `exif`/`xmp` (or `iptc`) for at least one of the editorial conflicts. Add an explicit EXIF-vs-XMP conflict assertion per the issue's acceptance criterion.
- `crates/xifty-cli/tests/snapshots/cli_contract__conflicting_png_report.snap` — will need regeneration via `cargo insta review` (not blanket-accept; see guardrail).
- Any other `*report*.snap` or view snapshots that serialize `conflicts` with entries (e.g. `cli_contract__extract_mixed_webp_normalized.snap`, `cli_contract__extract_mixed_png_normalized.snap`, `cli_contract__extract_real_camera_mp4_normalized.snap`, `cli_contract__extract_real_camera_mp4_interpreted.snap`) — regenerate after review.
- `docs/SCHEMA_POLICY.md` — no change; verify the additive path applies (it does).

## New files
- None required. `ConflictSide` lives next to `Conflict` in `crates/xifty-core/src/lib.rs` to keep the core types colocated.

## Step-by-step
1. Add `ConflictSide { namespace: String, tag_id: String, tag_name: String, value: TypedValue, provenance: Provenance }` to `crates/xifty-core/src/lib.rs`, with `#[derive(Debug, Clone, Serialize, PartialEq)]` matching `MetadataEntry`. Add `pub sources: Vec<ConflictSide>` to `Conflict` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so existing JSON consumers with empty conflicts remain byte-identical and schema-valid. Update `Conflict`'s derives to `PartialEq` (drop `Eq`, since `TypedValue` contains `f64`). — `cargo build -p xifty-core` compiles; downstream crates still compile because `sources` defaults away.
2. In `crates/xifty-policy/src/lib.rs`, add a private helper `fn build_conflict_sides(matches: &[&MetadataEntry], winner: &MetadataEntry) -> Vec<ConflictSide>` that returns winner-first, followed by each `m` in `matches` where `m.value != winner.value` (mirrors `has_material_difference` semantics used elsewhere). Import `ConflictSide` at line 1. — unit-testable in-crate.
3. Update all five `conflicts.push(Conflict { field, message })` sites (lines 265, 305, 341, 386, 422) to `Conflict { field: field_name.into(), message: ..., sources: build_conflict_sides(&matches, winner) }`. — `cargo check --workspace` clean.
4. Extend `schemas/xifty-analysis-0.1.0.schema.json`: add `conflict_side` to `$defs` (required: `namespace`, `tag_id`, `tag_name`, `value`, `provenance`; reuse `#/$defs/typed_value` and `#/$defs/provenance`), and add optional `sources: { type: "array", items: { $ref: "#/$defs/conflict_side" } }` to `$defs.conflict.properties`. Do NOT add `sources` to `required`, and do NOT bump `SCHEMA_VERSION` (docs/SCHEMA_POLICY.md:41-52). — `tools/validate_schema_examples.py` still passes.
5. In `crates/xifty-cli/tests/cli_contract.rs::overlap_editorial_jpeg_prefers_xmp_for_editorial_fields` (lines 373-415), add assertions that the `author` conflict's `sources` array contains at least two entries and that the set of `sources[*].provenance.namespace` values includes both `exif` (or `iptc`) and `xmp`. Add a similar assertion for `conflicting.png` via a new `fn conflicting_png_report_exposes_source_namespaces` test that reads the already-present `conflicting.png` fixture (tools/generate_fixtures.py:633) and asserts the `captured_at` conflict lists both `exif` and `xmp` namespaces in `sources`. — new test is the CLI contract demanded by the issue's acceptance criteria.
6. Run the full `validate[]` chain and regenerate snapshots with `cargo insta review` (per guardrail, review diffs — do not blanket-accept). Inspect each diff to confirm only the new `sources` arrays are added and that existing `field`/`message` stay unchanged. — snapshots committed with reviewed diffs.
7. Confirm no changes to `include/xifty.h`, `crates/xifty-ffi/src/lib.rs`, or `FFI_CONTRACT.md` are required: the FFI surface produces JSON strings; the additive schema change flows through untouched. Note this explicitly in the PR description. — `cargo test -p xifty-ffi --all-features` still passes unchanged.

## Tests
- Unit: none strictly required beyond what the CLI contract covers; optional inline `#[cfg(test)]` in `xifty-policy/src/lib.rs` (follows the existing pattern at lines 571/607/705) asserting that a synthetic `reconcile` call produces a `Conflict` whose `sources` contains both participant namespaces and distinct values.
- Contract (required): strengthened `overlap_editorial_jpeg_prefers_xmp_for_editorial_fields` and a new `conflicting_png_report_exposes_source_namespaces` in `crates/xifty-cli/tests/cli_contract.rs`.
- Snapshot: reviewed regeneration of `cli_contract__conflicting_png_report.snap` and any other snapshots that currently show non-empty `conflicts` arrays (search pattern: `"conflicts": \[\s*\{`).

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`
- Plus: `python tools/validate_schema_examples.py` (if present in CI) to confirm schema stays consistent with emitted JSON.

## Risks
- Serde rename: adding `sources` to `Conflict` without `#[serde(default, skip_serializing_if = "Vec::is_empty")]` would break every existing `"conflicts": []` snapshot. The plan explicitly requires the skip attribute, so empty-conflict snapshots stay byte-identical.
- `Eq` derive: `Conflict` currently derives `Eq`; adding `TypedValue` (which contains `f64`) transitively breaks `Eq`. Drop `Eq` from `Conflict` (keep `PartialEq`). Confirm no downstream code uses `Conflict` in `HashSet`/`BTreeSet` — `rg "HashSet<Conflict>|BTreeSet<Conflict>"` should be empty (spot-checked; no hits). Add this check to the plan.
- Snapshot churn: multiple report snapshots will shift. Guardrail requires reviewing each diff individually.
- Overlap with #8 (threading) and #11 (detection rules): this change is purely a type/shape enrichment and does not touch detection logic (`has_material_difference` stays) or threading. `sources` semantics will compose cleanly with future rule-based detection from #11 (each rule hit can populate a `ConflictSide`).
- Schema-version policy: additive-optional path is clearly allowed by docs/SCHEMA_POLICY.md:41-52; no bump needed. If plan-reviewer disagrees, fallback is to bump to `0.2.0` and duplicate the schema file — deferred decision.
- FFI: the C ABI is JSON-passthrough so no header/ABI change, but binding consumers that strictly validated `conflict` against the old schema shape with `additionalProperties: false` will now accept the superset. That matches stated policy but is worth calling out in the PR body.

