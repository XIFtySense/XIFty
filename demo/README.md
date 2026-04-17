# XIFty Web Demo

This directory holds the browser demo for XIFty.

The demo is intentionally static-first:

- files are processed locally in the browser
- no upload backend is required
- the UI is built on top of the `xifty-wasm` crate

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
