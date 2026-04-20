# XIFty State Of The Project

## Summary

XIFty is now in a strong architectural and product-shaping position.

The core thesis from the vision has been proven in code:

- metadata is not treated as a flat tag dump
- container parsing stays separate from metadata interpretation
- normalization is policy-driven
- provenance, conflicts, and issues are first-class concerns
- the JSON envelope and four-view model remain stable as capability expands
- the engine is now embeddable through a real `C ABI`
- the project now has a real browser-native WASM surface and public demo
- the project now has a first-party Node-on-Lambda adoption path
- the project now has checked-in JSON schema artifacts and schema lifecycle
  rules
- the project now has release hygiene that verifies shipped artifacts, not just
  repo code
- the project now has a canonical runtime-artifact contract for package-facing
  bindings

The main remaining gaps are no longer architectural. They are breadth,
distribution maturity, deployment ergonomics, and ecosystem polish relative to
the full vision.

## What Is Proven

### 1. The core architectural rule is holding

The most important design constraint in XIFty is that container parsing and
metadata interpretation remain separate.

That rule is now exercised across:

- JPEG / TIFF
- PNG / WebP
- HEIC / HEIF / ISOBMFF
- MP4 / MOV / media-oriented ISOBMFF

The same high-level extraction model survives segment-based, chunk-based,
box-based, and item-based containers without collapsing normalization or policy
into parser code.

### 2. The four-view model is real and stable

The vision called for:

- `raw`
- `interpreted`
- `normalized`
- `report`

That shape now exists in the CLI contract, the JSON-first ABI, and the first
binding layers. This is a meaningful milestone because it proves XIFty is
becoming a metadata engine with a clear product model, not a parser that merely
emits JSON.

### 3. Reconciliation is no longer speculative

XIFty already reconciles overlapping metadata across namespaces and containers.

What is now proven:

- EXIF and XMP coexistence
- policy-driven normalized field selection
- explicit conflict reporting from two independent sources: entry-level
  detection rules in `xifty-validate` and winner-selection conflicts from
  `xifty-policy`
- provenance-preserving normalized derivation
- bounded editorial precedence such as XMP-over-IPTC where explicitly modeled

`xifty-validate` now runs three rule families over the full `entries` slice
before normalization: cross-namespace semantic-tag disagreement (e.g. EXIF
`Make` vs XMP `Make` for the same file), timestamp timezone/offset mismatch
(same wall time, differing UTC offset), and numeric precision mismatch (0.5%
relative tolerance for ISO, aperture, focal length, shutter speed). Both
validate- and policy-produced conflicts are accumulated with `.extend` in the
CLI path, so `Report.conflicts` can contain entries from either source.

That moves the project beyond "can we decode bytes?" into "can we derive
explainable, stable application fields from messy real-world metadata?"

### 4. Modern still-image and bounded media support are real

The repository now supports meaningful coverage across:

- still-image formats: JPEG, TIFF, PNG, WebP, HEIF / HEIC
- bounded media containers: MP4, MOV
- namespaces: EXIF, XMP, bounded ICC, bounded IPTC, bounded QuickTime
- vendor-specific metadata paths: Sony MakerNotes, Sony RTMD, Apple MakerNotes

This matters because modern metadata systems get hard precisely where formats,
namespaces, and vendor ecosystems overlap. XIFty is no longer only proving a
clean design on simple cases.

### 5. XIFty is now an embeddable engine, not just a CLI

The largest roadmap gap after the metadata iterations was embeddability.

That gap is now materially closed at the core level:

- `xifty-ffi` is a real `C ABI`
- the ABI is narrow, JSON-first, and explicitly documented
- ownership and status/error semantics are defined
- checked-in headers are generated and tested deterministically
- a C harness proves non-Rust callers can probe, extract, handle errors, and
  free returned buffers correctly

This is a major vision milestone because the project now has the stable low-
level seam required to support multiple languages and embedding environments.

### 6. The binding ecosystem has started to exist

The vision called for thin bindings above a stable core seam.

That now exists as extracted organization repos for language-facing packages:

- `XIFtyNode`
- `XIFtySwift`
- `XIFtyPython`
- `XIFtyGo`
- `XIFtyRust`
- `XIFtyCpp`

