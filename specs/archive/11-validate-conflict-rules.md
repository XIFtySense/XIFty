<!-- loswf:plan -->
# Plan #11: xifty-validate — entry-level conflict detection rules

## Problem
`xifty-validate::build_report()` (crates/xifty-validate/src/lib.rs:3–17) is effectively empty: it only emits a single `no_metadata_entries` Info issue and hardcodes `conflicts: Vec::new()`. There are no rules that inspect the `entries: &[MetadataEntry]` slice and produce `Conflict` values. Today the only conflicts that ever reach `Report.conflicts` are the ones `xifty-policy` emits as a side-effect of normalization winner selection (crates/xifty-policy/src/lib.rs:265, 305, 341, 386, 422) — and those are scoped to fields that happen to be in the normalized schema. Cross-namespace disagreements on timestamps, device fields, or editorial fields that are not in the policy table are silently lost, even though VISION principle 3 ("First-class provenance … conflicts are reported, not silently resolved") and SRS line 60 make conflict reporting a non-negotiable concern distinct from normalization. This issue is scoped to the detection rules; issue #8 handles threading those conflicts into CLI output (already partially wired at crates/xifty-cli/src/lib.rs:352–353).

## Approach
Add entry-level conflict detection functions inside `xifty-validate` that run over the `entries` slice and emit `Conflict` values, independent of the policy winner-selection machinery. Start with three clearly-defined, low-false-positive rule families, each implemented as a small free function returning `Vec<Conflict>` and composed in `build_report`:

1. **Cross-namespace semantic-tag disagreement** — a curated map of semantic groups (e.g. `captured_at` ← `{exif:DateTimeOriginal, xmp:CreateDate, quicktime:CreationDate}`, `device.make` ← `{exif:Make, xmp:tiff:Make, quicktime:Make}`, `device.model` ← similar, `copyright` ← `{exif:Copyright, xmp:dc:rights, iptc:CopyrightNotice}`). When two entries in the same group carry different canonical string values (after trimming), emit a `Conflict { field: <semantic>, message: "<ns-a>:<tag-a>=<val-a> vs <ns-b>:<tag-b>=<val-b>" }`.
2. **Timestamp timezone/offset disagreement** — for entries whose tag is a known timestamp (DateTimeOriginal, CreateDate, ModifyDate, CreationDate), parse the trailing offset (`Z`, `+HH:MM`, or absent) and flag when two entries represent the same wall time but differing UTC offsets, or the same semantic group with offsets that disagree.
3. **Numeric precision mismatch** — for numeric semantic groups (ISO, FNumber, FocalLength, ExposureTime) compare rational/integer/float candidates using a relative-tolerance check (e.g. 0.5% of the larger magnitude). Exact byte-for-byte equality after type coercion does not fire; a real mismatch (e.g. XMP says `iso=200`, EXIF says `iso=400`) does.

