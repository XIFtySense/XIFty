# XIFty Iteration Six Checklist

This checklist turns the iteration-six plan into executable work.

## Goal

- [ ] Prove XIFty can be embedded safely through a narrow `C ABI`
- [ ] Keep the CLI and JSON contract backward compatible
- [ ] Preserve clean separation between core logic and the ABI wrapper

## FFI Crate

- [ ] Turn `xifty-ffi` into a real exported ABI crate
- [ ] Configure the crate for `cdylib` and/or `staticlib` output
- [ ] Keep all exported types `repr(C)` where applicable
- [ ] Keep Rust-internal types out of the ABI surface

## ABI Surface

- [ ] Add `xifty_probe_json`
- [ ] Add `xifty_extract_json`
- [ ] Add `xifty_free_*` support for returned memory
- [ ] Add a version/introspection function
- [ ] Add a `view_mode` ABI enum
- [ ] Add stable status/error codes

## Ownership And Errors

- [ ] Define explicit ownership rules for returned buffers
- [ ] Ensure no Rust panic crosses the C boundary
- [ ] Convert internal failures into stable ABI error results
- [ ] Handle invalid paths and null/invalid arguments defensively

## Header Generation

- [ ] Add `cbindgen` configuration
- [ ] Generate a checked-in C header
- [ ] Document how to regenerate the header
- [ ] Verify generated header output is deterministic

## Build And Tooling

- [ ] Add ABI build/test commands to docs
- [ ] Keep FFI build steps isolated from normal workspace development
- [ ] Avoid making callers install more tooling than necessary beyond documented generation steps

## Tests

- [ ] Unit tests for ABI argument validation
- [ ] Unit tests for result/buffer freeing
- [ ] Tests for stable status-code mapping
- [ ] Tests that panics/errors do not unwind across the ABI
- [ ] C integration test for `probe`
- [ ] C integration test for `extract`
- [ ] C integration test for error handling

## Fixtures

- [ ] Use checked-in minimal fixtures for ABI integration tests
- [ ] Avoid depending on local-only fixtures for ABI verification

## Docs

- [ ] Add an ABI contract document
- [ ] Document memory ownership clearly
- [ ] Document supported ABI functions and status codes
- [ ] Link iteration-six docs from the README

## Done Criteria

- [ ] A small C program can call XIFty successfully
- [ ] `probe` and `extract` work over the ABI
- [ ] Returned memory is safely releasable by callers
- [ ] The generated header is part of the repo contract
- [ ] The ABI remains narrow, boring, and honest
