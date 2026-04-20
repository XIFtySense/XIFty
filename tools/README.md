# tools/

Developer-facing scripts that live outside the cargo workspace. Each tool is
self-contained and invoked directly (Python 3.10+ stdlib, plus `jsonschema`
where noted). Runtime artifacts the tools consume — CLI JSON output, schemas,
fixtures — are produced by `crates/xifty-cli` and `schemas/`, so tools here
never call into the crates directly.

## generate_capabilities.py

Generates and validates entries in the repo-root `CAPABILITIES.json` against
observed CLI output over `fixtures/minimal/`.

The tool walks every regular file under `fixtures/minimal/`, invokes
`xifty-cli extract <path> --view raw`, and aggregates — keyed on
`input.detected_format` — the set of `raw.metadata[].namespace` values it
sees. It then compares the observed map against the claims in
`CAPABILITIES.json#/containers/*/namespaces`.

### Invocation

```sh
# Default: fail on under-reporting drift.
python3 tools/generate_capabilities.py --check

# Rewrite CAPABILITIES.json in place, promoting observed pairs.
python3 tools/generate_capabilities.py --write
```

Exit semantics (`--check`):

- `0` — observed map matches declared support.
- `1` — an observed `(detected_format, namespace)` pair is currently marked
  `not_yet_supported` or is entirely undeclared. Fix by running `--write` and
  committing the resulting `CAPABILITIES.json` diff.

Over-claims (declared `supported`/`bounded` but no fixture emits the
namespace) are logged as warnings on stderr and do **not** fail the check;
they indicate a fixture coverage gap, not a correctness bug.

### Hand-curated fields remain authoritative

`--write` only promotes `not_yet_supported` to `supported` and adds
newly-observed namespaces. It never:

- Downgrades `supported` to `not_yet_supported` on absence of observation —
  that would invert the drift risk the issue is guarding against.
- Touches `bounded` declarations, `supported_tags`, `normalized_fields`,
  `surfaces`, or the top-level `namespaces` map.

If you need to mark a namespace as `bounded` or `not_yet_supported`, edit
`CAPABILITIES.json` by hand; the generator leaves those edits alone on
subsequent `--write` runs.

### Extending coverage

New containers or namespaces land by:

1. Adding a representative fixture under `fixtures/minimal/` (see
   `tools/generate_fixtures.py`).
2. Running `python3 tools/generate_capabilities.py --write`.
3. Committing the updated `CAPABILITIES.json` alongside the fixture.

The `Hygiene` workflow (`.github/workflows/hygiene.yml`) runs `--check` on
every scheduled run, so drift surfaces as a CI failure within a week.

## validate_schema_examples.py

Validates CLI probe and extract output against the checked-in JSON schemas in
`schemas/`. Requires the `jsonschema` Python package.

## generate_fixtures.py

Regenerates the deterministic files under `fixtures/minimal/`. Run when
container parsing or fixture shape changes.

## build-runtime-artifact.py / validate-runtime-artifact.py

Build and validate the `xifty_runtime_targeted_bundle` artifact produced by
release jobs.

## build-web-demo.sh / build-node-lambda-layer.sh

Shell helpers for the browser demo and the AWS Lambda layer example.
