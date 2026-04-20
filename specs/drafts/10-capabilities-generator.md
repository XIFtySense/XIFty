<!-- loswf:plan -->
# Plan #10: Generate CAPABILITIES.json entries from test fixtures

## Problem
`CAPABILITIES.json` (repo root) is hand-edited. `STATE_OF_THE_PROJECT.md` lines 307-309 flag this as a known drift risk — a namespace can be wired and tested while CAPABILITIES.json still claims `not_yet_supported`, or vice versa. The `Hygiene` workflow (`.github/workflows/hygiene.yml` line 32) only checks the file parses as JSON; it does not verify the claims match observed behavior. The issue's acceptance criteria require a tool in `tools/` that validates CAPABILITIES.json against observed CLI output over `fixtures/minimal/`, wired into `hygiene.yml`, and documented in `tools/README.md`.

## Approach
Add a Python tool — `tools/validate_capabilities.py` — that mirrors the pattern established by `tools/validate_schema_examples.py` (same ROOT/FIXTURES constants, same `cargo run -q -p xifty-cli -- ...` invocation, same stdlib+jsonschema-only dependency stance). The tool walks every file under `fixtures/minimal/`, invokes `xifty-cli extract <path> --view raw`, and aggregates — per container (the `input.container` field) — the set of observed namespaces (distinct `raw.metadata[].namespace` values). It then compares the observed `{container -> {namespace}}` map against the claims in `CAPABILITIES.json#/containers/*/namespaces`. The tool fails when an observed `(container, namespace)` pair is marked `not_yet_supported` in CAPABILITIES.json (under-reporting drift). It supports two modes: `--check` (default; non-zero exit on drift) and `--write` (rewrite CAPABILITIES.json in place with observed supported pairs promoted, preserving hand-curated `bounded` distinctions and `supported_tags` lists). Hand edits remain authoritative for `not_yet_supported` — only under-reporting is a hard failure, since that is the specific drift the issue calls out. Wire the check into `hygiene.yml` as a new step in the existing `docs-and-contract` job, immediately after the current `Validate CAPABILITIES.json` JSON-parse step. No new heavyweight dependency — reuse existing Python 3 stdlib and the already-installed `jsonschema` isn't required here (this is a set-diff comparison). No new Rust crate, no xtask, no FFI touch, respecting the container/interpretation boundary (the tool only reads CLI JSON, it does not call into crates).

## Files to touch
- `.github/workflows/hygiene.yml` — add a new step in `docs-and-contract` that runs `python3 tools/validate_capabilities.py --check`; ensure the Rust toolchain + cargo cache steps already present above it cover the `cargo run` invocations.
- `tools/README.md` — add a section documenting `validate_capabilities.py` (purpose, `--check` vs `--write`, required Python version, how to extend when new containers or namespaces land).
- `CAPABILITIES.json` — only if the current tree under-reports; expected to be a no-op on first pass, but the plan must tolerate small corrections surfaced by the generator (explicitly not a license to edit claims by hand in the same PR — builder should open a follow-up if drift is detected).

## New files
- `tools/validate_capabilities.py` — generator/validator tool described in Approach.
- `tools/README.md` — create if absent (currently `tools/` has no README); otherwise append.

