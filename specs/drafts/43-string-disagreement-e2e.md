<!-- loswf:plan -->
# Plan #43: End-to-end CLI contract test for cross-namespace conflict detection

## Problem
The three `xifty-validate` conflict rules in `crates/xifty-validate/src/rules.rs` (cross-namespace string disagreement at lines 94-137, timestamp timezone-offset mismatch at lines 179-229, numeric precision mismatch at lines 257-295) are covered only by synthetic-entry unit tests in `crates/xifty-validate/tests/conflicts.rs`. The CLI contract suite in `crates/xifty-cli/tests/cli_contract.rs` has `conflicting_png_report_*` tests (lines 489-530 and 620-626), but those exercise the policy-layer conflict path — the existing snapshot `crates/xifty-cli/tests/snapshots/cli_contract__conflicting_png_report.snap` only contains policy-formatted messages like `"multiple candidates disagreed; selected ... from ..."`. A regression that silently dropped the `detect_conflicts` output on the path from `build_report` (`crates/xifty-cli/src/lib.rs:476`) into `report.conflicts` would not be caught by any CLI-layer test today.

## Approach
Scope the new fixture to trigger only the cross-namespace string-disagreement rule on `device.make` (Canon vs Nikon). This satisfies the issue's stated acceptance criterion verbatim ("at least one CLI contract test asserts that `report.conflicts` is non-empty for a fixture with a known cross-namespace disagreement, exercising the `xifty-validate` rule path end-to-end") and avoids three known dead-ends identified by the plan reviewer:

- The timestamp-offset rule cannot fire via an EXIF/XMP PNG fixture today: `parse_timestamp_offset` in `rules.rs` lines 139-177 requires bytes 4 and 7 of the wall portion to be `-` (hyphens). EXIF's `DateTimeOriginal` format (`YYYY:MM:DD HH:MM:SS`, emitted at `tools/generate_fixtures.py:55`) uses colons in those positions and is rejected. There is no normalization rewrite between decoding and `build_report`: `normalize_with_policy(&entries)` at `crates/xifty-cli/src/lib.rs:475` is called for the separate normalized view; `build_report(issues, &entries)` at line 476 receives the raw entries. The only remaining avenues (add a second XMP-style timestamp namespace, or fix `parse_timestamp_offset` to accept colon-separated EXIF) are out of scope for #43.
- The numeric precision rule is also out of scope. `build_tiff` already emits ISO tag `0x8827` at `tools/generate_fixtures.py:115` (the prior plan incorrectly claimed otherwise), but the value is a plain integer 200 and triggering the relative-mismatch comparator in `rules.rs:249-255` would require the XMP side to emit a meaningfully different numeric ISO value — a deliberate expansion that risks perturbing unrelated fixtures and snapshots. Deferred to a follow-up.

Mirror the existing `conflicting.png` fixture-authoring pattern: add a `make="XIFtyCam"` kwarg to `build_tiff` (byte-identical default preserves every existing fixture), then register a new `validate_conflicts.png` built from `build_tiff(make="Canon")` and `build_xmp(make="Nikon", ...)` — `build_xmp` already accepts `make=` at `tools/generate_fixtures.py:286`, no changes needed there. Add a targeted assertion test plus a snapshot test in `crates/xifty-cli/tests/cli_contract.rs` mirroring the `conflicting_png_report_*` pair. The validate-rule message is textually distinguishable from policy-layer output: validate emits `exif:Make=Canon vs xmp:Make=Nikon` per the format string at `rules.rs:129-132`; policy emits `multiple candidates disagreed; selected ...`.

## Files to touch
- `tools/generate_fixtures.py` — add `make="XIFtyCam"` kwarg to `build_tiff` (signature at line 38-48) and replace the hardcoded `b.ascii_blob("XIFtyCam")` at line 52 with `b.ascii_blob(make)`. Register new `validate_conflicts.png` entry in the `files` dict around line 682.
- `crates/xifty-cli/tests/cli_contract.rs` — add two tests near the `conflicting_png_report_*` pair (after line 530, and after line 626): an assertion-style `validate_rules_fire_end_to_end_on_cross_namespace_fixture` and a snapshot-style `validate_conflicts_png_report_snapshot`.

## New files
- `fixtures/minimal/validate_conflicts.png` — regenerated PNG fixture whose EXIF Make=`Canon` disagrees with XMP `tiff:Make`=`Nikon`. `device.make` is lowercased by `canonicalize_string` (`rules.rs:77-84`) before the bucket comparison, so `Canon` (canon) vs `Nikon` (nikon) is guaranteed to be bucketed as two distinct canonical values and emit the conflict. The raw (non-lowercased) values `Canon` and `Nikon` appear in the conflict message because `rules.rs:125-126` captures `a_val`/`b_val` from `string_value(&entry.value)` before canonicalization.
- `crates/xifty-cli/tests/snapshots/cli_contract__validate_conflicts_png_report.snap` — insta-managed snapshot, auto-created on first `cargo insta accept`.

## Step-by-step

1. Edit `tools/generate_fixtures.py` `build_tiff` signature (lines 38-48) to add `make="XIFtyCam"` as a keyword-only argument, and change line 52 from `make = b.ascii_blob("XIFtyCam")` to `make = b.ascii_blob(make)` (the local variable shadows the kwarg intentionally; or rename to `make_blob = b.ascii_blob(make)` and update the two references at lines 68 and 75 — the rename is cleaner). Outcome: every existing `build_tiff(...)` call is byte-identical because the default remains `"XIFtyCam"`.

