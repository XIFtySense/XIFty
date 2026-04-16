# XIFty Iteration Seven Checklist

## Goal

- [x] Deliver the first working Node package on top of `xifty-ffi`
- [x] Deliver the first working Swift package on top of `xifty-ffi`
- [x] Keep both bindings extractable into dedicated public repos

## Node Binding

- [x] Add `bindings/node`
- [x] Add Node-API addon wrapper over `xifty-ffi`
- [x] Expose `version`, `probe`, and `extract`
- [x] Add Node tests
- [x] Add Node example
- [x] Document the intended public repo/package naming

## Swift Binding

- [x] Add `bindings/swift`
- [x] Add SwiftPM package wrapper over `xifty-ffi`
- [x] Expose `version`, `probe`, and `extract`
- [x] Add Swift tests
- [x] Add Swift example or documented invocation path
- [x] Document the intended public repo/package naming

## Packaging Hygiene

- [x] Keep runtime/build caches ignored
- [x] Avoid introducing a new native seam that bypasses `xifty-ffi`
- [x] Keep both bindings JSON-first in the initial slice

## Verification

- [x] Node example runs successfully
- [x] Node tests pass
- [x] Swift tests pass
- [x] Workspace tests remain green

## Closeout Notes

- `bindings/node` is intentionally shaped to extract into `XIFtySense/XIFtyNode`
  with package name `@xiftysense/xifty-node`.
- `bindings/swift` is intentionally shaped to extract into `XIFtySense/XIFtySwift`
  with library product `XIFtySwift`.
- Both bindings remain thin wrappers over `xifty-ffi`, preserving the single C
  ABI seam for language packages.
