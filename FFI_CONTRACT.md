# XIFty FFI Contract

## Purpose

This document defines the first public embedding seam for XIFty.

Iteration six keeps the ABI intentionally narrow:

- `probe` as JSON
- `extract` as JSON
- explicit status codes
- explicit ownership for returned buffers
- no exposure of Rust-internal object graphs

## Design Rules

- The FFI surface lives only in `xifty-ffi`.
- The ABI returns UTF-8 JSON, not nested native structs.
- All exported ABI types are `repr(C)`.
- Callers never free Rust-owned memory directly.
- Rust panics must never unwind across the C boundary.

## Exported Functions

- `xifty_probe_json(const char *path)`
- `xifty_extract_json(const char *path, XiftyViewMode view_mode)`
- `xifty_free_buffer(struct XiftyBuffer buffer)`
- `xifty_version(void)`

## Returned Types

### `XiftyBuffer`

`XiftyBuffer` owns a returned byte buffer.

Fields:

- `ptr`: pointer to UTF-8 bytes
- `len`: initialized byte length
- `capacity`: allocation capacity used for safe release

Rules:

- Buffers are allocated by XIFty.
- Callers must release returned buffers only with `xifty_free_buffer`.
- Null buffers with zero length/capacity are valid empty buffers.

### `XiftyResult`

`XiftyResult` carries the ABI status plus either JSON output or an error message.

Fields:

- `status`
- `output`
- `error_message`

Rules:

- On success, `status == XIFTY_STATUS_CODE_SUCCESS` and `output` contains JSON.
- On error, `error_message` contains a human-readable UTF-8 message.
- Callers should free any non-empty `output` and `error_message` buffers.

## Status Codes

- `SUCCESS`
- `INVALID_ARGUMENT`
- `IO_ERROR`
- `UNSUPPORTED_FORMAT`
- `PARSE_ERROR`
- `INTERNAL_ERROR`

In the generated C header, these appear as namespaced enum values such as:

- `XIFTY_STATUS_CODE_SUCCESS`
- `XIFTY_STATUS_CODE_IO_ERROR`
- `XIFTY_VIEW_MODE_NORMALIZED`

These codes are intentionally coarse in the first ABI. The detailed diagnostics
remain in the JSON `report` and human-readable error strings.

## Ownership

- Input paths are borrowed C strings and remain owned by the caller.
- Returned buffers are owned by XIFty until the caller releases them.
- `xifty_version()` returns a static NUL-terminated string and must not be freed.

## Regenerating The Header

Run:

```bash
cbindgen --config cbindgen.toml --crate xifty-ffi --output include/xifty.h --lang c
```

## Verification

Recommended checks:

```bash
cargo test -p xifty-ffi
cargo test --workspace
```

The Rust integration test in `crates/xifty-ffi/tests/c_abi.rs` also regenerates
the header, checks it against the checked-in copy, compiles a small C harness,
and runs it against checked-in minimal fixtures.

## Example Consumer

A minimal C example is checked in at `examples/c/basic_usage.c`.

Build it against a debug library build with:

```bash
cargo build -p xifty-ffi
cc examples/c/basic_usage.c -I include -L target/debug -lxifty_ffi -o target/basic_usage
./target/basic_usage fixtures/minimal/happy.jpg
```
