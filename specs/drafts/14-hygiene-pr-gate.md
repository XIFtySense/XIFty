<!-- loswf:plan -->
# Plan #14: Gate PRs on hygiene (cbindgen header + schema validation)

## Problem
`.github/workflows/hygiene.yml` triggers only on `workflow_dispatch` and the `0 9 * * 1` weekly cron (lines 3-6). Its `docs-and-contract` job enforces two gates that belong on the merge path — the cbindgen header staleness check (`include/xifty.h` vs regenerated output, lines 47-50) and the JSON schema artifact validation (`schemas/xifty-*.schema.json` + `tools/validate_schema_examples.py`, lines 36-45). Because neither PRs nor pushes to `main` run this workflow, an FFI change that forgets to regenerate `include/xifty.h`, or a schema drift against the example outputs, can merge cleanly and go unnoticed for up to a week. The `oracle-differentials` job (lines 52-72) installs ExifTool and is correctly kept off the PR path (per `README.md:185-188` and `docs/iterations/ITERATION_EIGHT_CHECKLIST.md:75-78`).

## Approach
Add `pull_request` and `push: branches: [main]` triggers to `hygiene.yml`, and gate `oracle-differentials` with a job-level `if:` so only the cheap, deterministic `docs-and-contract` job runs on PR/push while ExifTool differentials stay schedule/dispatch-only. Tighten the concurrency group to include PR number, mirroring `ci.yml:19-21`. Update `specs/SRS.md` §4.7 table and `README.md` hygiene paragraph to document the new gating behavior, and add a branch-protection required-checks note to `docs/RELEASE_CHECKLIST.md`.

## Files to touch
- `.github/workflows/hygiene.yml` — add `pull_request` + `push: branches: [main]` triggers; gate `oracle-differentials` with `if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'`; tighten concurrency group.
- `README.md` — update lines 185-188 to reflect the new PR-gated scope of hygiene.
- `specs/SRS.md` — update §4.7 (lines 78-81) to list `Docs And Contract` as a required PR check alongside `ci.yml`.
- `docs/RELEASE_CHECKLIST.md` — add a bullet listing required status checks for branch protection on `main` (`Rust Core`, `Runtime Artifact`, `Lambda Node Example`, `Docs And Contract`).

## New files
- None. A dedicated `docs/BRANCH_PROTECTION.md` is unnecessary; the one-liner in `RELEASE_CHECKLIST.md` is sufficient.

## Step-by-step
1. Edit `.github/workflows/hygiene.yml` `on:` block (lines 3-6) to add `pull_request:` and `push: branches: [main]` while keeping `workflow_dispatch` and the Monday cron — verifiable by `actionlint` and by a new PR triggering the workflow.
2. Update `concurrency.group` (line 12) to `hygiene-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}` so PR re-pushes cancel their own in-flight run, matching `ci.yml:20`.
3. Add `if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'` to the `oracle-differentials` job (line 52) — outcome: PR runs show only `Docs And Contract`, weekly cron shows both jobs. Verified via `gh run view <id> --json jobs`.
4. Update `README.md` lines 185-188 to state that PR gates now include the cbindgen header check and schema validation, while ExifTool-backed oracle differentials remain weekly.
5. Update `specs/SRS.md` §4.7 (after line 81) to add `Docs And Contract` to the list of PR-blocking workflows.
6. Add a "Branch protection" bullet to `docs/RELEASE_CHECKLIST.md`: required status checks on `main` are `Rust Core`, `Runtime Artifact`, `Lambda Node Example`, `Docs And Contract`.
7. In the PR body, call out the operator follow-up: configure GitHub branch protection to require `Docs And Contract` once the workflow has run at least once on `main`.

## Tests
- No new unit tests; this is a CI-config change.
- Manual CI self-test: open a draft PR, confirm `Docs And Contract` runs and passes, and confirm `Oracle Differentials` is skipped (check `gh run view` on the hygiene run for the PR).
- Regression check: temporarily dirty `include/xifty.h` on a throwaway branch and confirm the hygiene job fails pre-merge. The existing test at `crates/xifty-ffi/tests/c_abi.rs:27-59` is the enforcement mechanism and requires no change.
- `actionlint .github/workflows/hygiene.yml` locally.

## Validation
From `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

Plus workflow-specific:
- `actionlint .github/workflows/hygiene.yml` (if available) — catches bad `on:` keys or `if:` expressions.
- Post-merge: `gh run list --workflow=hygiene.yml --event=pull_request` on a subsequent PR to confirm the trigger fires.

## Risks
- Adding `pull_request` doubles the `cargo install cbindgen --locked` cost on every PR (~60-90s cold, much less with `Swatinem/rust-cache@v2` at line 28). If this proves too slow, a follow-up can cache the `cbindgen` binary via `actions/cache` keyed on the Rust toolchain version.
- `jsonschema` pip install (line 42) uses `--user` on ubuntu-latest; sensitive to upstream yanks. Consider pinning a known-good version in a follow-up (out of scope).
- `cargo install cbindgen --locked` is not version-pinned, so a cbindgen point release could cause transient PR failures. Recording a known-good version in `FFI_CONTRACT.md` is a sensible follow-up but not required here.
- The `if:` expression on `oracle-differentials` must exclude `push` to `main` as well. `schedule || workflow_dispatch` correctly handles this — verify no `push.branches:[main]` leak into the job condition.
- PRs from forks: hygiene runs with `permissions: contents: read` (line 8-9), which is sufficient — no secrets are referenced.
- Branch protection configuration itself is a GitHub operator step; code cannot enforce it. The PR must explicitly surface this action item.
