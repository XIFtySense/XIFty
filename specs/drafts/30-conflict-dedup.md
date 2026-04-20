<!-- loswf:plan -->
# Plan #30: Deduplicate conflicts when both xifty-validate and xifty-policy emit the same disagreement

## Problem
`crates/xifty-cli/src/lib.rs:411-413` assembles `AnalysisOutput.report.conflicts` by calling `build_report(issues, &entries)` (which runs `xifty_validate::rules::detect_conflicts`) and then blindly `.extend()`ing `normalization.conflicts` from `xifty_policy::reconcile`. Both emitters independently evaluate the same logical field (e.g. `captured_at` in `xifty-validate/src/rules.rs:15-40` SEMANTIC_GROUPS and `xifty-policy/src/lib.rs:12-19` `maybe_choose_string(..., "captured_at", ...)`) and both push a `Conflict` with the same `field` string. The existing `conflicting_png_report.snap` literally contains two `captured_at` entries (lines 15 and 91) and two `device.model` entries (lines 53 and 167) — one with validate's "xmp:...=... vs exif:...=..." message and one with policy's "multiple candidates disagreed; selected ... from ..." message. This over-counts conflicts for programmatic consumers and clutters the report.

## Approach
Dedupe at the CLI assembly site (`crates/xifty-cli/src/lib.rs`) rather than inside either emitter, because (a) both emitters have legitimate independent reasons to surface the finding and their outputs are consumed elsewhere (e.g. FFI/JSON pipelines), and (b) the overlap is a CLI-level composition concern per PR #16's framing. The dedup key will be `(field, canonical_sources_fingerprint)` where the fingerprint is a sorted set of `(namespace, tag_id)` pairs drawn from `Conflict.sources`. When two conflicts collide, prefer the entry whose `sources` is non-empty and longer; ties go to the first seen (stable order). Implement as a small helper `dedupe_conflicts(Vec<Conflict>) -> Vec<Conflict>` in `xifty-cli` (private module) and call it once against the merged vector before constructing `AnalysisOutput`. Keep input order deterministic so insta snapshots stay stable.

## Files to touch
- `crates/xifty-cli/src/lib.rs` — replace `report.conflicts.extend(normalization.conflicts)` at line 413 with a merge-then-dedupe step; add private helper.
- `crates/xifty-cli/tests/snapshots/cli_contract__conflicting_png_report.snap` — review + re-accept after duplicate entries are removed (currently has duplicate `captured_at` at line 15/91 and duplicate `device.model` at line 53/167).
- Any other snapshot under `crates/xifty-cli/tests/snapshots/` that currently has duplicated `field` entries in `report.conflicts` — spot-check and review, do not blanket-accept.

## New files
- `crates/xifty-cli/src/conflict_dedupe.rs` — private helper module exposing `dedupe_conflicts` and pure-function unit tests (keeps `lib.rs` uncluttered; matches existing per-concern module style in the crate).

## Step-by-step
1. Add `crates/xifty-cli/src/conflict_dedupe.rs` with `pub(crate) fn dedupe_conflicts(conflicts: Vec<xifty_core::Conflict>) -> Vec<xifty_core::Conflict>` that (a) preserves input order, (b) keys by `(field.clone(), sorted_vec<(namespace, tag_id)>)` over `Conflict.sources`, and (c) when a collision occurs prefers the entry with more `sources`, breaking ties by the first-seen entry. Conflicts with empty `sources` key on `(field, empty-vec)` and still dedupe against each other — verifiable by unit test.
2. Wire the helper into `crates/xifty-cli/src/lib.rs` near line 411: build `let mut merged = report.conflicts; merged.extend(normalization.conflicts); report.conflicts = dedupe_conflicts(merged);` — verifiable by test that a synthetic `AnalysisOutput` no longer has duplicate `field` strings for the same source fingerprint.
3. Add unit tests in `conflict_dedupe.rs` covering: (a) identical fingerprints → one kept, (b) same `field` but different source set (e.g. `captured_at` across exif+xmp vs exif+quicktime) → both kept, (c) first-seen with populated `sources` beats a later empty-sources duplicate, (d) ordering preserved for distinct entries.
4. Run `cargo test -p xifty-cli` and `cargo insta review` for the `cli_contract` suite — verify `cli_contract__conflicting_png_report.snap` loses its duplicate `captured_at` and `device.model` entries and retains exactly one per (field, fingerprint). Accept deliberately, not with `--accept-all`.
5. Sweep other snapshots under `crates/xifty-cli/tests/snapshots/` (e.g. `extract_mixed_*`, `extract_real_camera_mp4_interpreted`) for duplicated `field` values in `report.conflicts`; review and accept any legitimate shrinkage.
6. Run the full `validate[]` set from `.loswf/config.yaml` to confirm no regression across the workspace or FFI.

## Tests
- Unit tests in `crates/xifty-cli/src/conflict_dedupe.rs` (pure-function; no fixtures).
- Existing integration/insta snapshots at `crates/xifty-cli/tests/cli_contract.rs` — especially `conflicting_png_report` — serve as end-to-end verification after review.
- No new fixtures required; the existing conflicting PNG fixture already exercises both emitters simultaneously.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- Snapshot churn: multiple snapshots likely shift. Guardrail is explicit: review via `cargo insta review`, never blanket-accept (per `.loswf/config.yaml` guardrails).
- Fingerprint granularity: if validate and policy emit overlapping-but-not-identical source sets (e.g. validate picks two sides, policy includes a third), the current key would keep both. That is intentional (different evidence surfaces), but worth calling out in a code comment so reviewers do not tighten the key to `field`-only and hide legitimate distinct conflicts.
- Upstream alternative (moving dedup into `xifty-policy` or `xifty-validate`) is rejected here because both crates are also consumed directly by `xifty-ffi` and `xifty-json`, and the duplication only manifests when both are composed — fixing at composition site keeps the two crates' individual semantics unchanged.

