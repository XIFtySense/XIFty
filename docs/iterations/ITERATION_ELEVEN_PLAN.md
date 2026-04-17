# XIFty Iteration Eleven Plan

## Summary

Iteration eleven focuses on package maturity through a canonical runtime
artifact for `xifty-ffi`.

The intent is to improve package/distribution quality without redesigning the
core metadata engine:

- keep Node as the canonical production package
- make Python genuinely self-contained on supported targets
- make Rust release-ready while still honestly source-first
- keep Swift, Go, and C++ explicitly source-first for now

This iteration is about turning the proven `xifty-ffi` seam into a reusable
runtime bundle and then using that bundle to make package maturity claims more
honest and more testable.

## Why This Iteration

XIFty had reached a point where the main architectural questions were largely
answered:

- the four-view model was stable
- the `C ABI` existed and was documented
- Node had become the most production-ready public package
- the browser demo and Lambda path proved the core could run in more places

What remained weak was package maturity outside Node.

The old pattern of “prepare core source locally and build against it” was fine
for incubation, but it was not a strong foundation for public package maturity.
Iteration eleven therefore focuses on a narrower and more valuable step:

- define one canonical runtime-bundle contract in the core repo
- use it to harden Python first
- use it to clean up Rust next
- keep the rest of the binding repos honest rather than pretending equal
  maturity

## Primary Goal

Deliver a real runtime-artifact story for package-facing bindings and use it to
establish a clear public maturity ladder across the org.

## Scope

### In scope

- canonical `xifty-ffi` runtime bundles in the core repo
- runtime-artifact build and validation tooling
- core CI and release workflow support for runtime artifacts
- `XIFTY_RUNTIME_DIR` as the preferred runtime contract for bindings
- Python self-contained wheel path on supported targets
- Rust runtime-backed validation and local-use path
- clearer public maturity messaging across the core repo, binding repos, and
  org profile

### Out of scope

- redesigning the metadata engine
- broad new metadata capability work
- making Swift, Go, or C++ fully packaged in this iteration
- crates.io or PyPI publication-by-default if the install story is not yet
  fully honest
- wider target expansion beyond:
  - `macos-arm64`
  - `linux-x64`

## Product Shape

At the end of iteration eleven, the public story should be:

- Node is the canonical production package today
- Python is the first self-contained wheel target beyond Node
- Rust is cleaner and more release-ready, but still source-first
- Swift, Go, and C++ remain source-first bindings
- the core repo publishes the canonical runtime bundle contract the other
  bindings can build on

## Architectural Direction

### Canonical runtime bundle in core

The core repo should define a reusable runtime artifact with:

- `include/xifty.h`
- `lib/libxifty_ffi.dylib` or `lib/libxifty_ffi.so`
- `manifest.json`

Release assets should use:

- `xifty-runtime-<target>-v<core_version>.tar.gz`

### Shared binding contract

Bindings should prefer the same runtime resolution order:

1. bundled runtime inside the package, if present
2. `XIFTY_RUNTIME_DIR`, if explicitly set
3. repo-local runtime cache populated from canonical core release artifacts
4. `XIFTY_CORE_DIR` only as an explicit source override

Bindings should not infer a source-build path from sibling checkouts or stale
caches.

### Tiered package maturity

Do not pretend every binding is equally packaged.

The maturity ladder should be explicit in:

- the core README
- the org profile
- binding READMEs
- GitHub repo descriptions

## Deliverables

### 1. Core runtime-artifact contract

- build script for runtime bundles
- validation script for runtime bundles
- CI coverage
- release workflow
- adoption doc

### 2. Python package hardening

- wheel path that bundles the runtime
- clean wheel install without cloning core
- CI/release validation that proves the built wheel actually works

### 3. Rust package hardening

- runtime-backed local/CI/release validation path
- explicit source override only when maintainers ask for it
- no implicit clone/build path by default

### 4. Messaging and org alignment

- core README maturity matrix
- updated state/capability language
- Swift/Go/C++ README clarification
- org profile and repo-description refresh

## Testing And Verification

### Core

- build runtime artifact locally
- validate artifact layout and manifest
- exercise runtime-artifact CI job

### Python

- unit tests
- examples
- wheel build
- clean wheel install smoke test

### Rust

- cargo test
- examples
- cargo publish dry-run using explicit `XIFTY_RUNTIME_DIR`

## Acceptance Criteria

- canonical runtime artifacts exist in the core repo contract
- core CI validates runtime artifact generation
- Python no longer requires a core source checkout for the built wheel
- Rust no longer clones core implicitly by default
- public messaging reflects the real maturity ladder instead of implying parity

## Done Criteria

Iteration eleven is done when:

- the runtime-artifact contract is implemented and documented in core
- Python and Rust consume that contract in a real, validated way
- source-first bindings are explicitly described as such
- the org-level public story matches the repo reality
