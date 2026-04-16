# XIFty Iteration Seven Plan

## Summary

Iteration seven should turn the new `C ABI` into the first two distributable
language-facing packages:

- a Node package
- a Swift package

These bindings can incubate in this repo, but they should be designed to split
cleanly into their own public repositories under the `XIFtySense` organization.

Recommended target repositories:

- `XIFtySense/XIFtyNode`
- `XIFtySense/XIFtySwift`

## Why This Iteration

The roadmap now has a proven metadata core, a stable JSON-first ABI, and the
first Python proof of embeddability. The next highest-leverage move is to
deliver the two most important consumer-facing packages on top of that seam.

This iteration should prove:

- the ABI is usable by JavaScript runtimes through Node-API
- the ABI is usable by Apple clients through SwiftPM and C interop
- the package boundaries are narrow enough to live outside the core repo

## Scope

### In scope

- incubating `bindings/node`
- incubating `bindings/swift`
- working examples for both bindings
- tests for both bindings
- docs that describe eventual extraction into dedicated repos

### Out of scope

- npm publication in this iteration
- Swift Package Index publication in this iteration
- prebuilt Node binaries for multiple platforms in this iteration
- xcframework or artifact bundle distribution in this iteration

## Node Direction

Use Node-API through the official `node-addon-api` C++ wrapper.

Why:

- Node-API is the stable native addon ABI for Node
- `node-addon-api` is the official C++ binding over Node-API
- it keeps the Node package on top of `xifty-ffi` instead of creating a second
  Rust-facing seam

Recommended package direction:

- repository: `XIFtySense/XIFtyNode`
- npm package: `@xiftysense/xifty-node`

Initial API:

- `version(): string`
- `probe(path): object`
- `extract(path, { view? }): object`

## Swift Direction

Use SwiftPM plus Swift's C interop against the checked-in XIFty C header.

Why:

- Swift packages can vend library products directly
- SwiftPM has first-class support for C-family targets and public headers
- this keeps Swift on top of the same `xifty-ffi` seam used by every other
  embedding path

Recommended package direction:

- repository: `XIFtySense/XIFtySwift`
- package/library product: `XIFtySwift`

Initial API:

- `XIFty.version() -> String`
- `XIFty.probe(path:) throws -> [String: Any]`
- `XIFty.extract(path:view:) throws -> [String: Any]`

## Packaging Notes

### Node

- use the org scope `@xiftysense`
- publish public scoped packages explicitly with `npm publish --access public`
- start with source builds in this repo, then move to prebuilt binaries in the
  dedicated package repo

### Swift

- start as a local Swift package in this repo
- keep the wrapper code thin and JSON-first
- evaluate artifact bundles / xcframework distribution after the API settles

## Success Criteria

Iteration seven is successful when:

- a Node example can call XIFty successfully
- a Swift example or test can call XIFty successfully
- both bindings are clearly layered on top of `xifty-ffi`
- both bindings are shaped so they can be extracted into dedicated public repos
  with minimal churn
