# XIFty Iteration Six Plan

## Summary

Iteration six should shift from metadata breadth to embedding viability.

The first five iterations established:

- clean parser / namespace / policy / normalization boundaries
- a stable CLI and JSON envelope
- still-image support across EXIF / XMP / ICC / IPTC
- bounded media metadata support across MP4 / MOV
- explicit capability reporting
- test-backed iteration closure

The largest remaining roadmap gap is no longer "can the core parse enough
metadata?" It is "can the core be embedded safely and durably by other
languages and applications?"

The strongest next move is therefore:

- a narrow but real `C ABI`
- a disciplined memory / ownership contract
- generated headers
- a small integration surface for `probe` and `extract`

This iteration should not attempt full bindings for `Node`, `Python`, or
`Swift`. It should establish the ABI those bindings would rely on.

## Why This Iteration

This is the highest-leverage roadmap move now because:

- the vision explicitly calls for a stable `C ABI`
- the architecture is now mature enough to expose a narrow core contract
- future bindings become cheaper and safer once the ABI contract is real
- the next failure risk is interface design, not parsing architecture

In other words: the next thing to prove is that XIFty is an engine, not just a
CLI.

## Primary Goal

Build the first production-shaped embedding surface for XIFty by:

- turning `xifty-ffi` from a placeholder into a usable `C ABI` crate
- exposing a small set of stable entry points
- defining the memory, string, error, and lifecycle rules explicitly
- generating a C header from the Rust ABI surface
- validating the ABI with at least one C integration test

## Scope

### In scope

- `xifty-ffi` as the sole FFI surface
- `cdylib` and/or `staticlib` output for the FFI crate
- `probe` and `extract` through the ABI
- UTF-8 JSON output as the first exchange format
- error codes plus an optional message channel
- explicit allocation / free functions for returned buffers
- header generation with `cbindgen`
- C smoke/integration test(s)
- docs for the ABI contract and examples of C usage

### Out of scope

- language-specific bindings for `Node`, `Python`, or `Swift`
- exposing the full internal type graph over FFI
- callback-heavy streaming APIs
- async FFI
- write support
- stable semver guarantees beyond the explicitly documented ABI slice
- redesigning the core metadata pipeline

## Recommended Surface

The first ABI should stay deliberately small.

Suggested exported functions:

- `xifty_probe_json(path, out_result)`
- `xifty_extract_json(path, view_mode, out_result)`
- `xifty_free_buffer(ptr, len, capacity)` or an equivalent opaque result free
- `xifty_last_error_message(...)` only if needed after keeping errors in result
  structs
- `xifty_version(...)`

Recommended result model:

- a `repr(C)` result struct with:
  - status / error code
  - pointer to UTF-8 bytes
  - length
  - capacity or opaque handle info needed for free

Recommended view-mode model:

- a `repr(C)` enum matching the current CLI views:
  - full
  - raw
  - interpreted
  - normalized
  - report

## Design Principles

### 1. JSON first, structs later

The first ABI should return JSON, not a graph of nested C structs.

Why:

- the CLI contract already proves the JSON envelope
- bindings can start with a stable text format
- it avoids freezing an immature native object model into the ABI too early

### 2. Ownership must be boring

The ABI must make allocation and release obvious:

- XIFty allocates returned buffers
- the caller releases them only through XIFty-provided free functions
- no caller-provided allocators in this iteration
- no borrowed pointers whose lifetime is tied to Rust stack frames

### 3. Panics must not cross the boundary

The ABI should catch internal failures and convert them to error results.

The exported `extern "C"` layer should never permit Rust panics to unwind into
C callers.

### 4. Rust internals remain internal

The ABI must not expose:

- Rust enums without `repr(C)`
- Rust strings or slices directly
- internal crate types
- ownership patterns that require the caller to understand Rust

### 5. One stable seam

All embedding should go through `xifty-ffi`.

The CLI should continue to depend on Rust crates directly, but future bindings
should treat the `C ABI` as the stable low-level contract.

## Crate / Build Changes

### `xifty-ffi`

Iteration six should turn `xifty-ffi` into a real crate that:

- depends on `xifty-cli` or a narrow orchestration layer for `probe` and
  `extract`
- exports `extern "C"` functions with `#[unsafe(no_mangle)]`
- defines `repr(C)` result and enum types
- converts internal errors into stable ABI error codes/messages

### Build / tooling

Add:

- `cbindgen` configuration
- generated header output path
- an integration test or small C harness
- build/test docs for ABI verification

Do not:

- expose all core crates directly as ABI-facing dependencies
- generate headers from arbitrary internal modules

## API Shape

Suggested C-facing types:

- `xifty_status_code`
- `xifty_view_mode`
- `xifty_buffer`
- `xifty_result`

Suggested status categories:

- success
- invalid_argument
- io_error
- unsupported_format
- parse_error
- internal_error

The exact names matter less than:

- `repr(C)` layout
- stable semantics
- documented ownership rules

## Testing Strategy

### Rust-side tests

- unit tests for FFI argument validation
- unit tests for buffer/result freeing
- tests ensuring no panic escapes the ABI boundary

### Header / generation tests

- test or CI step that regenerates the header deterministically
- verify the checked-in header matches generated output

### C integration test

Add a small C program or harness that:

- calls `xifty_probe_json`
- calls `xifty_extract_json`
- checks returned JSON bytes are non-empty and valid enough for the test case
- frees returned buffers correctly

This should use the checked-in minimal fixtures, not local-only corpus files.

## Suggested Phases

### Phase 1: ABI contract design

- define exported result / enum / buffer types
- define ownership and error semantics in docs
- keep the API intentionally tiny

### Phase 2: Implement probe/extract ABI

- route ABI calls into existing orchestration
- return UTF-8 JSON buffers
- add free/version functions

### Phase 3: Header generation

- add `cbindgen`
- generate and check in the C header
- document how to regenerate it

### Phase 4: ABI verification

- add C integration harness
- test probe/extract on minimal fixtures
- validate error handling for bad paths and unsupported files

## Success Criteria

Iteration six is successful when:

- XIFty exposes a real, documented `C ABI`
- `probe` and `extract` can be called from C
- returned memory can be released safely and predictably
- the generated header is part of the repo contract
- the ABI remains narrow and does not leak Rust internals
- the CLI contract stays unchanged

## Sources And Guidance

This iteration direction is aligned with primary sources and current guidance:

- Rustonomicon FFI guidance on calling Rust from C and unwinding:
  [doc.rust-lang.org](https://doc.rust-lang.org/beta/nomicon/ffi.html)
- Rust Reference ABI material:
  [doc.rust-lang.org](https://doc.rust-lang.org/reference/abi.html)
- `std::ffi` string and pointer interop:
  [doc.rust-lang.org](https://doc.rust-lang.org/std/ffi/)
- `cbindgen` project guidance for generating C headers from Rust public C APIs:
  [github.com/mozilla/cbindgen](https://github.com/mozilla/cbindgen)

Inference from those sources:

The safest first ABI for XIFty is a minimal JSON-returning `extern "C"` layer
with explicit ownership and generated headers, not a large native struct graph
or direct language-specific binding work.
