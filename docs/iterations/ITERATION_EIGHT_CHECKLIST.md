# XIFty Iteration Eight Checklist

This checklist turns the public-readiness and CI/CD plan into executable work.

## Goal

- [x] Prepare the main XIFty repo to become public without rushing the cleanup
- [x] Make the CLI the obvious first-use path
- [x] Add clean GitHub Actions CI while the repo is still private

## Repository Surface

- [x] Simplify the root README into a landing page
- [x] Add a short repository map
- [x] Reduce root-level planning clutter by moving lower-signal docs into `docs/`
- [x] Keep high-signal documents easy to find
- [x] Preserve project history without letting it dominate the first impression
- [x] Remove embedded binding package implementations from the main repo

## CLI Readiness

- [x] Document `probe` clearly
- [x] Document `extract` clearly
- [x] Document the four views clearly
- [x] Add a minimal quickstart using checked-in fixtures
- [x] Decide and document the default local install/build path for the CLI
- [x] Ensure CLI help output is clean and public-facing

## Capability Honesty

- [x] Keep `CAPABILITIES.json` aligned with the current repo reality
- [x] Make current support boundaries easy to understand from the README
- [x] Document the local-only fixture policy clearly
- [x] Avoid implying broader support than the tests and fixtures prove

## GitHub Actions

- [x] Add `.github/workflows/ci.yml`
- [x] Run Rust/core validation in CI
- [x] Run FFI validation in CI
- [x] Use least-privilege `permissions`
- [x] Add `concurrency` to cancel superseded runs
- [x] Use dependency caching intentionally
- [x] Keep local-only fixture tests out of required CI
- [x] Keep ExifTool-backed oracle checks out of the default core CI path

## Workflow Hygiene

- [x] Keep workflows readable and boring
- [x] Avoid write-back automation in normal CI jobs
- [x] Keep artifacts/logs small and useful
- [x] Decide whether any slower workflows should be optional or scheduled only

## Public Opening Readiness

- [x] Confirm README works for first-time visitors
- [x] Confirm contributor entry docs are clear
- [ ] Confirm CI is green on `main`
- [x] Confirm branch/PR expectations are documented
- [x] Confirm no sensitive or oversized local fixtures are tracked
- [ ] Confirm the repo can be opened without immediate follow-up cleanup

## Done Criteria

- [x] The root repo presents XIFty clearly to outsiders
- [ ] The CLI is the obvious first product surface
- [ ] GitHub Actions validates the project cleanly before the visibility flip
- [ ] Making the repo public is a deliberate release step, not a cleanup step

## Closeout Notes

- The main repo now treats language packages as external, canonical repos.
- `xifty-ffi` remains the single embedding seam inside the core repo.
- The in-repo example surface is intentionally minimal and centered on the ABI.
- Oracle-backed ExifTool differentials now belong to optional hygiene
  verification, not the default core CI workflow.
