<!-- loswf:plan -->
# Plan #105: linux-x64 prebuild — emit filesystem-mtime fallback for `captured_at` on all platforms

## Problem
On the linux-x64 prebuild, an image with no embedded EXIF/XMP capture timestamp returns no `captured_at` / `created_at` field — even though the host's filesystem mtime is set correctly. On darwin-arm64 the same call surfaces the mtime-derived field as expected. Investigation (see issue #105 confirmed comment) traces the regression to `crates/xifty-cli/src/lib.rs:649`, where `add_filesystem_timestamp_fallbacks` returns early unless `is_apple_screen_capture(path)` returns true. `is_apple_screen_capture` (lines 702–710) shells out to the macOS-only `xattr` tool; on Linux `Command::new("xattr").output()` returns `Err`, `.unwrap_or(false)` collapses to `false`, and the platform-neutral `metadata.modified()` / `metadata.created()` branches are never evaluated.

## Approach
Restructure `add_filesystem_timestamp_fallbacks` so the platform-neutral filesystem-mtime/ctime fallback runs unconditionally for PNGs that have no decoded capture-timestamp candidate, and treat the Apple `xattr`/`mdls` enrichment as an additive higher-confidence layer that is only attempted on macOS (or, more conservatively, only when `xattr`/`mdls` are actually available — same observable behavior). The PNG format gate is preserved (the existing surface is PNG-only and broadening it is out of scope for this bug). Confidence semantics, tag-name choices (`FileModifyDate` / `FileCreateDate`), and `MetadataEntry` shape are unchanged — only the gate is moved so the fallback can fire on Linux. The macOS-specific `apple_screenshot_png_uses_preserved_modified_time_when_birthtime_is_copy_time` test continues to pass on macOS, and a new portable test asserts the same `captured_at` / `created_at` emission on any host with a fresh tmpfile whose mtime has been set explicitly.

## Files to touch
- `crates/xifty-cli/src/lib.rs` — restructure `add_filesystem_timestamp_fallbacks` (line 640–700) to drop the `is_apple_screen_capture` precondition for the universal mtime/ctime fallback while keeping the Apple-screencap enrichment additive on macOS; gate the Apple enrichment behind `cfg!(target_os = "macos")` (or a `#[cfg(target_os = "macos")]` helper) so it never shells out on Linux.
- `crates/xifty-cli/tests/cli_contract.rs` — add a portable (non-`target_os`-gated) test that copies `no_exif.png` to a tmpfile, sets a known mtime, calls `extract_path(..., ViewMode::Normalized)`, and asserts both `captured_at` and `created_at` are populated from the filesystem mtime.

## New files
- _(none)_ — no new modules; existing test file gets one additional `#[test]`.

## Step-by-step
1. In `crates/xifty-cli/src/lib.rs`, edit `add_filesystem_timestamp_fallbacks` (around line 640):
   - Keep the `Some(metadata)` early-return and the `matches!(format, Format::Png)` format gate.
   - Remove `|| !is_apple_screen_capture(path)` from the gate so all PNGs enter the body.
   - Reorder the `or_else` chain so `apple_content_creation_timestamp(path)` is only attempted on macOS (wrap that branch in `cfg!(target_os = "macos")` or extract it into a `#[cfg(target_os = "macos")]` helper that returns `None` on non-macOS targets) and only when `is_apple_screen_capture(path)` is true (same enrichment trigger as today, but now scoped to one branch instead of gating the whole function).
   - Preserve the existing precedence: AppleContentCreationDate (mac+screencap only) → FileModifyDate → FileCreateDate. Verifiable outcome: on Linux/CI the `FileModifyDate` branch fires for any PNG missing `DateTimeOriginal`/`CreateDate`; on macOS, screencaps still pick up the higher-precedence Apple value.
