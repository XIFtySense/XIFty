# XIFty Iteration Ten Checklist

This checklist turns the Lambda-adoption iteration into executable work.

## Goal

- [x] Provide an official AWS Lambda adoption path for the Node binding
- [x] Give customers a copyable AWS SAM example
- [x] Define an honest Lambda layer story for `@xifty/xifty`

## Positioning

- [x] Document Node Lambda as the primary AWS serverless production path
- [x] Document WASM as the browser/edge evaluation path, not the primary Lambda path
- [x] Keep runtime/support claims explicit and narrow

## AWS SAM Example

- [x] Add `examples/aws-sam-node/`
- [x] Include a minimal Lambda handler using `@xifty/xifty`
- [x] Include a SAM template with function configuration
- [x] Keep the example understandable without AWS-specific sprawl
- [x] Document how to build and run the example
- [x] Document how to deploy the example

## Lambda Layer

- [x] Define a reproducible build path for a Lambda-ready layer zip
- [x] Ensure layer assembly does not depend on `../XIFty`
- [x] Target `nodejs22.x`
- [x] Target `nodejs24.x`
- [x] Validate `x86_64`
- [x] Validate `arm64` or explicitly defer it
- [x] Document layer artifact naming and usage

## Packaging And Tooling

- [x] Keep the layer/package path aligned with the real Node binding release flow
- [x] Avoid introducing a Lambda-specific public API
- [x] Avoid introducing a second serverless-only binding surface
- [x] Keep the packaging scripts understandable and local-first

## Docs

- [x] Add Lambda adoption docs to the main repo
- [x] Explain when to use Node Lambda vs WASM
- [x] Document supported runtimes and architectures
- [x] Document known limits and caveats honestly
- [x] Keep docs customer-oriented instead of maintainer-oriented

## Verification

- [x] Verify the Lambda example handler runs locally
- [x] Validate the SAM template
- [x] Verify the layer package assembles successfully
- [x] Exercise the packaging path in CI
- [x] Ensure a broken package/build path fails before release

## Capability Honesty

- [x] Do not imply Lambda parity for every binding
- [x] Do not imply WASM is the primary AWS runtime path
- [ ] Do not claim `arm64` support unless it is actually verified
- [x] Keep the Lambda story boring and operationally clear

## Done Criteria

- [x] A customer can copy the official SAM example and get started quickly
- [x] XIFty has an official Lambda layer packaging path for the Node binding
- [x] The Node Lambda adoption story is documented clearly
- [x] The runtime/architecture support matrix is explicit
- [x] kstore no longer needs to invent their own initial Lambda packaging story