## Step-by-step
1. Create `tools/README.md` (or confirm existing) and stub an entry for the validator. Verifiable by: file exists and renders on GitHub.
2. Implement `tools/validate_capabilities.py` with three functions mirroring `validate_schema_examples.py` style: `run_cli(*args)` returning parsed JSON, `observed_map(fixtures_dir: Path) -> dict[str, set[str]]` that iterates every regular file in `fixtures/minimal/` (excluding `README.md` and dotfiles), runs `extract <path> --view raw`, and collects `input.container -> set(raw.metadata[].namespace)` — skipping fixtures whose CLI exits non-zero (malformed corpus files are acceptable to skip, as `malformed_*.jpg` currently exists), and `diff_against_declared(observed, capabilities_json) -> list[str]` returning human-readable drift lines. Verifiable by: `python3 tools/validate_capabilities.py --check` exits 0 against current `main` (or surfaces a real, fixable drift row).
3. Add `--write` branch: rewrite CAPABILITIES.json in place, promoting observed `(container, namespace)` pairs currently marked `not_yet_supported` to `supported`. Preserve `bounded` markers, `supported_tags`, `normalized_fields`, `surfaces`, `namespaces`, and key order via `json.dumps(..., indent=2)` matching the existing 2-space layout (trailing newline). Verifiable by: running `--write` then `--check` is idempotent and `git diff CAPABILITIES.json` is empty on clean main.
4. Add a unit-style self-test at the bottom of the script (guarded by `if __name__ == "__main__"`) that asserts the observed map for `happy.jpg` contains `exif` under `jpeg`. Verifiable by: `python3 tools/validate_capabilities.py --check` smoke passes.
5. Wire into `hygiene.yml`. Insert a step named `Validate CAPABILITIES.json against fixtures` after the existing `Validate CAPABILITIES.json` step (line 32). The step runs `python3 tools/validate_capabilities.py --check`. Since the job already installs the Rust toolchain and caches cargo (lines 22-29), `cargo run -p xifty-cli` works. Verifiable by: `act` or a PR CI run executes the new step and passes.
6. Document in `tools/README.md`: invocation, exit semantics, how to add new containers (extend `fixtures/minimal/` + regenerate via `--write`), and the explicit rule that hand-edits for `bounded`/`not_yet_supported` remain authoritative.

## Tests
- The tool itself is a Python script; a lightweight doctest or `__main__` smoke assertion (step 4) guards regressions.
- No new Rust tests required — the tool consumes stable CLI JSON already covered by `crates/xifty-cli` contract tests and `tools/validate_schema_examples.py`.
- Manual verification: run `python3 tools/validate_capabilities.py --check` locally on clean main; expect exit 0.
- Drift behavior: temporarily edit CAPABILITIES.json to mark `jpeg.exif` as `not_yet_supported`, confirm the tool exits non-zero with a clear message, revert.

## Validation
Commands from `.loswf/config.yaml` `validate[]`:
- `cargo fmt --all -- --check` — no Rust changes, trivially passes.
- `cargo test --workspace --all-features` — must still pass; no code changes expected.
- `cargo test -p xifty-ffi --all-features` — no FFI changes, must pass.

Plus the new tool's own smoke check:
- `python3 tools/validate_capabilities.py --check` — must exit 0 on the final diff.

## Risks
- Fixture coverage gap: `fixtures/minimal/` does not cover every declared container combination (e.g. `mov.quicktime` has `happy.mov` but `mp4.rtmd` may not emit `rtmd` namespace in raw output). The tool must treat "no observation" as neutral — only under-reporting (observed but claimed `not_yet_supported`) is a failure. Over-claims (declared but never observed) are a softer drift class; log as warnings, do not fail CI, to avoid blocking on missing fixtures.
- Malformed fixtures: files like `malformed_app1.jpg`, `malformed.mov` may cause the CLI to exit non-zero on `extract`. The tool must skip non-zero exits gracefully (capture stderr, continue) rather than crashing the CI step.
- Boundary concern: `detected_format` vs `container` — use `input.container` (the normalized container kind) for keying, matching CAPABILITIES.json's container keys (`jpeg`, `tiff`, `png`, `webp`, `heif`, `mp4`, `mov`). Confirm HEIC fixtures report `container: heif` (per CAPABILITIES.json line 34-41) — spot check with `cargo run -q -p xifty-cli -- extract fixtures/minimal/happy.heic --view raw`.
- `--write` determinism: Python dict insertion order + `json.dumps(..., indent=2, sort_keys=False)` plus careful in-place edits (don't rebuild top-level from scratch) to avoid reordering `namespaces`, `containers`, `normalized_fields`, `surfaces`. Golden test: `--write` on clean main produces empty `git diff`.
- CI runtime: each `cargo run` invocation on a cold cache is slow. The step sits inside the cached `docs-and-contract` job, so incremental runs are fast; first-time runs may add ~60-90s. Acceptable since hygiene is weekly-cron + on-demand, not per-PR.
- Plan scope: issue explicitly allows leaving `not_yet_supported` as a hand-curated concept. Do NOT auto-downgrade `supported` to `not_yet_supported` on absence of observation — that would invert the drift risk.

