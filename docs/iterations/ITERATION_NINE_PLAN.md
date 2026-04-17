# XIFty Iteration Nine Plan

## Summary

Iteration nine should add a browser-safe XIFty surface through WebAssembly and
use it to power a public demo site.

This iteration is not about porting every existing XIFty capability into the
browser. It is about proving that XIFty can run client-side, process files
locally in the user’s browser, and expose the same four-view model through a
clean web-facing contract.

The intended outcome is:

- a new `xifty-wasm` crate
- a narrow browser-facing extraction API
- a static demo site that can be hosted on GitHub Pages
- local-in-browser file processing for the initial supported formats

## Why This Iteration

XIFty is now strong in the places that matter for a WASM step:

- the core architecture is layered cleanly
- the CLI and JSON envelope are stable
- the `C ABI` exists, but is narrow and intentionally separate from internal
  Rust types
- the project now runs in more deployment environments, including Lambda-backed
  paths

The next meaningful public proof point is not only “XIFty can be embedded in
apps,” but also “XIFty can be experienced directly in a browser without sending
files to a backend.”

That makes WebAssembly strategically valuable for:

- a public demo site
- privacy-preserving evaluation
- architecture validation of the core in a constrained runtime
- future browser-facing SDK possibilities

## External Constraints

GitHub Pages is a static hosting service. It can host HTML, CSS, JavaScript,
and WebAssembly artifacts, but it does not provide server-side compute for per-
request extraction logic.

That means a GitHub Pages demo is viable only if:

- extraction happens in the browser through WASM, or
- the site talks to a separate backend API

For this iteration, the cleaner architectural choice is browser-side WASM.

Sources:

