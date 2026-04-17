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

## Current Scope

The browser MVP is intentionally narrower than the native/server surface.

Current intended browser-first formats:

- JPEG
- TIFF
- PNG
- WebP

Media-heavy formats such as HEIF and MP4/MOV remain future work for the browser
path.

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
