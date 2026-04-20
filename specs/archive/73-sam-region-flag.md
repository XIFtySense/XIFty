# Plan #73: Lambda CI `sam validate` / `sam build` need explicit --region flag

## Problem
The `lambda-node-example` CI job's `Validate SAM template` step previously failed with "AWS Region was not found" because `sam validate` does not honour `AWS_REGION` / `AWS_DEFAULT_REGION` environment variables — it requires an explicit `--region` flag or a configured AWS profile. A top-level `env:` block in `.github/workflows/ci.yml` (added in commit `b3f82e8`) currently masks the failure on CI, but `examples/aws-sam-node/package.json:11` (`sam validate --template-file template.yaml`) and `.github/workflows/ci.yml:130` (`sam build --template-file template.yaml --build-dir .aws-sam/build`) still omit `--region`. Any developer running `npm run validate` locally without `AWS_REGION` exported hits the same failure, and the CI workaround is fragile.

## Approach
Add an explicit `--region us-east-1` flag to both call sites so the commands work regardless of ambient shell state. The region `us-east-1` matches the values already hardcoded in the workflow's top-level `env:` block at `.github/workflows/ci.yml:14-15`, so we are not introducing a new configuration value — just making the existing default explicit at each invocation. This mirrors the general "no ambient state" convention other steps in this workflow follow (e.g., explicit `--template-file` and `--build-dir` on `sam build`).

## Files to touch
- `/Users/k/Projects/XIFty/examples/aws-sam-node/package.json` — line 11: add `--region us-east-1` to the `validate` script.
- `/Users/k/Projects/XIFty/.github/workflows/ci.yml` — line 130: add `--region us-east-1` to the `sam build` invocation.

## New files
- None.

## Step-by-step
1. Edit `examples/aws-sam-node/package.json` line 11 — change `"validate": "sam validate --template-file template.yaml"` to `"validate": "sam validate --template-file template.yaml --region us-east-1"`. Verifiable outcome: `grep -n "sam validate" examples/aws-sam-node/package.json` shows the `--region us-east-1` flag.
2. Edit `.github/workflows/ci.yml` line 130 — change `run: sam build --template-file template.yaml --build-dir .aws-sam/build` to `run: sam build --template-file template.yaml --build-dir .aws-sam/build --region us-east-1`. Verifiable outcome: `grep -n "sam build" .github/workflows/ci.yml` shows the `--region us-east-1` flag.
3. Run `cd examples/aws-sam-node && unset AWS_REGION AWS_DEFAULT_REGION && npm run validate` locally to confirm it succeeds without ambient region env vars (if AWS SAM CLI is installed). Verifiable outcome: command exits 0 with `is a valid SAM Template.` message.
4. Push branch, open PR, observe `Lambda Node Example` CI job passes end-to-end (validate + build steps green).

## Tests
- No Rust test file changes required — this is a CI/scripts fix outside the cargo workspace.
- Manual/CI verification: a clean `sam validate` run without `AWS_REGION` in the environment must succeed (step 3 above).
- CI-level regression guard: the existing `Validate SAM template` and `Build AWS SAM application` workflow steps at `.github/workflows/ci.yml:109-111` and `:128-130` serve as the regression test — they must continue to pass after the change.

## Validation
- `cargo fmt --all -- --check` (from `.loswf/config.yaml` validate[0]) — unaffected but must pass.
- `cargo test --workspace --all-features` (validate[1]) — unaffected but must pass.
- `cargo test -p xifty-ffi --all-features` (validate[2]) — unaffected but must pass.
- CI `Lambda Node Example` job must pass on the PR (the actual behaviour under test).

## Risks
- Hardcoding `us-east-1` at two more call sites increases duplication of the region literal (already hardcoded at `.github/workflows/ci.yml:14-15`). Acceptable: `sam validate` is a schema check that does not actually call into a region-specific API, so the literal value is inconsequential for correctness — it only needs to be a valid AWS region string. A future refactor could centralize this via a workflow-level variable or an npm config, but that is out of scope for this bug fix.
- If a developer's local AWS profile defaults to a different region, the explicit `--region us-east-1` will override it. This is desirable for reproducible validation; no risk to real AWS resources because `sam validate` and `sam build` do not deploy anything.
- Acceptance criterion "does not hardcode a region value in a way that would break local developer use" is satisfied: `sam validate` and `sam build` are offline operations, so the hardcoded region does not affect deploy targets (which use `sam deploy`, not touched here).
