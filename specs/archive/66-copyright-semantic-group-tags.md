# Plan #66: fix validate copyright semantic group tag names

## Problem
`SEMANTIC_GROUPS` in `crates/xifty-validate/src/rules.rs` (lines 33–39) defines the `copyright` group with members `("xmp", "rights")` and `("iptc", "CopyrightNotice")`. Neither matches what the namespace parsers emit: `xifty-meta-xmp` emits `tag_name: "Copyright"` for `dc:rights` (see `crates/xifty-meta-xmp/src/lib.rs:122-124`) and `xifty-meta-iptc` emits `tag_name: "Copyright"` for dataset `(2, 116)` (see `crates/xifty-meta-iptc/src/lib.rs:102`). Because `detect_cross_namespace_disagreement` at `crates/xifty-validate/src/rules.rs:94-137` matches on `(namespace, tag_name)` equality, the XMP and IPTC copyright entries are never collected and genuinely conflicting copyright values across EXIF/XMP/IPTC produce zero conflicts in the report.

## Approach
Align the `copyright` group in `SEMANTIC_GROUPS` with the tag names actually emitted by the parsers — change both the XMP and IPTC entries to `"Copyright"`. Then add a unit test in `crates/xifty-validate/tests/conflicts.rs` (mirroring the structure of the existing `cross_namespace_string_disagreement_is_reported` test at lines 48-69) that synthesises EXIF/XMP/IPTC copyright entries with different values and asserts exactly one `copyright` conflict is reported. This mirrors the existing pattern and adds no new API surface.

## Files to touch
- `crates/xifty-validate/src/rules.rs` — update `SEMANTIC_GROUPS` copyright entries at lines 36-37.
- `crates/xifty-validate/tests/conflicts.rs` — add cross-namespace copyright conflict test using the existing `string_entry` helper.

## New files
- None.

## Step-by-step
1. In `crates/xifty-validate/src/rules.rs` line 36, change `("xmp", "rights")` to `("xmp", "Copyright")` — XMP parser's emitted tag name.
2. In `crates/xifty-validate/src/rules.rs` line 37, change `("iptc", "CopyrightNotice")` to `("iptc", "Copyright")` — IPTC parser's emitted tag name.
3. In `crates/xifty-validate/tests/conflicts.rs`, add a test `cross_namespace_copyright_disagreement_is_reported` that builds three `string_entry` rows — `("exif","Copyright","(c) Alice")`, `("xmp","Copyright","(c) Bob")`, `("iptc","Copyright","(c) Carol")` — calls `build_report(Vec::new(), &entries)`, filters `report.conflicts` for `c.field == "copyright"`, and asserts exactly one conflict with a message that mentions at least two of the three namespaces.
4. Optionally add a companion agreement test `copyright_agreement_across_namespaces_produces_no_conflict` (same three namespaces, identical values) asserting no `copyright` conflict is emitted — mirrors the existing agreement test at lines 122-140.

## Tests
- `crates/xifty-validate/tests/conflicts.rs` — new `cross_namespace_copyright_disagreement_is_reported` (required by acceptance criteria) and optional `copyright_agreement_across_namespaces_produces_no_conflict`.
- Existing tests in the same file must continue to pass unchanged.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- Other call sites or fixtures that encode the old tag strings `"rights"` or `"CopyrightNotice"` could exist; a repo-wide grep during build should confirm `SEMANTIC_GROUPS` is the only consumer. Snapshot tests under `crates/xifty-validate` that exercise fixtures containing XMP `dc:rights` or IPTC `(2,116)` may begin reporting a new `copyright` conflict — any insta snapshot changes must be reviewed manually (per guardrails), not blanket-accepted.
- The conflict-emission path picks the first two canonicalised values in `BTreeMap` key order (`rules.rs:113-124`); the new test must assert membership rather than exact ordering of the two namespaces in the message.
