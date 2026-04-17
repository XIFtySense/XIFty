# XIFty Iteration Nine Checklist

This checklist turns the WASM/browser-demo iteration into executable work.

## Goal

- [ ] Add a browser-safe XIFty surface through WebAssembly
- [ ] Preserve the existing four-view JSON product model in the browser
- [ ] Publish a public demo experience through GitHub Pages

## WASM Boundary

- [ ] Add a new `crates/xifty-wasm` workspace member
- [ ] Keep `xifty-wasm` thin and adapter-like
- [ ] Avoid leaking parser, policy, or normalization logic into the WASM crate
- [ ] Expose byte-oriented `probe` and `extract` entry points
- [ ] Preserve the existing JSON envelope shape
- [ ] Avoid routing the browser path through the `C ABI`

## Browser MVP Scope

- [ ] Support JPEG in the browser MVP
- [ ] Support TIFF in the browser MVP
- [ ] Support PNG in the browser MVP
- [ ] Support WebP in the browser MVP
- [ ] State clearly that browser support is intentionally narrower than native/server support
- [ ] Explicitly defer HEIF / HEIC for the first WASM slice unless it comes in cleanly
- [ ] Explicitly defer MP4 / MOV for the first WASM slice

## Build And Tooling

- [ ] Add the required WASM-target dependencies and build configuration
- [ ] Build successfully for `wasm32-unknown-unknown`
- [ ] Choose and document the WASM packaging workflow (`wasm-bindgen`, `wasm-pack`, or equivalent)
- [ ] Keep the browser build path understandable and boring
- [ ] Avoid introducing unnecessary frontend/framework complexity

## Demo App

- [ ] Add a static demo UI
- [ ] Support drag-and-drop or file-picker upload
- [ ] Process files locally in the browser
- [ ] Show a strong “files stay in your browser” message
- [ ] Present `raw`, `interpreted`, `normalized`, and `report` clearly
- [ ] Surface malformed-file issues rather than failing silently
- [ ] Add copy/export affordances for JSON output if they stay simple

## Testing

- [ ] Add Rust-side tests for the WASM wrapper behavior
- [ ] Add a build check for the WASM target
- [ ] Add fixture-backed browser/demo smoke coverage
- [ ] Verify the browser output preserves the JSON envelope contract
- [ ] Verify supported view selection behaves consistently with the CLI
- [ ] Verify at least one malformed fixture still produces `report` issues

## GitHub Pages

- [ ] Decide whether the initial demo lives in the main repo or a dedicated demo repo
- [ ] Add a GitHub Pages build/deploy workflow
- [ ] Confirm the generated Pages artifact is static-only
- [ ] Publish a public demo URL
- [ ] Link the demo from the main repo once it is real

## Capability Honesty

- [ ] Document the browser-supported format matrix clearly
- [ ] Keep the browser support claims narrower than the native support claims
- [ ] Make the privacy model explicit: local processing, no upload
- [ ] Avoid implying that the web demo is full product parity

## Done Criteria

- [ ] XIFty can compile to WebAssembly through a dedicated crate
- [ ] A user can open the public demo and inspect metadata from a local file
- [ ] The browser path preserves the four-view XIFty product model
- [ ] The demo is hosted statically through GitHub Pages
- [ ] Browser support boundaries are explicit and honest