2. If `is_apple_screen_capture` and `apple_content_creation_timestamp` are no longer reachable on non-macOS builds, gate them behind `#[cfg(target_os = "macos")]` to silence dead-code warnings without affecting macOS behavior. Verifiable outcome: `cargo build` and `cargo clippy` clean on both linux and macos targets.
3. In `crates/xifty-cli/tests/cli_contract.rs`, add a new test (next to `apple_screenshot_png_uses_preserved_modified_time_when_birthtime_is_copy_time` at line 261) named `png_without_embedded_datetime_falls_back_to_filesystem_mtime`:
   - Copy `fixtures/minimal/no_exif.png` to `std::env::temp_dir().join(...)` with a unique suffix (reuse `chrono_like_test_suffix`).
   - Set a known mtime on the tmpfile using `filetime::set_file_mtime` if the crate is already a dev-dependency, otherwise via `std::fs::File::open(...).set_modified(SystemTime)` (Rust 1.75+, available with the workspace's edition 2024 toolchain). No shelling out to `touch` — the test must run portably on Linux CI.
   - Call `xifty_cli::extract_path(temp_path, ViewMode::Normalized)`, normalize the result via the existing `normalized_map` helper, and assert `captured_at.value` and `created_at.value` equal the expected ISO-8601 UTC string derived from the mtime.
   - Clean up the tmpfile with `let _ = fs::remove_file(temp_path);` like the existing test.
   - This test is intentionally NOT gated on `target_os = "macos"` — it must pass on Linux CI and is the regression sentinel for this bug. Verifiable outcome: `cargo test -p xifty-cli png_without_embedded_datetime_falls_back_to_filesystem_mtime` passes on linux-x64.
4. (Optional, if practical) Add an inline doc comment on `add_filesystem_timestamp_fallbacks` summarizing the two-tier behavior (universal mtime/ctime fallback; macOS Apple-screencap enrichment) so the next reader does not re-introduce the regression. Verifiable outcome: doc comment present.

## Tests
- New: `crates/xifty-cli/tests/cli_contract.rs::png_without_embedded_datetime_falls_back_to_filesystem_mtime` — portable Linux-runnable proof that the mtime fallback emits `captured_at` / `created_at`.
- Existing: `crates/xifty-cli/tests/cli_contract.rs::apple_screenshot_png_uses_preserved_modified_time_when_birthtime_is_copy_time` (line 262) — must continue to pass on macOS unchanged; verifies the Apple enrichment branch is intact.
- Existing snapshot suite (`extract_snapshot_*`) — must not regress; rerun with `cargo insta review` only if a deliberate diff is observed (none expected, since fixtures already have embedded datetimes or are not PNG).
- Harness/regression against `prebuilds/linux-x64/@xifty+xifty.node`: out of scope for this PR. The Rust integration test in step 3 covers the same code path that gets compiled into the prebuild; once merged, the next prebuild release will carry the fix and the reporter can re-run their Lambda harness. No N-API harness change needed in this issue.

## Validation
Per `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

## Risks
- **Test environment mtime resolution.** Some Linux filesystems (notably tmpfs with default `noatime`/coarse timestamps, or certain CI runners) round mtime to the second; the test must set a whole-second `SystemTime` to avoid sub-second drift in the formatted ISO string. Mitigation: choose a fixed Unix timestamp (e.g. `SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_000)`) and format the expected string from the same value rather than reading it back.
- **`set_modified` minimum Rust version.** `File::set_modified` requires Rust 1.75; the workspace is on edition 2024 (rustc ≥ 1.85), so this is safe. If the toolchain in CI is older than expected, fall back to the `filetime` crate (already commonly used in Rust workspaces) added under `[dev-dependencies]` of `xifty-cli`.
- **Confidence-score change.** The current `filesystem_timestamp_entry` notes drive confidence/precedence in the normalize layer; restructuring the precedence inside `add_filesystem_timestamp_fallbacks` must keep the same `(tag_id, tag_name, note)` triples in the same order for the Apple branch — otherwise normalized output for existing macOS screenshots could shift confidence. Mitigation: preserve the current `or_else` order verbatim, only changing whether the Apple branch is attempted.
- **Dead code on non-macOS.** Gating `is_apple_screen_capture` / `apple_content_creation_timestamp` with `#[cfg(target_os = "macos")]` removes them from Linux builds — confirm no other callsite references them (rg shows only the one callsite at line 649).
- **Out-of-scope temptation.** The reporter asks about thresholds for "mtime too close to now" — explicitly NOT addressed here. There is no such threshold today and adding one would change behavior on macOS too. If desired, file a follow-up.

