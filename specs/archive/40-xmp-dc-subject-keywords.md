<!-- loswf:plan -->
# Plan #40: XMP `dc:subject` keyword bag extraction (revised)

## Problem
`crates/xifty-meta-xmp/src/lib.rs::decode_packet` has no extraction path for `dc:subject`, so XMP packets produced by Lightroom/Capture One/Photoshop/Bridge never populate the `Keywords` metadata entry. Downstream, `crates/xifty-normalize/src/lib.rs::entry_strings` (lines 183-197) aggregates every `Keywords` string into the normalized `keywords` field — but it never receives any because the decoder never calls `push_string` for `dc:subject`. Result: files with non-empty XMP keyword bags surface an empty `keywords` field in normalized output.

## Approach
Add a bag-aware helper `find_element_all` that returns every `<rdf:li>…</rdf:li>` payload inside the first matched `<{name}>…</{name}>` element, and a `push_string_multi` helper that pushes one `MetadataEntry` per decoded value. Wire `dc:subject` → `Keywords` from `decode_packet`. This mirrors the IPTC path in `crates/xifty-meta-iptc/src/lib.rs:101` which emits one `Keywords` entry per keyword, and the existing `dc:creator` wiring at `crates/xifty-meta-xmp/src/lib.rs:111-118` for namespace/tag plumbing.

**Sequencing with #42 (dependency — blocking):** #42 is in `factory:phase:building` (unmerged as of planning). #42 rewrites the `<rdf:li` needle inside `find_element` (`lib.rs:285`) into a helper (working name `find_rdf_li`) that accepts both bare `<rdf:li>` and attributed `<rdf:li xml:lang="…">` opening tags. Our new `find_element_all` must scan for `rdf:li` with the same broadened matcher — otherwise attributed keyword bags (which the XMP spec permits on `rdf:Seq`) will silently drop values. **Preferred merge order: #42 first, then #40 rebases on top and reuses #42's `find_rdf_li` helper.** Builder action:

1. Before starting, check whether #42 has merged (`gh pr list --search "issue:42" --state merged`).
2. If #42 is merged: rebase this branch onto `main` and implement `find_element_all` by calling #42's `find_rdf_li` helper in a loop.
3. If #42 is NOT merged: do not start this issue until #42 merges, OR — if parallel work is required — extract the attributed-`<rdf:li` scan as a small private helper in this PR with the exact same semantics #42's plan specifies (accept both `<rdf:li>` and `<rdf:li{WS}…>`; terminate on the first `>` after `<rdf:li`; read body up to `</rdf:li>`), and coordinate with the #42 author to reconcile during #42's rebase. The shared helper must live in `crates/xifty-meta-xmp/src/lib.rs` so both `find_element` (post-#42) and `find_element_all` (this PR) call the same needle-scan logic.

`x-default` preference is not relevant for `dc:subject` (it is `rdf:Seq`/`rdf:Bag`, not `rdf:Alt`) so that part of #42 is orthogonal to this work.

