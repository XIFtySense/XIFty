# Plan #44: Runtime artifact CI covers macos-arm64 alongside linux-x64

## Problem
The `runtime-artifact` job in `.github/workflows/ci.yml` (lines 49-71) runs only on `ubuntu-latest`, so only the `linux-x64` canonical bundle is built and validated on every PR. `CAPABILITIES.json:141-148` and `STATE_OF_THE_PROJECT.md:295` list both `macos-arm64` and `linux-x64` as canonical runtime targets, and `SRS.md §4` marks `Runtime Artifact` as a PR-blocking check. The `runtime-artifacts.yml` release workflow already builds both via a `strategy.matrix` (lines 25-32), but only on `release: published` / `workflow_dispatch` — so the macOS path has no PR-time regression coverage. A cbindgen header drift, dylib linker issue, or macOS-specific validator failure goes undetected until a release is cut.

## Approach
Mirror the proven release-workflow matrix (`runtime-artifacts.yml:25-32`) onto the PR-time `runtime-artifact` job in `ci.yml`. Convert the single ubuntu job into a `strategy.matrix` fan-out with `{target: macos-arm64, runs_on: macos-14}` and `{target: linux-x64, runs_on: ubuntu-latest}`. Use `matrix.target` in the job name so the check surfaces per-platform, and in the artifact filename so the two steps don't collide. `tools/build-runtime-artifact.py` already self-detects the host triple (line 41: `macos-arm64` on darwin/arm64; line 43: `linux-x64` on linux/x86_64), so no script changes are needed. `fail-fast: false` keeps one platform's failure from masking the other.

## Files to touch
- `.github/workflows/ci.yml` — convert `runtime-artifact` job (lines 49-71) to a matrix over `macos-arm64` / `linux-x64`, parameterize `runs-on`, job name, and artifact output path.

## New files
- None.

## Step-by-step
1. In `.github/workflows/ci.yml`, replace the `runs-on: ubuntu-latest` on the `runtime-artifact` job with `runs-on: ${{ matrix.runs_on }}` and add a `strategy` block with `fail-fast: false` and a `matrix.include` list containing `{target: macos-arm64, runs_on: macos-14}` and `{target: linux-x64, runs_on: ubuntu-latest}` — mirrors `runtime-artifacts.yml:25-32` exactly. Verifiable: `yq '.jobs.runtime-artifact.strategy.matrix.include' .github/workflows/ci.yml` prints both targets.
2. Update the job `name` to `Runtime Artifact (${{ matrix.target }})` so each matrix leg appears as its own status check. Verifiable: simulated PR run shows two `Runtime Artifact (...)` check entries.
3. Parameterize the build output path: replace `/tmp/xifty-runtime-linux-x64.tar.gz` with `/tmp/xifty-runtime-${{ matrix.target }}.tar.gz` in both the build step and the validate step. Verifiable: `grep xifty-runtime ci.yml` shows only `${{ matrix.target }}` references; no hard-coded `linux-x64`.
4. Do NOT modify `tools/build-runtime-artifact.py` or `tools/validate-runtime-artifact.py` — the host-triple auto-detection on lines 41 and 43 of `build-runtime-artifact.py` already produces the correct target per runner. Verifiable: scripts are untouched in the diff.
5. Note for reviewers/ops: the PR-blocking check listed in `specs/SRS.md §4` as `Runtime Artifact` will become two checks (`Runtime Artifact (macos-arm64)` and `Runtime Artifact (linux-x64)`). Branch-protection required-status-checks on `main` may need to be updated to require both; this is an admin/settings action, not a code change, but the plan flags it so the reviewer calls it out in the PR body.

## Tests
- No new Rust tests (this is CI infra). Primary verification is the CI run on the PR itself: both matrix legs must build and validate the artifact successfully. The `test-workspace` gate from `.loswf/config.yaml` still runs on `ubuntu-latest` under the `rust` job and must stay green.
- Optional local smoke: on an arm64 macOS host, `python3 tools/build-runtime-artifact.py --output /tmp/xifty-runtime-macos-arm64.tar.gz && python3 tools/validate-runtime-artifact.py --artifact /tmp/xifty-runtime-macos-arm64.tar.gz` should exit 0. Not required; CI is authoritative.

## Validation
- `cargo fmt --all -- --check` — unaffected (no Rust changes), but must still pass.
- `cargo test --workspace --all-features` — unaffected; must still pass on both platforms.
- `cargo test -p xifty-ffi --all-features` — unaffected; must still pass.
- CI gate: both `Runtime Artifact (macos-arm64)` and `Runtime Artifact (linux-x64)` jobs must complete successfully on the PR. `hygiene.yml` `Docs And Contract` also stays green to confirm no cbindgen drift.

## Risks
- macOS runners are ~10x slower and more expensive than ubuntu; PR latency on `runtime-artifact` will rise to the slower leg's wall time. Acceptable given it's the only CI proof of the macos-arm64 contract.
- Running two legs drops effective cache hit rate for `Swatinem/rust-cache@v2` per platform on first runs; steady state is fine since each OS caches independently.
- `macos-14` is GitHub's arm64 image (matching `runtime-artifacts.yml:30`); if GitHub retires or renames it, both workflows must update in lockstep. Keeping the runner label identical across the two workflows is the mitigation.
- Branch protection required-status-checks on `main` currently name `Runtime Artifact`; after this change the check name becomes platform-suffixed. Admin must add both new names to required checks, otherwise the gate silently becomes non-blocking. Plan surfaces this so the reviewer notes it in the PR body.
- No change is needed to `tools/build-runtime-artifact.py`, but if a future contributor adds a third target there without updating the CI matrix, coverage drifts again. A follow-up could assert matrix parity against `CAPABILITIES.json.core.runtime_artifacts.targets`; out of scope here.
