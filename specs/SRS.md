# XIFty — Software Requirements Specification (SRS)

## 1. Purpose

This document is the factory-facing requirements surface for XIFty. It
complements the product vision in `specs/VISION.md` and the architectural
documents at the repo root (`ENGINEERING_PRINCIPLES.md`,
`FFI_CONTRACT.md`, `STATE_OF_THE_PROJECT.md`, `CAPABILITIES.json`).

XIFty is a modern metadata engine for media files, built as a Rust cargo
workspace with a C ABI embedding surface (`xifty-ffi`) and thin bindings
for Node, Python, and Swift. The factory uses this SRS to ground intake,
planning, building, and review of work that spans the workspace — core
engine, container parsers, metadata namespace parsers, normalization,
policy, validation, JSON output, FFI, CLI, and WASM.

This is a seed SRS. Detailed requirements for individual subsystems
(containers, metadata namespaces, normalization, policy, FFI surface,
CLI, WASM, bindings, differential tooling) are expected to accrue here
over time as specs are authored and matured under `specs/` and
`specs/drafts/`.

## 2. Scope

In-scope for the factory:

- Rust workspace crates under `crates/` (core, source, detect, container
  parsers, metadata parsers, normalize, policy, validate, json, ffi,
  cli, wasm).
- C headers generated via cbindgen and governed by `FFI_CONTRACT.md`.
- JSON schemas in `schemas/` and generated artifacts.
- Tools and demo surfaces: `tools/`, `demo/`, `examples/`.
- Python corpus/differential tooling and TypeScript SDK/inspector
  scaffolding that lives in this repo.

Out-of-scope for v1:

- File mutation / write support.
- Feature parity with ExifTool as a target (coverage is
  capability-driven, not tag-count driven).
- Thick logic inside language bindings.

## 3. Architectural invariants

These invariants are guardrails for every plan and review. They are
intentionally redundant with `specs/VISION.md` and the top-level
`ENGINEERING_PRINCIPLES.md` so the factory sees them at planning time.

1. **Container parsing and metadata interpretation live in separate
   crates** and must not cross-import each other's internal types.
2. **Provenance is preserved end-to-end** — raw source values,
   namespaces, and (where possible) byte ranges survive into the
   `report` view.
3. **Normalization is additive, never destructive** — the `normalized`
   view is a projection; the `raw` and `interpreted` views remain
   available.
4. **FFI stability is a contract** — any change to the C ABI requires a
   documented update to `FFI_CONTRACT.md` and a rationale in the plan.
5. **Malformed input surfaces typed issues**, not panics or silent data
   loss. `Issue` and `Conflict` are first-class values.
6. **Snapshot tests (insta) are authoritative** — drift is reviewed and
   accepted deliberately, never blanket-accepted.

## 4. Validation configuration

The factory's builder and reviewer agents MUST run every command in
`validate[]` from `.loswf/config.yaml` before declaring a task done.
Commands run in the order listed (fail fast — cheapest first).

Current validation gates:

| # | Name            | Command                                      | Purpose                                                              |
|---|-----------------|----------------------------------------------|----------------------------------------------------------------------|
| 1 | `fmt`           | `cargo fmt --all -- --check`                 | Formatting gate across the workspace. Fast first line of defense.    |
| 2 | `test-workspace`| `cargo test --workspace --all-features`      | Unit + integration + insta snapshot tests across every workspace crate. |
| 3 | `test-ffi`      | `cargo test -p xifty-ffi --all-features`     | Targeted FFI surface tests — cheap way to catch C ABI regressions.   |

CI gating is enabled (`ci_check: true` in `.loswf/config.yaml`). The
plan-reviewer and reviewer block on failing GitHub Actions runs from
`.github/workflows/` (currently `ci.yml`, `hygiene.yml`,
`pages-demo.yml`, `runtime-artifacts.yml`).

Snapshot-test drift policy: if `cargo test` fails because of insta
snapshot diffs, the expected resolution is `cargo insta review` —
diffs are inspected and accepted deliberately, never blanket-accepted.
The factory's reviewer should surface this explicitly in its summary.

## 5. Roadmap pointer

The live roadmap lives in the GitHub issue labeled `factory:roadmap`.
Specs under `specs/` are prioritized there. New backlog candidates flow
in via the `factory:curator-proposals` issue.

## 6. Further reading

Canonical companion documents (at the repo root):

- `VISION.md` — source product vision (mirrored into `specs/VISION.md`).
- `ENGINEERING_PRINCIPLES.md` — engineering guardrails.
- `FFI_CONTRACT.md` — C ABI contract and stability rules.
- `STATE_OF_THE_PROJECT.md` — current progress snapshot.
- `CAPABILITIES.json` — machine-readable capability matrix.
- `AGENTS.md` / `CONTRIBUTING.md` — agent and contributor guidance.
