<!-- loswf:plan -->
# Plan #43 v3: Prove xifty-validate rule path fires end-to-end via captured_at

## Problem
PR #48 v2 added `validate_conflicts.png` (EXIF Make=Canon vs XMP Make=Nikon) and
asserted on `report.conflicts[device.make]` with both namespaces + raw values
visible. The reviewer correctly noted this does not prove `xifty-validate`
actually ran: `xifty-policy` alone already emits a `device.make` conflict with
the same `sources` set (both exif+xmp, both raw values preserved). The CLI
dedupe at `crates/xifty-cli/src/conflict_dedupe.rs:42-57` collapses the
validate entry into the policy entry when fingerprints match, tie-breaking to
the policy "selected" message. Both the named assertion and the snapshot would
pass unchanged if `detect_conflicts` were silently removed from the pipeline
— the regression is invisible.

## Approach
Narrow Option 2 from the reviewer. There already exists a field on the
existing fixture where validate fires but policy does NOT emit a conflict:
`captured_at`. The EXIF value is `2024:04:16 12:34:56` (colon date) and the
XMP value is `2024-04-16T12:34:56`; policy's `normalize_timestamp`
(`xifty-policy/src/lib.rs:575-592`) maps the colon form to the ISO form, so
`typed_values_equal` reports no material difference and **policy emits no
`captured_at` conflict**. Validate's `detect_cross_namespace_disagreement`
(`xifty-validate/src/rules.rs:94-137`) canonicalises with plain
`trimmed.to_string()`, treats the two timestamp strings as distinct, and
emits a conflict with the distinctive `"xmp:CreateDate=... vs
exif:DateTimeOriginal=..."` message. The PR #48 snapshot already captures
this as the first entry of `report.conflicts`, confirming the emission.

Replace the misleading `device.make` named assertion with one that targets
the `captured_at` conflict and asserts on three things only policy-free
emission can satisfy: (1) the conflict exists, (2) its message is the
validate-rule format (contains `" vs "` and both tag names and does NOT
contain `"selected"`), (3) both raw timestamp strings (`2024:04:16 12:34:56`
and `2024-04-16T12:34:56`) survive to `report.conflicts[].sources[].value`.
If `detect_conflicts` is removed or the cross-namespace string rule
regresses, no conflict remains for `captured_at` (policy is silent on it)
and the assertion fails at the `.expect("missing captured_at ...")` call.
The snapshot also drifts because the first conflict entry disappears.

The fixture, the `make=` kwarg in `tools/generate_fixtures.py`, and the
`validate_conflicts_png_report_snapshot` test stay exactly as PR #48
introduced them — those are fine. The only change is the body of
`validate_rules_fire_end_to_end_on_cross_namespace_fixture` and its doc
comment. The snapshot file on disk is unchanged (still captures the
canonical wire output including the validate-authored captured_at entry).

No fixture regeneration, no new files, no policy/validate code changes. The
reviewer's Option 3 (rewriting dedupe tie-break) is explicitly rejected as
out-of-scope scope creep.

