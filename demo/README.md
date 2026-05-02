# XIFty Web Demo

This directory holds the browser demo for XIFty.

The demo is intentionally static-first:

- files are processed locally in the browser
- no upload backend is required
- the UI is built on top of the `xifty-wasm` crate

The current demo is designed to be readable before it is exhaustive:

- a structured normalized "facts" view
- grouped inventories for complete normalized, interpreted, and raw metadata
- explicit issue/conflict presentation
- readable timestamps and GPS when present
- JSON copy/export for the exact envelope behind the UI

## Local Build

Build the WASM package and browser assets:

```bash
./tools/build-web-demo.sh
```

Then serve the `demo/web/` directory with any simple static server, for example:

```bash
cd demo/web
python3 -m http.server 4173
```

Open <http://localhost:4173>.

## Smoke Test

A headless browser smoke test lives in `demo/web/smoketest/`. It drives
`index.html` with Playwright, feeds `fixtures/minimal/happy.jpg` through the
real file input, and validates the resulting analysis envelope against
`schemas/xifty-analysis-0.1.0.schema.json`.

The test opts into an instrumentation hook gated by the `?smoketest=1` query
flag. When present, the demo assigns `window.__xiftyDebug = { probe, views }`
after a successful extraction. The flag has no effect for ordinary Pages
visitors.

Run it locally after building the WASM bundle:

```bash
./tools/build-web-demo.sh
cd demo/web/smoketest
npm ci
npx playwright install --with-deps chromium
npm test
```

CI runs the same test on pull requests via `.github/workflows/pages-demo.yml`.

## Current Scope

The browser demo runs the full `xifty-wasm` surface — every container the
native Rust crates support is available in the browser. Practical limits are
file-size / browser-memory rather than format coverage.

Currently exercised end-to-end in the demo:

- **Still images:** JPEG, TIFF, DNG, PNG, WebP, GIF, HEIF/HEIC, AVIF
- **Camera RAW:** Canon CR2 / CR3, Nikon NEF, Sony ARW, Fuji RAF, Olympus ORF, Panasonic RW2
- **Video:** MP4, MOV (including Sony FX/A7-series PROF/USMT video metadata)
- **Audio:** MP3 (ID3v2), WAV (BWF + iXML), M4A, FLAC, OGG (Vorbis / Opus), AIFF / AIFC

Large RAW and long-form video files may stress browser memory; for production
batch processing prefer the Node binding on Lambda or the native CLI.

## Current Presentation Model

The browser demo uses the same XIFty four-view model as the rest of the
project:

- `normalized`
- `interpreted`
- `raw`
- `report`

But it does not present every view as a plain JSON dump anymore.

The current UX aims to show:

- the stable application-facing fields first
- the complete available metadata inventory in grouped sections
- the underlying JSON only when explicitly copied or inspected