This is important because XIFty is no longer only a core-engine project. It is
beginning to become a small ecosystem centered on the same `C ABI`.

### 7. Capability discipline is improving

The repository is doing a better job of being honest about what it supports:

- `CAPABILITIES.json` records bounded capability claims explicitly
- `CAPABILITIES.json` is now validated against observed CLI output by
  `tools/generate_capabilities.py --check`, wired into the `Hygiene`
  workflow — under-reporting drift fails CI automatically
- checked-in JSON Schema artifacts now exist for the probe and analysis
  envelopes
- a schema policy now defines additive vs breaking JSON changes
- local-only large camera/media fixtures are kept out of git
- differential testing exists for supported oracle-backed slices
- iteration checklists have been used to close scope honestly instead of
  implying completeness

This matters because a metadata engine can easily overclaim. XIFty has been
moving in the healthier direction, and capability claims are now backed by
generated evidence rather than hand-editing alone.

### 10. Release discipline is starting to match repository discipline

One of the most important recent lessons was that "fix merged in the repo" is
not the same thing as "fix shipped to users."

That gap is now being addressed more explicitly:

- the core repo has a checked-in release checklist
- hygiene now validates real CLI output against checked-in JSON schemas
- the Node package now smoke-tests the packed tarball, not just the repo
  checkout
- release-sensitive local verification can now be pointed at real local
  fixtures such as the Sony XAVC sample that previously exposed a shipped
  artifact mismatch

This is a meaningful milestone because XIFty is now hardening the path between
correct code and correct customer-facing artifacts.

### 8. Browser-native inspection is now real

The vision increasingly implied that XIFty should be usable in more places than
just a CLI or server process.

That is now materially true:

- `xifty-wasm` exists as a dedicated browser-facing surface
- the same four-view JSON envelope is exposed in the browser path
- the public GitHub Pages demo processes files locally in the browser
- the browser demo now presents normalized facts, grouped inventories, readable
  timestamps, GPS, and report evidence in a product-shaped way

This matters because XIFty is no longer only proving embeddability for native
callers. It is also proving that the core metadata model survives a real
browser UX without inventing a second product model.

### 9. Serverless adoption has a first-party path

XIFty now has an official Node-on-Lambda story:

- a checked-in AWS SAM example
- a reproducible Lambda layer assembly path
- adoption documentation for the Node Lambda path
- main-CI validation of `sam validate`, local fixture invocation, layer
  preparation, and `sam build`

This is important because XIFty is now proving not just that the engine can be
embedded, but that it can be adopted in a production-oriented serverless
environment without inventing a bespoke integration story.

## Where XIFty Now Stands Relative To The Vision

### Vision areas that are substantially achieved

These parts of the vision are now materially real:

- a modular Rust core
- a stable JSON-based CLI contract
- a real `C ABI` embedding seam
- four explicit metadata views
- first-class provenance, validation, conflicts, and normalized fields
- support across the intended first container families
- thin language-facing wrappers built above the ABI rather than around it
- a browser-native WASM demo surface
- a first-party Node-on-Lambda adoption path
- a managed JSON schema surface for the public envelope
- early shipped-artifact hygiene for core and Node releases

In other words: the project's architectural promises are mostly no longer
aspirational.

### Vision areas that are partially achieved

These parts are present, but still deliberately bounded:

- EXIF support
- XMP support
- ICC support
- IPTC support
- QuickTime/media support
- vendor-specific metadata support
- capability reporting
- multi-language package ecosystem
- browser-native UX
- serverless adoption
- public schema governance
- shipped-artifact verification discipline

XIFty is now on the board in all of these areas, but it is not yet broad or
deep enough to claim exhaustive support.

### Vision areas that remain clearly unfinished

The clearest remaining gaps relative to the original vision are:

- broader QuickTime metadata coverage and richer media semantics
- broader ICC and IPTC coverage beyond the current bounded slice
- more complete vendor ecosystems beyond the current Sony and Apple paths
- stronger machine-readable capability reporting tied to generated facts/tests
- richer downstream SDK surfaces and future inspector/documentation tooling
- distribution maturity for the public package repos
- broader first-party deployment paths beyond the current Node Lambda story
- eventual write/repair-oriented workflows, which remain intentionally out of
  scope for now
