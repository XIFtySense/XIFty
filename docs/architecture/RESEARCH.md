# XIFty Research Notes

This document captures the research behind the current XIFty architecture and implementation plan.

It is intentionally source-backed and decision-oriented.

## Executive Summary

The research supports the original instinct behind XIFty:

- ExifTool is the coverage benchmark, not the architectural model to copy.
- The standards landscape is fragmented across EXIF, XMP, IPTC, ICC, QuickTime/ISOBMFF, and format-specific embedding rules.
- A modern engine should separate container parsing, namespace interpretation, normalization, and conflict reporting.
- `Rust` remains the best fit for the core because it combines parser-friendly performance with memory safety and a solid FFI story.
- A stable `C ABI` is the safest long-term embedding surface for `Node`, `Python`, and `Swift`.

## Standards and Ecosystem Findings

### EXIF is still moving

The official EXIF specification is maintained by CIPA, and the current English listing shows `Exif Version 3.1` published on January 30, 2026.

This matters for XIFty in two ways:

- The project should not hard-code assumptions around older EXIF revisions.
- Spec-version awareness should be built into validation and reporting from the start.

Source:

- [CIPA standards page](https://www.cipa.jp/e/std/std-sec.html)

### XMP is more than XML serialization

Adobe’s XMP docs are especially important because they split the problem into:

- data model and core properties
- additional standard properties
- storage in files
- reconciliation with other metadata formats

That last point is central to XIFty. Adobe explicitly frames XMP as something that must coexist with non-XMP metadata and be reconciled when values overlap.

Source:

- [Adobe XMP specifications](https://developer.adobe.com/xmp/docs/xmp-specifications/)

### ExifTool’s breadth is the benchmark to test against

ExifTool documents itself as a read/write metadata tool across a very large set of file types, and its application docs explicitly list supported formats with `r`, `w`, and `c` capabilities.

That makes ExifTool the right differential oracle for XIFty:

- tag discovery parity
- normalized-field comparison
- malformed-file behavior comparison
- explicit capability tracking by format

But it is not a good shape to copy wholesale. Its strength is accumulated coverage over time, not necessarily a clean modern internal layering.

Sources:

- [ExifTool application docs](https://exiftool.org/exiftool_pod2.html)
- [ExifTool tag documentation](https://exiftool.org/TagNames/)

### HEIF / AVIF support has real ecosystem leverage

`libheif` is notable because its public API is a `C API`, it exposes metadata extraction, and it can read EXIF from HEIF items. It also notes that HEIF image sequences are close enough to MP4 video that it can handle MP4 video without audio.

This is a strong signal that XIFty should treat HEIF/AVIF and ISOBMFF/QuickTime as closely related at the container layer.

Sources:

- [libheif README](https://github.com/strukturag/libheif)
- [mp4parse docs](https://docs.rs/mp4parse/latest/mp4parse/)

## Language and FFI Findings

### Rust remains the best core language choice

The strongest reasons from current sources are:

- Rust’s `extern "C"` support is straightforward for exported functions.
- `#[repr(C)]` gives explicit layout guarantees for types crossing the ABI boundary.
- Cargo workspaces are a natural fit for a modular crate architecture.

This lines up well with a parser-heavy, long-lived systems project.

Sources:

- [Rust `extern` docs](https://doc.rust-lang.org/beta/std/keyword.extern.html)
- [Rust type layout reference](https://doc.rust-lang.org/nightly/reference/type-layout.html)
- [Cargo reference](https://doc.rust-lang.org/cargo/reference/)

### C ABI is safer than C++ ABI for the public bridge

This is an inference from the sources, but a strong one:

- Rust’s official docs give direct, stable guidance for `extern "C"` and `repr(C)`.
- Swift’s C++ interoperability exists and is useful, but the Swift docs still describe it as an evolving feature and discuss compatibility-versioning over time.

So the best public contract for XIFty is:

- stable `C ABI`
- language-specific ergonomic wrappers on top

That keeps the core embedding surface narrow and durable.

Sources:

- [Rust `extern` docs](https://doc.rust-lang.org/beta/std/keyword.extern.html)
- [Rust type layout reference](https://doc.rust-lang.org/nightly/reference/type-layout.html)
- [Swift C++ interoperability docs](https://www.swift.org/documentation/cxx-interop/)

### Python and Node both have good Rust binding paths

For `Python`, PyO3 is mature and well-documented.

For `Node`, napi-rs provides a clean path for precompiled native addons and good platform coverage.

These are good wrapper technologies, but they should remain wrappers. They should not define the core XIFty API contract.

Sources:

- [PyO3 guide](https://pyo3.rs/)
- [napi-rs docs](https://napi.rs/)

## Existing Rust Library Findings

### `kamadak-exif`

Useful as a reference point because it is a pure-Rust EXIF parser and directly supports:

- TIFF and TIFF-based RAW
- JPEG
- HEIF / HEIC / AVIF
- PNG
- WebP

Its README also points to the standards it targets, which is helpful when scoping early conformance expectations.

Source:

- [kamadak/exif-rs](https://github.com/kamadak/exif-rs)

### `nom-exif`

Useful because it stretches beyond still images and supports:

- HEIF / HEIC
- JPEG
- TIFF
- RAF
- ISOBMFF video/audio such as MP4, MOV, 3GP
- Matroska family formats such as WebM, MKV, MKA

This is a signal that XIFty can unify image and media metadata under one workflow without forcing everything into a single parser.

Source:

- [nom-exif on docs.rs](https://docs.rs/crate/nom-exif/latest)

### `mp4parse`

Useful as a focused parser for ISOBMFF-family structures and because its docs explicitly reference AVIF and HEIF-related brands.

This reinforces the value of treating ISOBMFF as a foundational container abstraction rather than only as “video support”.

Source:

- [mp4parse docs](https://docs.rs/mp4parse/latest/mp4parse/)

## Testing and Quality Findings

### Fuzzing should be part of the initial plan

`cargo-fuzz` remains the standard entry point for Rust fuzzing with libFuzzer.

Given XIFty’s threat model and input surface, fuzzing should not be postponed until “later hardening”. It should begin as soon as the first container readers exist.

Source:

- [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)

### Snapshot testing fits metadata output well

`insta` is a strong fit for:

- raw JSON output
- normalized JSON output
- validation reports
- regression review of precedence or conflict-rule changes

That makes it ideal for XIFty’s differential and golden-output tests.

Source:

- [Insta snapshots](https://insta.rs/)

## Architecture Implications

The research strongly suggests these design constraints:

- Do not build one giant metadata parser.
- Keep container parsing separate from namespace interpretation.
- Make provenance and conflict reporting first-class.
- Represent capabilities explicitly by format and namespace.
- Use ExifTool as an oracle and comparison target, not as an implementation template.
- Prefer a stable C ABI over language-specific core bindings.
- Design from day one for malformed-file handling, fuzzing, and golden-output testing.

## Final Recommendations

### Use Rust for the core

This is still the best overall choice for correctness, performance, portability, and long-term maintainability.

### Publish a C ABI as the stable low-level contract

Then build:

- Python wrapper
- Node wrapper
- Swift wrapper

on top of it.

### Organize the implementation as a workspace of focused crates

The project should be modular enough that:

- new formats do not destabilize existing ones
- namespace logic is reusable across containers
- normalization can evolve independently from raw extraction
- validation and repair-related work can be added later without reshaping the entire core

### Treat validation and conflict analysis as product features, not side effects

That is one of the clearest ways for XIFty to become a genuinely better option rather than just a newer parser.
