<!-- loswf:plan -->
# Plan #44: Runtime artifact CI covers macos-arm64 alongside linux-x64

## Problem
The `runtime-artifact` job in `.github/workflows/ci.yml` (lines 49-71) runs only on `ubuntu-latest`, so only the `linux-x64` canonical bundle is built and validated on every PR. `CAPABILITIES.json:141-148` and `STATE_OF_THE_PROJECT.md:295` list both `macos-arm64` and `linux-x64` as canonical runtime targets, and `specs/SRS.md §4` marks `Runtime Artifact` as a PR-blocking check. The `runtime-artifacts.yml` release workflow already builds both via a `strategy.matrix` (lines 25-32), but only on `release: published` / `workflow_dispatch` — so the macOS path has no PR-time regression coverage. A cbindgen header drift, dylib linker issue, or macOS-specific validator failure goes undetected until a release is cut.

## Approach
Mirror the proven release-workflow matrix (`runtime-artifacts.yml:25-32`) onto the PR-time `runtime-artifact` job in `ci.yml`. Convert the single ubuntu job into a `strategy.matrix` fan-out with `{target: macos-arm64, runs_on: macos-14}` and `{target: linux-x64, runs_on: ubuntu-latest}`. Use `matrix.target` in the job name so the check surfaces per-platform, and in the artifact filename so the two legs don't collide. `tools/build-runtime-artifact.py` already self-detects the host triple (line 41 emits `macos-arm64` on darwin/arm64; line 43 emits `linux-x64` on linux/x86_64), so no script changes are needed. `fail-fast: false` keeps one platform's failure from masking the other.

## Files to touch
- `.github/workflows/ci.yml` — convert the `runtime-artifact` job (lines 49-71) to a matrix over `macos-arm64` / `linux-x64`, parameterize `runs-on`, the job name, and the artifact output path.

## New files
- None.

## Step-by-step
1. In `.github/workflows/ci.yml`, replace `runs-on: ubuntu-latest` on the `runtime-artifact` job with `runs-on: ${{ matrix.runs_on }}` and add a `strategy` block with `fail-fast: false` and a `matrix.include` list containing `{target: macos-arm64, runs_on: macos-14}` and `{target: linux-x64, runs_on: ubuntu-latest}` — mirrors `runtime-artifacts.yml:25-32` exactly. Verifiable: `yq '.jobs.runtime-artifact.strategy.matrix.include' .github/workflows/ci.yml` prints both targets.
2. Update the job `name` to `Runtime Artifact (${{ matrix.target }})` so each matrix leg appears as its own status check. Verifiable: PR run shows two `Runtime Artifact (...)` check entries.
3. Parameterize the build output path: replace `/tmp/xifty-runtime-linux-x64.tar.gz` with `/tmp/xifty-runtime-${{ matrix.target }}.tar.gz` in both the build step and the validate step. Verifiable: `grep xifty-runtime ci.yml` shows only `${{ matrix.target }}` references; no hard-coded `linux-x64`.
4. Do NOT modify `tools/build-runtime-artifact.py` or `tools/validate-runtime-artifact.py` — the host-triple auto-detection on lines 41 and 43 of `build-runtime-artifact.py` already produces the correct target per runner. Verifiable: those files are untouched in the diff.
5. Flag branch-protection follow-up in the PR body: the required status check named `Runtime Artifact` listed in `specs/SRS.md §4` becomes two checks (`Runtime Artifact (macos-arm64)` and `Runtime Artifact (linux-x64)`). An admin must update branch protection on `main` to require both; otherwise the gate silently becomes non-blocking. This is a settings action, not a code change, but the plan flags it so the reviewer surfaces it explicitly.

## Tests
- No new Rust tests — this is CI infra. Primary verification is the CI run on the PR itself: both matrix legs must build and validate the artifact successfully. The `test-workspace` and `test-ffi` gates from `.loswf/config.yaml` still run on `ubuntu-latest` under the `rust` job and must stay green.
- Optional local smoke: on an arm64 macOS host, `python3 tools/build-runtime-artifact.py --output /tmp/xifty-runtime-macos-arm64.tar.gz && python3 tools/validate-runtime-artifact.py --artifact /tmp/xifty-runtime-macos-arm64.tar.gz` should exit 0. Not required; CI is authoritative.

## Validation
- `cargo fmt --all -- --check` — unaffected (no Rust changes); must still pass.
- `cargo test --workspace --all-features` — unaffected; must still pass.
- `cargo test -p xifty-ffi --all-features` — unaffected; must still pass.
- CI gate: both `Runtime Artifact (macos-arm64)` and `Runtime Artifact (linux-x64)` jobs must complete successfully on the PR. `hygiene.yml` `Docs And Contract` stays green to confirm no cbindgen header drift on either platform.

## Risks
- macOS runners are slower and more costly than ubuntu; PR latency for `runtime-artifact` rises to the slower leg's wall time. Acceptable given it is the only CI proof of the macos-arm64 contract claimed in `CAPABILITIES.json`.
- `Swatinem/rust-cache@v2` caches are keyed per-OS, so first runs on macOS will be slow; steady state is fine.
- `macos-14` is GitHub's arm64 image (matching `runtime-artifacts.yml:30`); if GitHub retires or renames it, both workflows must update in lockstep. Keeping the runner label identical across both workflows is the mitigation.
- Branch protection on `main` currently names `Runtime Artifact`; after this change the check becomes platform-suffixed. Admin must add both new names to required checks, otherwise gating silently breaks. Surfaced explicitly in step 5 so the reviewer calls it out.
- If a future contributor adds a third canonical target (e.g. `linux-arm64`) to `CAPABILITIES.json.core.runtime_artifacts.targets` without updating the CI matrix, coverage drifts again. A follow-up could assert matrix parity against `CAPABILITIES.json`; out of scope here.