Rule registration mirrors the policy-table pattern in crates/xifty-policy/src/lib.rs:12–231 — a simple static table, one function per rule family. No new crate-level dependencies. Normalization-driven conflicts from `xifty-policy` continue to flow through the CLI path at crates/xifty-cli/src/lib.rs:352–353 (that wiring is #8's responsibility); this plan adds an independent set produced inside `build_report`.

## Files to touch
- `crates/xifty-validate/src/lib.rs` — extend `build_report` to run the new rule functions, merging their output into `Report.conflicts`. Keep the existing `no_metadata_entries` issue behavior.
- `crates/xifty-validate/Cargo.toml` — no dependency changes expected; rules rely only on `xifty-core` types already imported.

## New files
- `crates/xifty-validate/src/rules.rs` — new module for conflict detection rule functions (semantic groups, timestamp-offset, numeric-tolerance). Declared as `mod rules;` from `lib.rs` and only its `detect_conflicts(entries) -> Vec<Conflict>` entry-point is called from `build_report`.
- `crates/xifty-validate/tests/conflicts.rs` — integration-style tests that exercise `build_report` with crafted multi-namespace entry slices (one test per rule family, plus a negative test asserting no false positives when entries agree). Entry construction mirrors the inline style used in crates/xifty-policy/src/lib.rs:574–707.

## Step-by-step
1. Add `mod rules;` to `crates/xifty-validate/src/lib.rs` and create `rules.rs` with a `pub(crate) fn detect_conflicts(entries: &[MetadataEntry]) -> Vec<Conflict>` that returns an empty vec initially. Verify compile with `cargo check -p xifty-validate`.
2. Introduce a static `SEMANTIC_GROUPS` table (`&[(&str /* field */, &[(&str /* namespace */, &str /* tag */)])]`) in `rules.rs` covering `captured_at`, `device.make`, `device.model`, `copyright`. Implement `fn detect_cross_namespace_disagreement(entries, groups) -> Vec<Conflict>` that groups matches, canonicalizes strings (trim + lowercase for makes/models, normalize_timestamp-equivalent for timestamps), and emits one `Conflict` per group with ≥2 distinct canonical values. Verifiable outcome: unit test in `tests/conflicts.rs` asserting a conflict appears for an EXIF `Make=Canon` vs XMP `Make=Nikon` case.
3. Implement `fn detect_timestamp_offset_mismatch(entries) -> Vec<Conflict>`: parse the trailing `Z`/`±HH:MM` suffix of entries in the timestamp semantic group; if two entries share a semantic group and wall-time but carry different offsets (or one has an offset and one does not), emit a `Conflict { field: <semantic>, message: "timezone offset disagreement: …" }`. Keep parsing inline (no chrono dep) to avoid new workspace deps. Verifiable outcome: unit test with `DateTimeOriginal=2024-01-01T10:00:00+00:00` and `CreateDate=2024-01-01T10:00:00-05:00` produces exactly one conflict on `captured_at`.
4. Implement `fn detect_numeric_precision_mismatch(entries) -> Vec<Conflict>` for numeric semantic groups (`exposure.iso`, `exposure.aperture`, `exposure.focal_length_mm`, `exposure.shutter_speed`). Coerce Integer/Float/Rational to `f64` via the same shape as crates/xifty-policy/src/lib.rs:502–518 (duplicate the small helper locally in `rules.rs` to avoid widening the `xifty-core` surface). Fire when `|a-b| / max(|a|,|b|) > 0.005`. Verifiable outcome: unit test where EXIF `ISO=200` and XMP `ISO=400` produces one conflict on `exposure.iso`; `ISO=200` + `ISOSpeedRatings=200` produces none.
5. Wire all three detectors from `rules::detect_conflicts`, then call it from `build_report` and extend `Report.conflicts` with the result instead of returning `Vec::new()`. Confirm existing CLI test (`crates/xifty-cli/tests/cli_contract.rs`) still passes; if any insta snapshots pick up new conflicts, review diffs individually with `cargo insta review` per the guardrail at .loswf/config.yaml:56.
6. Add a negative unit test: a slice with matching values across namespaces yields `Report.conflicts.is_empty()`.

## Tests
- `crates/xifty-validate/tests/conflicts.rs`:
  - `cross_namespace_string_disagreement_is_reported` (step 2)
  - `timestamp_offset_mismatch_is_reported` (step 3)
  - `numeric_precision_mismatch_is_reported` (step 4)
  - `agreement_across_namespaces_produces_no_conflict` (step 6)
- In-module unit tests (`#[cfg(test)] mod tests` in `rules.rs`) for the canonicalization helpers (trim/lowercase, offset parsing, f64 tolerance comparator).
- Review and update insta snapshots in `crates/xifty-cli/tests/cli_contract.rs` only if fixtures happen to trigger new detections.

## Validation
Commands from `.loswf/config.yaml` `validate[]` (lines 37–43) that gate this work:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- Overlap with `xifty-policy`'s own conflict emission (crates/xifty-policy/src/lib.rs:265 etc.): the same underlying disagreement could now appear in `Report.conflicts` from both sources. Acceptable for this iteration — the `field` label is the same and messages differ; dedup can be a follow-up. Call this out in the PR description.
- Timestamp offset parsing without chrono is fiddly around `Z` vs `+00:00` and missing offsets. Keep the rule conservative: only fire when both sides parse cleanly and offsets are unambiguously different; skip when either side is unparseable.
- Semantic-group table is curated; false negatives are expected and fine at this iteration. Document the table as the extension point.
- #8 is a dependency for end-to-end user visibility. This plan does not depend on #8 landing first: validate-produced conflicts flow through `build_report` directly, so `Report.conflicts` will populate even without the CLI-level `report.conflicts = normalization.conflicts` line at crates/xifty-cli/src/lib.rs:353.
- Canonicalization (trim+lowercase for makes/models) can introduce false positives on legitimately distinct brand strings; limit lowercase-compare to `device.make` and `device.model`, keep strict equality elsewhere.

