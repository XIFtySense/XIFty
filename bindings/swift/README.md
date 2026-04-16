# Swift Binding

This is the incubating Swift package for XIFty.

It is designed to split into its own public repository under the `XIFtySense`
organization once the package shape settles.

Recommended extraction target:

- repo: `XIFtySense/XIFtySwift`
- package product: `XIFtySwift`

## Architecture

- package manager: Swift Package Manager
- C interop target: `CXIFty`
- core dependency: `xifty-ffi`
- data exchange: JSON strings decoded with `Foundation`

## Local Development

```bash
cargo build -p xifty-ffi
cd bindings/swift
DYLD_LIBRARY_PATH=../../target/debug swift test
```