- fully automated package publishing/release discipline across all public
  binding repos

## What Is Still Missing

### Breadth gaps

The core architecture is proven, but the supported metadata breadth is still
narrow compared with the long-term ambition.

Important remaining breadth gaps include:

- deeper QuickTime atoms and metadata semantics
- more editorial / rights-oriented IPTC coverage
- broader ICC tag coverage and richer color normalization
- deeper audio/video metadata normalization
- richer HEIF and ISOBMFF metadata families
- more vendor-specific namespaces and camera ecosystems

These are product-capability gaps, not design gaps.

### Distribution and packaging gaps

This is now one of the biggest practical gaps.

The binding repos now have a clearer maturity ladder, but the ecosystem is
still deliberately tiered rather than uniformly turnkey:

- Node now has a real npm package with a documented Lambda path, but its
  release automation and supported-platform matrix are still intentionally
  narrow
- the core repo now defines canonical `xifty-ffi` runtime bundles for
  `macos-arm64` and `linux-x64`
- Python is now on a self-contained wheel path built around those runtime
  artifacts, but that story still needs publication discipline and broader
  target coverage before it feels finished
- Rust is now on the same runtime contract for release validation and local
  use, but it remains honestly source-first rather than a turnkey published
  crate
- Swift, Go, and C++ still do not yet have a cleaner artifact/package
  distribution strategy

This means XIFty has proven embeddability, but not yet frictionless adoption.

### Release and artifact gaps

XIFty is in a much better place here than it was even a short while ago, but
this area is still maturing:

- the core repo now has schema and release hygiene, but that process still
  depends partly on human discipline
- the Node package now verifies packed tarballs and real local fixtures, but
  manual npm publishing is still the active release path
- Python and Rust are moving onto cleaner runtime-backed validation paths, but
  the other binding repos do not yet all have the same level of
  shipped-artifact verification parity

So the project now understands this class of failure much better, but has not
fully eliminated it across the ecosystem.

### Corpus and verification gaps

The project has strong discipline here, but there is still room to improve:

- richer local/private corpora need clearer long-term process and tooling
- some of the strongest real-camera regression coverage remains local-only by
  design
- the newest fuzz targets are checked in, but local smoke execution is still
  environment-sensitive on this machine
- the browser demo is real and public, but browser-level automated smoke
  coverage is still lighter than the core CLI/FFI surface

## Assessment

### Where XIFty is ahead

XIFty is ahead of where many projects would be at this stage in:

- architectural discipline
- layering and separation of concerns
- honesty about supported capabilities
- provenance/conflict/report modeling
- incremental iteration closure
- embeddability design

That is exactly where an architecture-first project should be ahead.

### Where XIFty is behind

XIFty is behind mainly in:

- breadth of supported metadata families
- distribution maturity for public consumer packages
- higher-level SDK/documentation surface area
- broader deployment packaging and runtime coverage
- broader external-corpus and capability automation
- ecosystem-wide release automation and shipped-artifact verification parity

This is a healthy trade so far. The project chose to prove durable structure
before chasing superficial completeness.

What has changed recently is that XIFty is also starting to prove the
operational layer around that structure: schema governance, release checklists,
artifact smoke checks, canonical runtime bundles, and customer-facing
verification against real local fixtures.

## Roadmap Implication

The next phase should not redesign the core.

The main architecture is already doing the job it was meant to do. The highest-
leverage work now is to convert that proven core into a more complete and more
consumable platform.

That suggests the next roadmap focus should likely be one of these:

- package/distribution hardening for the public binding repos
- deeper deployment and runtime adoption stories beyond the first Lambda slice
- deeper media and QuickTime metadata coverage
- broader corpus coverage to strengthen capability-generation signal
- selected namespace-depth work where the product value is highest

## Recommended Next-Step Framing

The best framing now is:

**XIFty has largely proven its core architecture and first embedding seam. The
next stage should focus on turning that proven engine into a broader, easier-
to-consume metadata platform without sacrificing capability honesty.**

That means:

- keep the current architecture
- deepen support deliberately
- improve packaging and adoption ergonomics
- keep capability claims narrow, explicit, and test-backed
