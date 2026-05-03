#!/usr/bin/env python3
"""One-off generator for issue #136 fixtures.

Builds `fixtures/minimal/raw_profile_app1.png` — a minimal PNG that wraps
the EXIF APP1 segment from `fixtures/minimal/happy.jpg` inside a `zTXt`
chunk keyed `Raw profile type APP1`. Mirrors what ImageMagick / libvips
produce when transcoding a JPEG with embedded EXIF to PNG.

Stdlib only (zlib, struct, binascii). Not wired into CI; rerun manually
when `happy.jpg` changes.
"""

from __future__ import annotations

import binascii
import struct
import sys
import zlib
from pathlib import Path


def find_app1_exif(jpeg: bytes) -> bytes:
    """Return the APP1 segment payload (`Exif\\0\\0` + TIFF stream)."""
    if jpeg[:2] != b"\xFF\xD8":
        raise SystemExit("not a JPEG")
    i = 2
    while i + 4 <= len(jpeg):
        if jpeg[i] != 0xFF:
            raise SystemExit(f"unexpected byte at {i}: {jpeg[i]:#x}")
        marker = jpeg[i + 1]
        if marker == 0xD8:
            i += 2
            continue
        if marker == 0xDA:  # SOS — image data follows; stop scanning.
            break
        seg_len = struct.unpack(">H", jpeg[i + 2 : i + 4])[0]
        payload = jpeg[i + 4 : i + 2 + seg_len]
        if marker == 0xE1 and payload.startswith(b"Exif\x00\x00"):
            return payload
        i += 2 + seg_len
    raise SystemExit("no APP1/EXIF segment found in JPEG")


def imagemagick_frame(name: bytes, raw: bytes) -> bytes:
    """ImageMagick raw-profile framing: \\n<name>\\n<len>\\n<hex>\\n."""
    hex_chars = binascii.hexlify(raw)
    out = bytearray()
    out.append(0x0A)  # leading newline
    out.extend(name)
    out.append(0x0A)
    out.extend(str(len(raw)).encode("ascii"))
    # Wrap hex at 72 chars per line for readability.
    for i in range(0, len(hex_chars), 72):
        out.append(0x0A)
        out.extend(hex_chars[i : i + 72])
    out.append(0x0A)
    return bytes(out)


def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


def build_png(raw_profile_app1: bytes) -> bytes:
    sig = b"\x89PNG\r\n\x1a\n"
    # 1x1 grayscale.
    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0)

    # zTXt: keyword + nul + compression-method (0) + zlib(text).
    keyword = b"Raw profile type APP1"
    body = imagemagick_frame(b"APP1", raw_profile_app1)
    ztxt_data = keyword + b"\x00" + b"\x00" + zlib.compress(body)

    # Single-pixel IDAT.
    raw_image = b"\x00\x00"  # filter byte + 1 grayscale sample
    idat_data = zlib.compress(raw_image)

    return (
        sig
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"zTXt", ztxt_data)
        + png_chunk(b"IDAT", idat_data)
        + png_chunk(b"IEND", b"")
    )


def main() -> int:
    repo = Path(__file__).resolve().parent.parent
    src = repo / "fixtures" / "minimal" / "happy.jpg"
    dst = repo / "fixtures" / "minimal" / "raw_profile_app1.png"

    jpeg_bytes = src.read_bytes()
    app1 = find_app1_exif(jpeg_bytes)
    print(f"happy.jpg APP1/EXIF payload: {len(app1)} bytes", file=sys.stderr)

    png_bytes = build_png(app1)
    dst.write_bytes(png_bytes)
    print(f"wrote {dst} ({len(png_bytes)} bytes)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
