## Context

Reviewer findings from PR #127 (Phase 1 sidecar framework + Sony NRT adapter, #122) — non-blocking advisory items captured here for follow-up.

## Item 1 — promote \`sidecar_parse_error\` to \`pub const\`

**File:** \`crates/xifty-sidecar-sony-nrt/src/lib.rs:473\`

The Sony NRT adapter emits \`\"sidecar_parse_error\"\` as a raw string literal when \`quick-xml\` parsing fails. All other sidecar issue codes (\`sidecar_target_missing\`, \`sidecar_no_index_entry\`, \`sidecar_unknown_schema_version\`, \`sidecar_discovery_unavailable_in_wasm\`) are stable \`pub const\`s in \`crates/xifty-sidecar/src/lib.rs\`.

This creates a stability gap: the literal can drift silently across refactors and the value is not documented in \`CAPABILITIES.json\` or \`docs/SCHEMA_POLICY.md\`. Consumers filtering on issue codes will miss it.

**Fix:**
- Add \`pub const SIDECAR_PARSE_ERROR: &str = \"sidecar_parse_error\";\` to \`xifty-sidecar/src/lib.rs\` alongside the other four constants.
- Update the sony-nrt callsite to use the constant.
- Add the code to the documented sidecar issue codes list.

## Item 2 — fix \`MergePolicy::Override\` doc comment

**File:** \`crates/xifty-sidecar/src/lib.rs:182-184\`

Current doc comment claims \"Conflict-detection downstream catches the cross-source disagreement\" — but the actual implementation removes the embedded entry via \`entries.retain(...)\` before pushing the sidecar entry. Only one entry survives; the conflict detector sees nothing to flag. The behavior is correct for a true override (silent replacement), but the comment will mislead Phase 4 (#125) implementers building the Adobe XMP sidecar adapter.

**Fix:** rewrite the comment to:
\`\`\`
/// Sidecar entries silently replace embedded entries for the same field.
/// The embedded value does NOT reach the conflict detector — only the sidecar value does.
/// Use this for sidecars whose entire purpose is overriding embedded metadata
/// (e.g. Adobe XMP sidecars carrying non-destructive Lightroom edits).
\`\`\`

## Item 3 — minor: \`attr_value\` does not decode XML entity references

**File:** \`crates/xifty-sidecar-sony-nrt/src/lib.rs:390-403\`

Currently reads \`attr.value\` raw. For all known Sony NRT attribute payloads (UMID hex, FPS strings, codec names, serial numbers) this is fine — none carry encoded entities. Future Sony schema versions or device names with \`&amp;\` would surface as raw entity strings.

**Fix:** add an inline comment documenting the assumption + cite which Sony NRT attribute values are guaranteed-no-entities. Or call \`attr.decode_and_unescape_value(reader)\` if cheap; benchmark first.

## Item 4 — minor style cleanup

**File:** \`crates/xifty-sidecar-sony-nrt/src/lib.rs:161-162\`

\`#[allow(unused_assignments)]\` is suppressing a legitimate lint. The \`schema\` variable is initialized to \`Unknown\` then overwritten on the first \`NonRealTimeMeta\` element. Could be cleaner as a direct call once the element is encountered. Style only.

## Acceptance criteria

- All four items addressed.
- No behavior change visible to consumers (these are internal hygiene items).
- Validation per \`.loswf/config.yaml\` passes.

## Out of scope

- Tightening the synthetic-fixture integration test to assert all 12 normalized fields → separate issue.
- Sidecar-aware smoke harness in XIFtyNode → separate issue (different repo).
