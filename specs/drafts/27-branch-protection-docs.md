<!-- loswf:plan -->
# Plan #27: Configure branch protection required status checks on main

## Problem
`gh api repos/XIFtySense/XIFty/branches/main/protection` currently returns an empty `required_status_checks` object, so PRs to `main` can merge without `Rust Core`, `Runtime Artifact`, `Lambda Node Example`, or `Docs And Contract` passing. PR #17 shipped the `Docs And Contract` job and flagged this operator follow-up, but the GitHub settings change was never actioned. The four job names already exist in CI (`.github/workflows/ci.yml` lines 25, 50, 74 and `.github/workflows/hygiene.yml` line 21), and are documented as required in `docs/RELEASE_CHECKLIST.md` lines 63–72.

## Approach
The primary action is an operator-level GitHub repository settings change (Settings → Branches → main → Require status checks to pass before merging), because branch protection cannot be committed as a code artifact in this repo (no Ruleset-as-code file exists under `.github/`). The repo-side deliverable is a docs update in `docs/RELEASE_CHECKLIST.md` confirming the configured state and recording the verification commands, plus a brief note on how an operator should validate via `gh api`. This mirrors the existing pattern in `docs/RELEASE_CHECKLIST.md` where operator actions (npm publish verification at lines 58–61) are documented alongside the commands that prove they were performed.

## Files to touch
- `docs/RELEASE_CHECKLIST.md` — update the "Branch Protection" section (lines 63–75) to reflect the configured state and add a verification command block.

## New files
- None. Branch protection is configured in GitHub UI/API, not in repo files.

## Step-by-step
1. Operator action (out-of-repo): In GitHub Settings → Branches → `main` → "Require status checks to pass before merging", enable the rule and add the four contexts exactly as they appear in CI: `Rust Core`, `Runtime Artifact`, `Lambda Node Example`, `Docs And Contract`. Enable "Require branches to be up to date before merging". Verifiable outcome: `gh api repos/XIFtySense/XIFty/branches/main/protection --jq '.required_status_checks.contexts'` returns a JSON array containing all four strings.
2. In `docs/RELEASE_CHECKLIST.md` at the "Branch Protection" section (starting line 63), rewrite the lead-in from "Required status checks on `main` (configure in GitHub repository settings …)" to state that protection is configured, and keep the bulleted list of the four required contexts with their source workflow files. Verifiable outcome: the file no longer instructs the reader to configure; it instead lists the enforced state.
3. In the same section, append a fenced `bash` block with `gh api repos/XIFtySense/XIFty/branches/main/protection --jq '.required_status_checks.contexts'` plus the expected output, so future operators can verify the rule has not drifted. Verifiable outcome: the new block renders and the command prints the four contexts.
4. Preserve the existing note (lines 74–75) that `Oracle Differentials` is intentionally not required (schedule/dispatch only, per `hygiene.yml` line 62 `if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'`). Verifiable outcome: note remains unchanged.
5. Open a draft PR from a scratch branch touching an unrelated trivial file, confirm GitHub shows all four checks as "Required" and blocks merge until green, then close the draft PR without merging. Verifiable outcome: screenshot/log evidence attached to issue #27 showing the four required checks on the PR merge box.

## Tests
- No Rust/JS test changes. Validation is via `gh api` assertion (step 1 outcome) and the draft-PR merge-gate check (step 5).
- Markdown lint/render: none configured in repo; visual check sufficient.

## Validation
- `cargo fmt --all -- --check` (from `.loswf/config.yaml` `validate[]`)
- `cargo test --workspace --all-features` (from `.loswf/config.yaml` `validate[]`)
- `cargo test -p xifty-ffi --all-features` (from `.loswf/config.yaml` `validate[]`)
- Post-merge operator verification: `gh api repos/XIFtySense/XIFty/branches/main/protection --jq '.required_status_checks.contexts'` returns `["Rust Core","Runtime Artifact","Lambda Node Example","Docs And Contract"]` (order may vary).

## Risks
- Context-name drift: GitHub matches required checks by the `name:` string, not the job key. If any job is renamed in `ci.yml` or `hygiene.yml`, the protection rule silently stops gating. Mitigation: the docs update pairs each context with its source workflow file so renames are caught in review.
- `Runtime Artifact` context collision: `runtime-artifacts.yml` line 23 also produces jobs named `Runtime Artifact (<matrix>)` on release. The required check name `Runtime Artifact` (from `ci.yml` line 50) is distinct — verify in the draft PR that GitHub resolves the required check to the CI job, not a release job, since release jobs don't run on PRs.
- Operator-only change: this plan is not code-executable by the builder agent without repo admin credentials. The builder can land the docs update; the GitHub Settings change must be performed by a human admin. Flag this clearly to the reviewer so the issue is not closed on docs-only merge if protection is still empty.
- `branch-protection` API deprecation: the modern equivalent is Rulesets. If XIFtySense/XIFty uses a Ruleset instead of classic branch protection, the `gh api …/branches/main/protection` probe will return empty even when rules are active. Mitigation: also check `gh api repos/XIFtySense/XIFty/rulesets` during verification.