2. In `main()` of `tools/generate_fixtures.py`, near line 640, add:
   ```python
   validate_conflicts_xmp = build_xmp(make="Nikon")
   ```
   No `create_date` override; the default is fine since we are not exercising the timestamp rule. `build_xmp` already accepts `make=` (line 286), no generator changes needed there. Outcome: an XMP payload with `tiff:Make="Nikon"` ready to pair with a `Canon` EXIF.

3. Add the new fixture entry to the `files` dict around line 682:
   ```python
   "validate_conflicts.png": build_png(build_tiff(gps=False, make="Canon"), validate_conflicts_xmp),
   ```
   Run `python3 tools/generate_fixtures.py`. Outcome: `fixtures/minimal/validate_conflicts.png` is written and no other fixture byte-changes (the generator is deterministic and the `make=` default is preserved everywhere else). Confirm via `git status` — only the new fixture file and generator diff should appear; if any other `.png`/`.jpg`/`.tiff`/`.webp`/`.heic`/`.mp4` changed, step 1 leaked the default and must be corrected.

4. In `crates/xifty-cli/tests/cli_contract.rs`, after the existing `conflicting_png_report_exposes_source_namespaces` test (ends line 530), add:
   ```rust
   #[test]
   fn validate_rules_fire_end_to_end_on_cross_namespace_fixture() {
       // Exercises the xifty-validate cross-namespace string-disagreement rule
       // end-to-end: extract -> build_report -> report.conflicts.
       // Timestamp and numeric rules deferred — see issue #43 plan notes.
       let output = extract_json("validate_conflicts.png", ViewMode::Report);
       let conflicts = output["report"]["conflicts"].as_array().unwrap();
       let make_conflict = conflicts
           .iter()
           .find(|c| {
               c["field"] == "device.make"
                   && c["message"].as_str().is_some_and(|m| {
                       // Validate-rule format: "exif:Make=Canon vs xmp:Make=Nikon"
                       // (raw values, not lowercased — see rules.rs:125-132).
                       m.contains("Canon") && m.contains("Nikon") && m.contains(" vs ")
                   })
           })
           .expect("missing validate-rule device.make conflict");
       let sources = make_conflict["sources"].as_array().expect("sources array");
       let namespaces: std::collections::BTreeSet<&str> = sources
           .iter()
           .filter_map(|side| side["provenance"]["namespace"].as_str())
           .collect();
       assert!(
           namespaces.contains("exif") && namespaces.contains("xmp"),
           "expected both exif and xmp namespaces, got {:?}",
           namespaces
       );
   }
   ```
   The message-format assertion (`"Canon"` + `"Nikon"` + `" vs "`) is what distinguishes the validate-rule path from the policy-layer path, whose messages use `"multiple candidates disagreed; selected ... from ..."`. Outcome: a failing build if the validate output is ever dropped before reaching `report.conflicts`.

5. After the existing `conflicting_png_report_snapshot` (ends line 626), add:
   ```rust
   #[test]
   fn validate_conflicts_png_report_snapshot() {
       assert_json_snapshot!(
           "validate_conflicts_png_report",
           extract_json("validate_conflicts.png", ViewMode::Report)
       );
   }
   ```
   Run `cargo test -p xifty-cli --test cli_contract validate_conflicts_png_report_snapshot` once, then `cargo insta review` to accept the new `cli_contract__validate_conflicts_png_report.snap`. Outcome: deliberate acceptance of the snapshot; future wire-output drift requires another intentional `cargo insta review`.

6. Run the full validation gate (`cargo fmt --all -- --check`, `cargo test --workspace --all-features`, `cargo test -p xifty-ffi --all-features`) and confirm (a) only the new snapshot is pending, (b) no existing snapshot drifted, (c) `cli_contract__conflicting_png_report.snap` and all other fixture-consuming snapshots remain byte-identical. Outcome: green validation, single net-new snapshot accepted.

## Tests
- New: `validate_rules_fire_end_to_end_on_cross_namespace_fixture` in `crates/xifty-cli/tests/cli_contract.rs` — asserts the validate-rule message shape survives to `report.conflicts`.
- New: `validate_conflicts_png_report_snapshot` in same file — `assert_json_snapshot!` for ongoing regression coverage (deliberate acceptance via `cargo insta review`).
- Existing: `crates/xifty-validate/tests/conflicts.rs` is unchanged.
- Existing: `conflicting_png_report_snapshot` and all other fixture-consuming snapshots must remain byte-identical — regression guard on the `make=` kwarg default.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`
- Expect one insta-pending snapshot after first run: `cargo insta review` (accept the new `cli_contract__validate_conflicts_png_report.snap`; reject any other drift).

## Risks
- **Generator default leak**: the `make=` kwarg must default to `"XIFtyCam"` so every existing PNG/JPEG/TIFF/HEIC/WebP/MP4 fixture regenerates byte-identically. Any other fixture byte-change is a red flag — do not blanket-accept snapshots; fix the default.
- **Deferred rules**: the timestamp-offset and numeric-precision rules remain uncovered at the CLI contract layer after this change. File follow-up issues for either (a) teaching `parse_timestamp_offset` to accept EXIF colon format, or (b) extending `build_xmp` to emit a mismatching ISO value. Out of scope for #43.
- **Snapshot of new fixture**: first-run will write a pending `.snap`; reviewer must run `cargo insta review`, not `cargo insta accept --all`, to conform to the guardrail ("never blanket-accept").
- **Canonicalization shadowing**: `canonicalize_string` lowercases `device.make` values (`rules.rs:77-84`), but the conflict message uses the raw values captured at lines 125-126. The assertion on `"Canon"` + `"Nikon"` (mixed case) is correct — do not lowercase the expected substrings.

