# XIFty Iteration Six Checklist

This checklist turns the iteration-six plan into executable work.

## Goal

- [x] Prove XIFty can be embedded safely through a narrow `C ABI`
- [x] Keep the CLI and JSON contract backward compatible
- [x] Preserve clean separation between core logic and the ABI wrapper

## FFI Crate

- [x] Turn `xifty-ffi` into a real exported ABI crate
- [x] Configure the crate for `cdylib` and/or `staticlib` output
- [x] Keep all exported types `repr(C)` where applicable
- [x] Keep Rust-internal types out of the ABI surface

## ABI Surface

- [x] Add `xifty_probe_json`
- [x] Add `xifty_extract_json`
- [x] Add `xifty_free_*` support for returned memory
- [x] Add a version/introspection function
- [x] Add a `view_mode` ABI enum
- [x] Add stable status/error codes

## Ownership And Errors

- [x] Define explicit ownership rules for returned buffers
- [x] Ensure no Rust panic crosses the C boundary
- [x] Convert internal failures into stable ABI error results
- [x] Handle invalid paths and null/invalid arguments defensively

## Header Generation

- [x] Add `cbindgen` configuration
- [x] Generate a checked-in C header
- [x] Document how to regenerate the header
- [x] Verify generated header output is deterministic

## Build And Tooling

- [x] Add ABI build/test commands to docs
- [x] Keep FFI build steps isolated from normal workspace development
- [x] Avoid making callers install more tooling than necessary beyond documented generation steps

## Tests

- [x] Unit tests for ABI argument validation
- [x] Unit tests for result/buffer freeing
- [x] Tests for stable status-code mapping
- [x] Tests that panics/errors do not unwind across the ABI
- [x] C integration test for `probe`
- [x] C integration test for `extract`
- [x] C integration test for error handling

## Fixtures

- [x] Use checked-in minimal fixtures for ABI integration tests
- [x] Avoid depending on local-only fixtures for ABI verification

## Docs

- [x] Add an ABI contract document
- [x] Document memory ownership clearly
- [x] Document supported ABI functions and status codes
- [x] Link iteration-six docs from the README

## Done Criteria

- [x] A small C program can call XIFty successfully
- [x] `probe` and `extract` work over the ABI
- [x] Returned memory is safely releasable by callers
- [x] The generated header is part of the repo contract
- [x] The ABI remains narrow, boring, and honest

## Closeout Notes

- The first public ABI is intentionally JSON-first and limited to `probe` /
  `extract`.
- The checked-in header is regenerated and compared in test to keep the contract
  honest.
- The C smoke harness proves a non-Rust caller can probe, extract, handle an IO
  error, and release returned buffers correctly.
