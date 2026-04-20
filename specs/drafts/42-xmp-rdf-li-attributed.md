<!-- loswf:plan -->
# Plan #42: XMP find_text substring scan drops bag/alt values

## Problem
`xifty-meta-xmp::find_element` at `crates/xifty-meta-xmp/src/lib.rs:285` locates
`rdf:li` items using the literal needle `"<rdf:li>"`. Lightroom, Capture One,
Photoshop, and Bridge emit the canonical language-alternative form
`<rdf:li xml:lang="x-default">…</rdf:li>`, which that literal never matches —
so `find_element` falls through to the raw element-body branch, capturing the
wrapper markup (`<rdf:Alt><rdf:li xml:lang="x-default">© 2025</rdf:li></rdf:Alt>`)
rather than the inner value, and for `rdf:Alt` the `x-default` preference is
never honored. This systematically corrupts or drops `copyright`
(`dc:rights`), `description` (`dc:description`), and any future `rdf:Alt`- or
attributed-`rdf:Seq`-backed field.

## Approach
Broaden the `rdf:li` match in `find_element` to accept both the bare
`<rdf:li>` and the attributed `<rdf:li …>` forms, and when scanning an
`rdf:Alt` block prefer the `xml:lang="x-default"` alternative before falling
back to the first `rdf:li`. Keep the implementation dependency-free (no new
crates) and mirror the existing substring-scan style in this file — the
pattern already tolerates unprefixed attribute discovery in `find_attr`
(`lib.rs:270-279`) so a symmetric extension for element-form `rdf:li` fits
naturally. This is an isolated, targeted fix to the defect described in the
investigator note; it intentionally stops short of pulling in `quick-xml` or
rewriting the parser (that broader remediation is listed as an alternative
in the issue but is out of scope for this fix).

## Files to touch
- `crates/xifty-meta-xmp/src/lib.rs` — broaden `find_element` (lines 281-302) to match `<rdf:li` with optional attributes and to prefer `xml:lang="x-default"` inside `rdf:Alt`; extend the `tests` module (line 313+) with fixtures covering the Lightroom/Photoshop element-form `rdf:Alt` layout for `dc:rights` and `dc:description`, plus an `rdf:Alt` with a non-default first `li` to prove `x-default` preference.

## New files
- None. All changes fit within the existing crate and its in-file tests.

## Step-by-step
1. In `crates/xifty-meta-xmp/src/lib.rs`, replace the literal `body.find("<rdf:li>")` scan at line 285 with a helper (e.g. `find_rdf_li(body)`) that locates the next `<rdf:li` token, advances past the tag's `>` terminator (handling both `<rdf:li>` and `<rdf:li xml:lang="x-default">`), then reads up to the next `</rdf:li>` — verifiable by a new unit test that feeds `<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">© 2025</rdf:li></rdf:Alt></dc:rights>` and asserts the decoded value is exactly `© 2025` (not the wrapper markup).
2. When the element body contains `<rdf:Alt`, scan all `rdf:li` children and prefer the one whose opening tag contains `xml:lang="x-default"`; fall back to the first `li` when no default exists — verifiable by a unit test with two `rdf:li` siblings (`xml:lang="fr"` first, `xml:lang="x-default"` second) asserting the `x-default` value is chosen.
3. Update the `note` string in `DecodedText` for the new path to remain descriptive (e.g. `"decoded from xmp rdf:Alt x-default inside {name}"` vs the existing `"decoded from xmp rdf:li inside {name}"`) so provenance notes stay traceable — verifiable by asserting the note substring in the new tests.
4. Keep existing `rdf:Seq` + bare `<rdf:li>` behavior unchanged (covered by the existing `dc:creator` test at line 320) — verifiable by the pre-existing `xmp_decoder_extracts_supported_fields` test continuing to pass unmodified.
5. Run the full `validate[]` sequence locally and review any insta snapshot drift (XMP fixtures under the workspace) deliberately via `cargo insta review`, per SRS §4 — verifiable by all three gates going green.

## Tests
- Extend the in-crate `tests` module in `crates/xifty-meta-xmp/src/lib.rs`:
  - `xmp_decoder_handles_rdf_alt_x_default_copyright` — `dc:rights` in element form with `xml:lang="x-default"`; asserts `Copyright` entry exists with the inner string only.
  - `xmp_decoder_handles_rdf_alt_x_default_description` — `dc:description` same shape; asserts `Description` entry value matches expected text.
  - `xmp_decoder_prefers_x_default_over_other_languages` — `rdf:Alt` with `fr` first and `x-default` second; asserts the `x-default` value is chosen.
  - Negative regression: confirm `dc:creator` `rdf:Seq` + `<rdf:li>` path (existing test) still yields `Author` (no change required).
- Workspace snapshot surfaces (`xifty-normalize`, CLI snapshot suites) may pick up newly populated `copyright` / `description` fields if any existing XMP fixtures use the element form; any resulting snapshot diffs must be reviewed via `cargo insta review` rather than blanket-accepted.

## Validation
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- Overlap with issue #40 (`dc:subject` keyword `rdf:Seq` extraction). Issue #40 adds a brand-new `push_string` call for `dc:subject` and does not itself modify `find_element`; once #40 lands it will flow through the same `find_text` path, so the `find_element` broadening in this plan will directly benefit (and must not regress) keyword extraction. If #40 merges first, this PR should rebase cleanly; if this PR merges first, #40 gains a more capable parser with no rework needed. Builders should confirm no merge collision in `decode_packet` and that any new `dc:subject` test in #40 still passes against the broadened `find_element`.
- Insta snapshot churn risk: if existing XMP fixtures carry element-form `dc:rights` or `dc:description` that silently produced garbage strings previously, those snapshots will now change to the correct inner value. That is a desired fix, but builders must review diffs — never blanket-accept — per SRS §4.
- The broadened `<rdf:li` match must stop at the opening tag's `>` before consuming the body; care is needed so a pathological attribute value containing `>` (rare but legal when entity-encoded) does not mis-terminate. Keep the scan simple (find first `>` after `<rdf:li`) and document the assumption in a comment; a quick-xml migration remains the right long-term answer.
- No FFI surface or schema change; `FFI_CONTRACT.md` and `CAPABILITIES.json` untouched.

