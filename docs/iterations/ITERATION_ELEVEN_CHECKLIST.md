# XIFty Iteration Eleven Checklist

This checklist turns the runtime-artifact and package-maturity iteration into
executable work.

## Goal

- [x] Define a canonical core runtime artifact for `xifty-ffi`
- [x] Harden package maturity in a tiered way instead of pretending parity
- [x] Keep Node as the canonical production package while improving Python and Rust

## Core Runtime Artifacts

- [x] Add a first-party runtime-artifact builder to the core repo
- [x] Add a first-party runtime-artifact validator to the core repo
- [x] Define runtime artifact naming as `xifty-runtime-<target>-v<core_version>.tar.gz`
- [x] Ensure runtime artifacts include `include/xifty.h`
- [x] Ensure runtime artifacts include platform library output in `lib/`
- [x] Ensure runtime artifacts include `manifest.json`
- [x] Include `core_version` in the manifest
- [x] Include `schema_version` in the manifest
- [x] Include target and platform identity in the manifest
- [x] Document the runtime-artifact contract
- [x] Validate runtime-artifact generation in core CI
- [x] Add a release workflow for runtime artifacts

## Binding Runtime Contract

- [x] Introduce `XIFTY_RUNTIME_DIR` as the preferred binding runtime override
- [x] Keep `XIFTY_CORE_DIR` only as an explicit source-tree override
- [x] Make bundled runtime first in the resolution order where applicable
- [x] Make repo-local runtime cache second-class to explicit overrides but ahead of source override
- [x] Remove implicit sibling/source assumptions from the new contract

## Node

- [x] Keep Node on the current prebuilt addon model
- [x] Keep manual npm publish as the current release policy
- [x] Keep supported package targets narrow and explicit
- [x] Preserve tarball smoke and real-fixture smoke discipline
- [x] Make Node’s “canonical production package” status explicit in the core/org messaging

## Python

- [x] Move Python to a runtime-artifact-backed install path
- [x] Bundle the native runtime into the built wheel
- [x] Ensure the built wheel no longer requires a core source checkout
- [x] Preserve `XIFTY_CORE_DIR` as an explicit maintainer override
- [x] Add CI validation that builds distributions
- [x] Add clean-install wheel smoke testing
- [x] Ensure the wheel is platform-tagged rather than misleadingly universal
- [x] Keep publication claims narrow and honest

## Rust

- [x] Remove the default implicit core clone/build path
- [x] Add `XIFTY_RUNTIME_DIR` support to the crate build path
- [x] Keep `XIFTY_CORE_DIR` as an explicit source override
- [x] Validate tests and examples against a runtime artifact
- [x] Validate package dry-run against an explicit runtime artifact
- [x] Keep Rust messaging honest: cleaner and more release-ready, but still source-first

## Swift / Go / C++

- [x] Do not try to fully productize Swift in this iteration
- [x] Do not try to fully productize Go in this iteration
- [x] Do not try to fully productize C++ in this iteration
- [x] Update Swift messaging to reflect source-first status explicitly
- [x] Update Go messaging to reflect source-first status explicitly
- [x] Update C++ messaging to reflect source-first status explicitly

## Docs And Public Messaging

- [x] Add a maturity matrix to the core README
- [x] Update the core state doc to reflect runtime-artifact progress
- [x] Update the capability artifact to reflect runtime artifacts
- [x] Update the org profile to reflect the real maturity ladder
- [x] Update GitHub repo descriptions where the old wording became misleading
- [x] Keep claims narrow and concrete instead of overstating publication/install maturity

## Verification

- [x] Build and validate a real runtime artifact locally
- [x] Verify Python tests against the runtime-artifact path
- [x] Verify Python wheel build and clean-install smoke path
- [x] Verify Rust tests against the runtime-artifact path
- [x] Verify Rust example execution against the runtime-artifact path
- [x] Verify Rust package dry-run with explicit runtime configuration
- [x] Confirm new core CI is green
- [x] Confirm new Python CI is green
- [x] Confirm new Rust CI is green

## Done Criteria

- [x] Core runtime artifacts are implemented, documented, and CI-validated
- [x] Python is the first self-contained package target beyond Node
- [x] Rust is cleaner and more release-ready without pretending to be turnkey
- [x] Swift, Go, and C++ remain source-first and say so clearly
- [x] The public org/repo story now matches the real package maturity ladder
