<!-- loswf:plan -->
# Plan #28: archive stale plan draft for closed issue #12

## Problem
Issue #28 asks to archive stale drafts for closed issues #12 and #13. Scouting the tree shows only issue #12's draft is still stale: `specs/drafts/12-wasm-demo-ci-smoke.md` (9423 bytes) still exists in `specs/drafts/` even though issue #12 was closed 2026-04-20 via PRs #22/#15. The corresponding archived copy `specs/archive/12-wasm-demo-ci-smoke.md` (9489 bytes) already exists and is identical except for a trailing archive marker (`<!-- archived by docs agent: issue #12 closed via PR #15 -->`). Issue #13's draft has already been archived — `specs/drafts/13-conflict-source-provenance.md` is absent and `specs/archive/13-conflict-source-provenance.md` is present. `specs/drafts/` currently contains only `.gitkeep` and the stale `12-*.md` file.

## Approach
Remove the duplicate stale draft `specs/drafts/12-wasm-demo-ci-smoke.md` (the archived copy is authoritative and already carries the archive footer). Do NOT add a new archive file — `specs/archive/12-wasm-demo-ci-smoke.md` is already in place from commit `02d64a0`. For issue #13, no action is needed; note this divergence from the issue body in the PR description. After removal, `specs/drafts/` should contain only `.gitkeep`, satisfying the acceptance criteria that drafts contains neither stale file and that both live under `specs/archive/`.

## Files to touch
- `specs/drafts/12-wasm-demo-ci-smoke.md` — delete (stale duplicate of archived plan for closed issue #12).

## New files
- none.

## Step-by-step
1. `git rm specs/drafts/12-wasm-demo-ci-smoke.md` — working tree no longer contains the stale draft; `specs/drafts/` contains only `.gitkeep`.
2. Verify `specs/archive/12-wasm-demo-ci-smoke.md` is untouched and still carries its archive marker footer — archived copy remains authoritative.
3. Verify `specs/drafts/13-conflict-source-provenance.md` does not exist and `specs/archive/13-conflict-source-provenance.md` does exist — confirms #13's acceptance criterion already satisfied pre-change.
4. Commit on a feature branch with message referencing #28; open PR noting that only #12 required action because #13 was already archived in a prior change.

## Tests
- No code changes; no unit tests. Verification is filesystem state:
  - `test -f specs/archive/12-wasm-demo-ci-smoke.md && ! test -f specs/drafts/12-wasm-demo-ci-smoke.md`
  - `test -f specs/archive/13-conflict-source-provenance.md && ! test -f specs/drafts/13-conflict-source-provenance.md`
  - `ls specs/drafts/` shows only `.gitkeep`.

## Validation
From `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check`
- `cargo test --workspace --all-features`
- `cargo test -p xifty-ffi --all-features`

These are no-ops for a docs-only change but MUST still pass per config.

## Risks
- The issue body lists two files to remove but only one is actually stale in the current tree. The plan intentionally narrows scope to match reality; reviewer should confirm via `ls specs/drafts/` and the diff in this plan's Problem section before merging, and the PR description should call out this divergence.
- The archived copy has a trailing archive marker that the drafts copy lacks; deleting the drafts copy is correct because the archive version is authoritative — we do NOT want to overwrite the archive with the un-footered draft.
- `specs/drafts/.gitkeep` must remain so the directory continues to exist in git.