## Files to touch
- `crates/xifty-cli/tests/cli_contract.rs` — rewrite the body and doc
  comment of `validate_rules_fire_end_to_end_on_cross_namespace_fixture`
  (PR #48 lines 532-574) so the assertion targets `captured_at` with the
  validate-path message shape and raw timestamp values, and explains why
  this field is load-bearing (policy normalises timestamps → no policy
  conflict → only validate can produce this entry).

## New files
- none

## Step-by-step
1. In `crates/xifty-cli/tests/cli_contract.rs`, replace the body of
   `validate_rules_fire_end_to_end_on_cross_namespace_fixture` with a
   `captured_at`-targeted assertion:
   - Call `extract_json("validate_conflicts.png", ViewMode::Report)`.
   - Locate the `captured_at` conflict via `.find(|c| c["field"] ==
     "captured_at").expect("missing captured_at conflict in report.conflicts
     — xifty-validate detect_cross_namespace_disagreement appears not to
     have fired; xifty-policy does not emit this field because
     normalize_timestamp collapses EXIF colon-date and XMP ISO-date to the
     same canonical form")`.
   - Assert on `message`: contains `" vs "`, contains `"CreateDate"`,
     contains `"DateTimeOriginal"`, and does NOT contain `"selected"`. The
     negative assertion on `"selected"` is the load-bearing piece — it
     rejects the policy message format and therefore rejects a regression
     where captured_at is produced by policy alone.
   - Assert that `sources` has at least two sides with namespaces `{exif,
     xmp}` and that the raw value strings `{"2024:04:16 12:34:56",
     "2024-04-16T12:34:56"}` both appear in `sources[].value.value`.
   - Expected outcome: test passes against the current
     `validate_conflicts_png_report.snap` (which already carries the
     validate-path captured_at entry as conflict [0]); test would fail if
     `xifty-validate::rules::detect_conflicts` is removed or if
     `detect_cross_namespace_disagreement`'s string-disagreement branch
     regresses.
2. Rewrite the doc comment above the test (PR #48 lines 534-543) to
   explain the captured_at argument: cite `xifty-policy/src/lib.rs:575-592`
   (`normalize_timestamp`) as the reason policy is silent here, cite
   `xifty-validate/src/rules.rs:94-137` as the rule being exercised, and
   state plainly that the `"selected"`-negative message assertion is what
   makes the test fail if validate is dropped. Remove the misleading
   "collapsed against the policy-layer conflict" language — on this field
   there is nothing to collapse against.
   - Expected outcome: future readers understand the test's guarantee.
3. Run the validate suite from config.yaml (fmt, workspace, ffi). The
   existing `validate_conflicts_png_report.snap` must continue to pass
   unchanged; no `cargo insta review` step is required because we are not
   modifying the snapshot or the fixture.
   - Expected outcome: all three validate steps green, no new pending
     snapshots.

## Tests
- Rewritten named assertion `validate_rules_fire_end_to_end_on_cross_namespace_fixture`
  in `crates/xifty-cli/tests/cli_contract.rs`.
- Existing snapshot test `validate_conflicts_png_report_snapshot` (unchanged)
  continues to provide wire-level regression coverage.
- Existing unit tests in `crates/xifty-validate/tests/conflicts.rs` and
  `crates/xifty-cli/src/conflict_dedupe.rs` tests remain untouched.

### Regression posture (what the new test catches)
- If someone deletes the `out.extend(detect_cross_namespace_disagreement(
  entries, SEMANTIC_GROUPS));` line from `xifty-validate/src/rules.rs:68-71`,
  the captured_at conflict disappears entirely (policy doesn't emit it) and
  the `.expect` fires.
- If someone silently swaps validate's message format to start with
  `"selected"` or removes the `" vs "` infix, the message assertions fire.
- If someone changes `canonicalize_string` to also normalise timestamps
  (making validate agree with policy on captured_at), the canonical values
  collapse and no validate conflict is emitted → `.expect` fires. This is
  an intentional guard: if we ever want that behaviour, the test must be
  updated deliberately.
- If policy starts emitting a captured_at conflict alongside validate (e.g.
  someone weakens `normalize_timestamp`), the dedupe fingerprint will match
  (both have sources={(exif, 0x9003), (xmp, CreateDate)}) and tie-break to
  the "selected" policy message — the negative `"selected"` assertion
  fires. That is also correct: the test asserts the validate path is the
  one producing the observable entry, and if policy takes over the output
  the validate contribution is no longer visible and the test must be
  rethought.

## Validation
- `cargo fmt --all -- --check` (config.yaml validate[0])
- `cargo test --workspace --all-features` (config.yaml validate[1])
- `cargo test -p xifty-ffi --all-features` (config.yaml validate[2])

## Risks
- **Policy timestamp normalisation is the load-bearing asymmetry.** If a
  future change to `xifty-policy` drops or weakens `normalize_timestamp`,
  policy will start emitting captured_at conflicts and the dedupe will
  tie-break to the policy message, failing the `"selected"`-negative
  assertion. This is the intended guard but will surface as a "why did my
  unrelated policy change break this test" moment — the test's doc comment
  must call this out so the next contributor understands.
- **Snapshot stability.** The existing PR #48 snapshot already pins the
  validate-path captured_at entry as `conflicts[0]` with the expected
  message. We are not regenerating it; the rewritten assertion must agree
  with the pinned snapshot. Verify by running `cargo test -p xifty-cli`
  locally before committing.
- **Out-of-scope bug surfaced but not fixed.** Validate's copyright
  semantic group at `xifty-validate/src/rules.rs:33-39` uses tag-name keys
  `"rights"` and `"CopyrightNotice"` that do not match what the XMP/IPTC
  parsers actually emit (both emit tag_name `"Copyright"` — see
  `crates/xifty-meta-xmp/src/lib.rs:122-124` and
  `crates/xifty-meta-iptc/src/lib.rs:102`). That rule is effectively dead
  code on real fixtures. Do NOT fix it in this PR — file a separate issue;
  this plan is narrowly about giving #43 a test that actually proves the
  pipeline it claims to test.
- **No acceptance-criteria drift.** Issue #43 asks for snapshot regression
  coverage (`assert_json_snapshot!`) and a CLI-level assertion that
  exercises the validate rule path. The snapshot is already in place; this
  plan only makes the named assertion actually discriminative.

