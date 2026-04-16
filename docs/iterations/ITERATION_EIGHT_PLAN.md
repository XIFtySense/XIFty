# XIFty Iteration Eight Plan

## Summary

Iteration eight should prepare the main XIFty repository to become public
without making that visibility change the forcing function for core cleanup.

The correct order is:

1. simplify and clarify the repository surface
2. make the CLI experience obvious and trustworthy
3. add clean GitHub Actions CI while the repo is still private
4. open the repository only after the project presents itself honestly

This iteration is not about adding major new metadata breadth. It is about
making the project legible, runnable, and trustworthy to outside developers who
arrive looking for a serious metadata solution.

## Why This Iteration

XIFty is now strong where architecture-first projects should be strong:

- clean layering
- stable JSON contract
- proven FFI seam
- multiple binding/package experiments

The new risk is not internal design failure. It is outward readability and
adoption friction.

If the main repo becomes public before cleanup, new visitors will encounter:

- a documentation surface optimized for ongoing internal iteration
- many planning docs but a less obvious landing path
- no repository-level CI story
- limited immediate guidance about the CLI as the primary entry point

That would undersell the project.

## Primary Goal

Make the main XIFty repo public-ready by delivering:

- a simple landing-page README
- a clear CLI-first product story
- a minimal but credible documentation structure
- clean GitHub Actions CI for the core repo
- explicit public-readiness criteria before the visibility flip

## Scope

### In scope

- repository surface cleanup
- README simplification and navigation
- documentation restructuring where needed
- a clearer CLI installation and usage story
- GitHub Actions workflows for validation and hygiene
- branch/PR expectations for a public-facing repo
- a public-readiness checklist

### Out of scope

- major new metadata namespace work
- package publication pipelines for every binding repo
- release automation for all downstream language packages
- write support
- a full docs website or inspector UI

## Product Framing For A Public Repo

The public repo should present XIFty in this order:

1. what XIFty is
2. why it is different from flat tag-dump tools
3. how to run the CLI quickly
4. what XIFty supports today
5. where to go for deeper architecture and contributor docs

The repo front page should not feel like an internal planning archive.

### README direction

The root README should become a concise landing page, not a dense project
ledger.

Recommended sections:

- one-paragraph thesis
- quickstart
- current capability summary
- why XIFty is different
- repository map
- links to deeper docs

What to avoid on the landing page:

- long iteration history
- exhaustive planning-doc lists
- internal process detail before the user knows why the project matters
- too many language-binding details above the CLI story

## Documentation Restructuring

The repo likely needs a cleaner docs shape before going public.

Recommended top-level documentation structure:

- `README.md` as the landing page
- `VISION.md` as product thesis
- `STATE_OF_THE_PROJECT.md` as honest current-state assessment
- `CONTRIBUTING.md` as contributor entry
- `ENGINEERING_PRINCIPLES.md` as architecture/craft expectations
- `FFI_CONTRACT.md` as embedding contract
- a `docs/` directory for archived iteration plans and deeper planning material

Recommended move:

- keep active/high-signal docs at the root
- move historical iteration plans/checklists under `docs/iterations/`
- move supporting research/planning docs under `docs/architecture/` or
  `docs/research/`

This keeps the repo welcoming without deleting useful history.

## CLI-First Public Readiness

Before opening the repo, the CLI should feel like the obvious first way to use
XIFty.

The README and docs should make these questions easy to answer:

- how do I build it?
- how do I run `probe`?
- how do I run `extract`?
- what do the four views mean?
- what formats/namespaces are supported today?
- where are example fixtures?

Recommended CLI readiness tasks:

- add a short quickstart using checked-in fixtures
- document the `probe` and `extract` commands clearly
- make the view modes explicit
- consider whether `cargo install --path crates/xifty-cli` should be documented
  as the default local install path
- ensure the CLI help output is clean and public-facing

If the repo must become public before CI/CD is complete, the minimum bar should
still be: a clear README and a credible CLI path.

## CI/CD Direction

The repository can and should adopt GitHub Actions before becoming public.

Based on current GitHub documentation:

- GitHub Actions is available for private repositories on current GitHub plans,
  with legacy-plan exceptions
- reusable workflows can be shared within private repositories and within an
  organization without publishing them publicly
- workflow `permissions` should be kept minimal for `GITHUB_TOKEN`
- dependency caching and concurrency controls should be used intentionally, not
  by default cargo cult

Sources:

- [GitHub Actions documentation](https://docs.github.com/en/actions)
- [Billing and usage](https://docs.github.com/actions/learn-github-actions/usage-limits-billing-and-administration)
- [Sharing actions/workflows with your organization](https://docs.github.com/en/actions/how-tos/reuse-automations/share-with-your-organization)
- [GITHUB_TOKEN permissions guidance](https://docs.github.com/actions/writing-workflows/choosing-what-your-workflow-does/controlling-permissions-for-github_token)
- [Concurrency](https://docs.github.com/en/actions/concepts/workflows-and-actions/concurrency)
- [Dependency caching](https://docs.github.com/en/actions/concepts/workflows-and-actions/dependency-caching)

### CI principles

- keep workflows boring and explicit
- default `permissions` to least privilege
- use pinned major-version official actions, and move to full SHA pinning if the
  org wants stricter supply-chain posture
- separate fast PR validation from slower optional verification
- do not make local-only fixture tests mandatory in CI
- avoid workflows that require the repo to be public

### Recommended initial workflows

#### 1. `ci.yml`

Runs on `push` and `pull_request`.

Initial jobs:

- format check
- workspace test
- FFI test
- Node incubator test
- Swift incubator test on macOS

Recommended matrix:

- Linux job for Rust/core verification
- macOS job for Swift verification

#### 2. `lint-docs.yml`

Optional lightweight workflow for:

- JSON validity checks
- checked-in generated-header consistency
- maybe README/doc link sanity later

#### 3. `nightly-fuzz-or-smoke.yml`

Not required immediately, but a good follow-on once the base CI is stable.

This should be scheduled or manually triggered, not required for every PR.

### Workflow hygiene

Recommended defaults:

- use `concurrency` to cancel superseded runs on the same branch/PR
- use dependency caching through setup actions where appropriate
- keep artifacts small and purposeful
- keep logs readable
- avoid writing back to the repository from normal CI jobs

## Public-Readiness Criteria

The repo should be considered ready to open when:

- the README works as a landing page for a first-time visitor
- the CLI quickstart is obvious and trustworthy
- repository structure is cleaner and less internally noisy
- CI runs automatically on PRs and `main`
- supported capabilities are clearly and honestly stated
- contributor expectations are explicit
- large/private fixtures remain excluded and documented clearly

## Suggested Phases

### Phase 1: Surface cleanup

- simplify the root README
- decide what stays at the root versus moves to `docs/`
- reduce visible planning clutter
- add a clearer repository map

### Phase 2: CLI hardening

- improve quickstart and command docs
- verify help output and examples
- confirm checked-in fixtures support the public story

### Phase 3: GitHub Actions foundation

- add `.github/workflows/ci.yml`
- add minimal permissions
- add caching/concurrency
- verify private-repo execution and branch protections

### Phase 4: Public opening checklist

- final pass on docs and repo hygiene
- decide whether to move or archive older iteration files first
- confirm CI green on default branch
- then flip visibility

## Success Criteria

Iteration eight is successful when:

- a new visitor can understand XIFty from the landing page quickly
- the CLI is the obvious first-use path
- the repository feels intentional rather than internally cluttered
- GitHub Actions validates the repo cleanly while it is still private
- opening the repo becomes a low-risk visibility change rather than a rushed
  cleanup event