## Files to touch
- `crates/xifty-meta-xmp/src/lib.rs` — add `find_element_all` (new helper mirroring `find_element` at `:281-302` but looping `rdf:li` children using the shared attributed-`<rdf:li` matcher from / for #42); add `push_string_multi` (mirroring `push_string` at `:196-226`, hardcoding non-timestamp semantics); add one `push_string_multi(..., "Keywords", "Keywords", find_element_all(text, "dc:subject"))` call inside `decode_packet` after the `Description` push at `:135-142`; extend the `xmp_decoder_extracts_supported_fields` test at `:317-330` with a `dc:subject` Bag assertion.
- `tools/generate_fixtures.py` — add `keywords: list[str] | None = None` kwarg to `build_xmp` (`:284-320`); when set, emit `<dc:subject><rdf:Bag><rdf:li>…</rdf:li>…</rdf:Bag></dc:subject>` inside the `<x:xmpmeta>` block; pass `keywords=["alpha", "beta"]` at the `xmp.tiff` call site (`:668`).
- `fixtures/minimal/xmp.tiff` — regenerated binary via `python tools/generate_fixtures.py`.
- `crates/xifty-cli/tests/snapshots/cli_contract__extract_xmp_tiff_normalized.snap` — updated snapshot. Expect a new `keywords` field (value `"alpha, beta"` per `entry_strings` join behavior) and shifted `offset_end` values on xmp-sourced provenance entries (the XMP packet grew). Review diff by eye per the `cargo insta review` guardrail — do not blanket-accept.
- `CAPABILITIES.json` — add `supported_tags: ["Keywords"]` under `namespaces.xmp` (currently `{"status": "bounded"}` at `:8-10`), adding the `supported_tags` key. Do NOT enumerate pre-existing XMP tags — that is a separate hygiene issue and explicitly out of scope per plan-review feedback. Only `Keywords` (the newly-extractable tag introduced by this PR) is added.

## New files
- None. Fixture generator is extended rather than adding a new fixture file, to keep the normalized-snapshot surface minimal and because `xmp.tiff` already exercises the XMP-only path.

## Step-by-step
1. **Dependency check.** Run `gh pr list --search "issue:42" --state merged --json number,mergedAt`. If #42 is merged: rebase onto `main`. If not: either wait, or extract the shared attributed-`<rdf:li` matcher helper in this PR per the Approach section. Verifiable: the branch base contains #42's `find_rdf_li` (or equivalent shared helper) before step 2 begins.
2. In `crates/xifty-meta-xmp/src/lib.rs`, add `fn find_element_all(xml: &str, name: &str) -> Vec<DecodedText>`: find the first `<{name}>` open tag, take the slice up to its matching `</{name}>`, and within that slice iterate every `<rdf:li…>…</rdf:li>` pair (using the shared matcher from step 1). Each match becomes a `DecodedText { value: xml_unescape(inner), note: format!("decoded from xmp rdf:li inside {name}") }`. Verifiable via unit test in step 5 asserting a two-`rdf:li` Bag returns a 2-element Vec with the correct strings.
3. Add `fn push_string_multi(entries: &mut Vec<MetadataEntry>, packet: XmpPacket<'_>, tag_id: &str, tag_name: &str, values: Vec<DecodedText>)` mirroring `push_string` at `:196-226` but iterating the Vec and pushing one entry per decoded value. Hardcode `TypedValue::String` (keywords are never timestamps) — do NOT include a `timestamp: bool` parameter; document the divergence from `push_string` in a short comment. Verifiable: two `Keywords` entries emitted from a two-item Bag in the unit test in step 5.
4. In `decode_packet`, after the `Description` push at `:135-142`, insert `push_string_multi(&mut entries, packet.clone(), "Keywords", "Keywords", find_element_all(text, "dc:subject"));`. Verifiable: new unit test + updated snapshot.
5. Extend the `xmp_decoder_extracts_supported_fields` test (`:317-330`) with `dc:subject` added to the fixture XML as `<dc:subject><rdf:Bag><rdf:li>alpha</rdf:li><rdf:li>beta</rdf:li></rdf:Bag></dc:subject>`. Assert `entries.iter().filter(|e| e.tag_name == "Keywords").count() == 2` and that each `Keywords` entry carries a `xmp rdf:li inside dc:subject` note. Verifiable: `cargo test -p xifty-meta-xmp` passes.
6. Extend `build_xmp` in `tools/generate_fixtures.py` (`:284-320`) with `keywords: list[str] | None = None`; when set, append `<dc:subject><rdf:Bag>{"".join(f"<rdf:li>{kw}</rdf:li>" for kw in keywords)}</rdf:Bag></dc:subject>` before `</x:xmpmeta>`. Leave `build_editorial_xmp` (`:323-352`) untouched to avoid disturbing the `overlap_editorial.jpg` snapshot. Update the `xmp.tiff` entry at `:668` to `build_tiff(gps=False, xmp_payload=build_xmp(keywords=["alpha", "beta"]))` (note: the `xmp` local built at ~`:650s` is reused by `xmp.tiff` — either rebind that variable with `keywords=...` or pass a fresh `build_xmp(...)` call inline; builder to pick the minimum-diff form). Regenerate: `python tools/generate_fixtures.py`.
7. Run `cargo test --workspace --all-features`. The `cli_contract__extract_xmp_tiff_normalized` snapshot will fail. Review the diff carefully (`cargo insta review`): expect a new `keywords: ["alpha", "beta"]` (or `"alpha, beta"` string depending on normalized schema) field plus shifted xmp provenance `offset_end` values because the packet grew. Confirm no other snapshots moved — `xmp_only.*` fixtures and `overlap_editorial.jpg` were deliberately untouched. Accept only if the diff matches expectations.
8. Update `CAPABILITIES.json`: change `"xmp": { "status": "bounded" }` (`:8-10`) to `"xmp": { "status": "bounded", "supported_tags": ["Keywords"] }`. Do not add other tag names. Verifiable: diff review; any schema validation tests in the workspace pass.

## Tests
- **Unit** (primary correctness gate): extend `xmp_decoder_extracts_supported_fields` in `crates/xifty-meta-xmp/src/lib.rs:317` with a two-`rdf:li` `dc:subject` Bag; assert exactly two `Keywords` entries with values `"alpha"` and `"beta"` and non-empty provenance notes. Preserves the existing `dc:creator` single-`rdf:li` assertion as a no-regression guard.
- **Snapshot** (end-to-end gate): `cli_contract__extract_xmp_tiff_normalized.snap` regeneration proves the decoder → normalize → CLI JSON path populates `keywords`.
- **Regression**: existing `Author` assertion in the unit test and the unchanged `xmp_only.*` / `overlap_editorial.jpg` snapshots confirm no disturbance to other XMP extraction paths.

## Validation
Per `.loswf/config.yaml` `validate[]` (lines 39-43):
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- **#42 ordering**: The dependency on #42's `find_rdf_li` helper is explicit and blocking. If the builder starts before #42 merges and extracts the shared helper locally, they MUST coordinate with the #42 branch author to avoid a silent divergence in matcher semantics during #42's rebase. A broken merge here means attributed `<rdf:li xml:lang="…">` entries inside keyword Bags will silently drop — the same defect class #42 is fixing for `rdf:Alt`.
- **Snapshot churn scope**: xmp-sourced entries in `cli_contract__extract_xmp_tiff_normalized.snap` will have `offset_end` shifts because the XMP packet length changed. This is expected but must be reviewed per-entry, not blanket-accepted (workspace guardrail).
- **Normalize schema shape**: `entry_strings` (`crates/xifty-normalize/src/lib.rs:183-197`) aggregates all `Keywords` strings into a list but only the first entry's provenance is surfaced. Pre-existing behavior shared with IPTC — not a regression, but noted so the snapshot showing a single provenance source is not misread as a bug.
- **`>` inside attribute values on `<rdf:li`**: shared with #42's risk — a pathological entity-encoded `>` in `xml:lang` would mis-terminate the scan. Accepted risk; deferred to a future `quick-xml` migration.
- **`supported_tags` narrowness**: Deliberately adding only `Keywords`. The broader question of whether `xmp.supported_tags` should enumerate all historically-extracted tags is a separate hygiene issue and out of scope here per plan-review feedback.