- [GitHub Pages: What is GitHub Pages?](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages)
- [GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits)
- [Rust and WebAssembly docs](https://rustwasm.github.io/docs.html)
- [The `wasm-bindgen` guide](https://rustwasm.github.io/docs/wasm-bindgen/)
- [The `wasm-pack` docs](https://rustwasm.github.io/docs/wasm-pack/)

## Primary Goal

Deliver a minimal but credible browser-native XIFty demo stack:

- a Rust WASM target crate that exposes `probe` and `extract`
- a small JS/TS wrapper suitable for browser use
- a static demo UI that can be deployed on GitHub Pages
- local file processing with no upload required

## Scope

### In scope

- a new `xifty-wasm` crate or equivalent workspace member
- a browser-safe extraction API over raw bytes
- JSON envelope output compatible with the existing product model
- a static demo app that reads local files with the browser File API
- GitHub Pages deployment for the demo UI
- a disciplined initial format matrix for browser support
- browser-specific tests and smoke verification

### Out of scope

- trying to reuse the Node binding in the browser
- exposing the `C ABI` directly to the web
- broadening the browser target to full parity with native/server surfaces
- adding a backend API for the initial public demo
- write support, repair workflows, or inspector-grade UI complexity

## Product Shape

The public demo should answer a simple question:

“Can I drop in a file and see what XIFty extracts?”

The demo should not attempt to become the long-term documentation site or a
full metadata workstation. It should stay focused on:

- drag-and-drop or file-picker upload
- quick file summary
- tabs or sections for:
  - `raw`
  - `interpreted`
  - `normalized`
  - `report`
- clear indication that processing happens locally in the browser

## Architectural Direction

### New crate boundary

Add a dedicated crate for the browser target:

- `crates/xifty-wasm`

Responsibilities:

- expose browser-facing Rust entry points
- accept raw bytes and optional filename hints
- call into existing detection/extraction logic without collapsing boundaries
- serialize outputs into the same envelope shape used elsewhere
- stay thin and avoid becoming a second policy/normalization layer

This crate should not own:

- container parsing
- metadata interpretation
- normalization policy
- demo UI code

### Web-facing API

The initial WASM contract should stay narrow.

Recommended exported functions:

- `probe_bytes(bytes: &[u8], file_name: Option<String>) -> JsValue`
- `extract_bytes(bytes: &[u8], file_name: Option<String>, view: Option<String>) -> JsValue`

Or equivalent string-returning JSON exports if that keeps the JS boundary
simpler.

The important rule is:

- preserve the existing JSON envelope shape
- do not invent a browser-only metadata model

### Why not use the C ABI here?

The C ABI is the stable cross-language embedding seam, but it is not the right
browser seam.

For WebAssembly:

- Rust-to-WASM bindings are more naturally expressed through `wasm-bindgen`
- the browser path should avoid a second foreign-function layer if possible
- the goal is a thin Rust WASM surface, not an FFI-through-WASM stack

That keeps the architecture cleaner and reduces browser-side complexity.

## Supported Browser MVP

The browser MVP should be intentionally narrower than the full native product.

### Initial supported formats

- JPEG
- TIFF
- PNG
- WebP

### Deferred initially

- HEIF / HEIC
- MP4 / MOV
- larger media-heavy workflows
- vendor-specific deep paths unless they already come along for free in the
  bounded browser slice

Rationale:

- still images are enough to prove the browser model
- they give a much better first Pages demo experience
- they reduce bundle size, complexity, and runtime surprises

If HEIF or media support turns out to work cleanly without bloating the bundle,
that can be expanded later as a follow-on iteration.

## Demo Site Direction

The demo site should be a separate static app, either:

- under `demo/` in the main repo, or
- in a dedicated public repo such as `XIFtySense/XIFtyDemo`

Recommended default:

- start in the main repo to move faster and prove the shape
- split to its own repo later only if distribution or ownership becomes cleaner

### UI principles

- simple landing message
- strong “files stay in your browser” note
- clear supported-format messaging
- no backend requirement
- avoid UI overdesign that obscures the extraction result

### Output presentation

Recommended layout:

- file summary strip
- format detection summary
- tabbed JSON views
- copy-to-clipboard affordance
- issue/conflict highlighting in the `report` tab

## Tooling Direction

Recommended tool choices:

- `wasm-bindgen`
- `wasm-pack` or an equivalent predictable build flow
- a lightweight frontend bundler only if needed
- static deployment via GitHub Pages Actions

The build should stay understandable:

- no framework complexity unless it clearly helps
- no unnecessary SSR assumptions
- no backend runtime coupling

## Testing And Verification

### Rust-side verification

- unit tests for the new WASM-facing wrapper behavior
- build checks for `wasm32-unknown-unknown`
- fixture-backed extraction tests at the Rust layer where practical

### Browser/demo verification

- a smoke test that loads the built WASM module
- a fixture-driven browser test that uploads a known file and confirms
  normalized output includes expected fields
- a Pages build/deploy workflow validation

### Contract verification

- the browser output must preserve the same envelope structure
- supported view selection must behave the same way as CLI/bindings
- malformed files must still surface `report` issues rather than fail silently

## Public-Facing Success Criteria

Iteration nine should be considered successful when:

- a public visitor can open the demo page
- choose a supported file locally
- see XIFty metadata without server upload
- inspect `raw`, `interpreted`, `normalized`, and `report` views
- and understand what formats are currently supported in-browser

## Suggested Phases

### Phase 1: WASM seam

- create `xifty-wasm`
- expose minimal `probe`/`extract` byte-based APIs
- compile successfully to `wasm32`
- prove the JSON envelope works in browser-oriented tests

### Phase 2: Browser demo

- create the static demo UI
- wire file input to the WASM module
- display the four views cleanly
- add supported-format and privacy messaging

### Phase 3: GitHub Pages delivery

- add build/deploy workflow for the demo
- publish a public demo URL
- document where the demo lives and what it supports

## Risks And Tradeoffs

### 1. Bundle size creep

If we drag too much of the native surface into the browser target, the demo
will become slow and the build story messy.

Response:

- keep the browser MVP narrow
- start with still-image formats
- measure before broadening

### 2. Boundary erosion

It would be easy to add browser-only shortcuts in the extraction path.

Response:

- keep `xifty-wasm` as a thin adapter
- do not let UI needs reshape parser/policy ownership

### 3. Browser/runtime mismatch

Some dependencies or assumptions may not behave cleanly under `wasm32`.

Response:

- adapt the surface at the crate boundary
- do not force every native path into the browser on day one

## Assumptions And Defaults

- GitHub Pages will be used as the public host for the initial demo
- browser processing is preferred over a backend for the first demo iteration
- the initial browser format scope is still-image first
- the existing JSON envelope remains the single product contract
- the WASM surface should be a thin Rust/browser adapter, not a second core
