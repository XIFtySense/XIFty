#!/usr/bin/env python3
"""Fixture generator for issue #132 — C2PA JUMBF manifest decoder.

Builds a minimal `caBX`-bearing PNG under ``fixtures/minimal/c2pa_synthetic.png``
plus a sidecar fixture pair under ``fixtures/minimal/c2pa_sidecar/``.

Stdlib only (zlib, struct, binascii). Mirrors the layout of
``tools/gen_ai_gen_fixtures.py`` and ``tools/gen_raw_profile_fixtures.py``.

The on-disk box hierarchy follows the C2PA 2.0 "Box Type Identifiers" table:

    jumb (top-level C2PA superbox, UUID prefix b'c2pa')
    +-- jumd (UUID = c2pa, label "c2pa")
    +-- jumb (manifest, UUID prefix b'c2ma')
        +-- jumd (UUID = c2ma, label "urn:uuid:xifty-test-manifest-0001")
        +-- jumb (claim, UUID prefix b'c2cl')
        |   +-- jumd (UUID = c2cl, label "c2pa.claim.v2")
        |   +-- cbor: { claim_generator, format, instance_id, assertions }
        +-- jumb (assertion store, UUID prefix b'c2as')
        |   +-- jumd (UUID = c2as, label "c2pa.assertions")
        |   +-- jumb (one assertion)
        |       +-- jumd (UUID = cbor, label "c2pa.actions.v2")
        |       +-- cbor: { actions: [ { action, digitalSourceType, ... } ] }
        +-- jumb (signature, UUID prefix b'c2cs')
            +-- jumd (UUID = c2cs, label "c2pa.signature")
            +-- cbor: COSE_Sign1 (CBOR tag 18) array
                       [ <<{1: -8}>>, {}, h'', h'00 * 64' ]
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path


PNG_SIG = b"\x89PNG\r\n\x1a\n"

# JUMBF base UUID tail per ISO 19566-5 / C2PA 2.0.
JUMBF_BASE_TAIL = bytes.fromhex("00110010800000aa00389b71")


def c2pa_uuid(label: bytes) -> bytes:
    assert len(label) == 4
    return label + JUMBF_BASE_TAIL


UUID_C2PA = c2pa_uuid(b"c2pa")
UUID_C2MA = c2pa_uuid(b"c2ma")
UUID_C2CL = c2pa_uuid(b"c2cl")
UUID_C2AS = c2pa_uuid(b"c2as")
UUID_C2CS = c2pa_uuid(b"c2cs")
UUID_CBOR = c2pa_uuid(b"cbor")


# ---------- minimal CBOR encoder (RFC 8949 subset) ----------

def cbor_int(n: int) -> bytes:
    if n >= 0:
        major = 0
    else:
        major = 1
        n = -1 - n
    if n < 24:
        return bytes([(major << 5) | n])
    if n < 0x100:
        return bytes([(major << 5) | 24, n])
    if n < 0x10000:
        return bytes([(major << 5) | 25]) + struct.pack(">H", n)
    if n < 0x1_0000_0000:
        return bytes([(major << 5) | 26]) + struct.pack(">I", n)
    return bytes([(major << 5) | 27]) + struct.pack(">Q", n)


def cbor_head(major: int, n: int) -> bytes:
    if n < 24:
        return bytes([(major << 5) | n])
    if n < 0x100:
        return bytes([(major << 5) | 24, n])
    if n < 0x10000:
        return bytes([(major << 5) | 25]) + struct.pack(">H", n)
    if n < 0x1_0000_0000:
        return bytes([(major << 5) | 26]) + struct.pack(">I", n)
    return bytes([(major << 5) | 27]) + struct.pack(">Q", n)


def cbor_bytes(b: bytes) -> bytes:
    return cbor_head(2, len(b)) + b


def cbor_text(s: str) -> bytes:
    body = s.encode("utf-8")
    return cbor_head(3, len(body)) + body


def cbor_array(items: list[bytes]) -> bytes:
    return cbor_head(4, len(items)) + b"".join(items)


def cbor_map(pairs: list[tuple[bytes, bytes]]) -> bytes:
    out = cbor_head(5, len(pairs))
    for k, v in pairs:
        out += k + v
    return out


def cbor_tag(tag: int, body: bytes) -> bytes:
    return cbor_head(6, tag) + body


def cbor_encode(value) -> bytes:
    if isinstance(value, bool):
        return bytes([0xF5 if value else 0xF4])
    if isinstance(value, int):
        return cbor_int(value)
    if isinstance(value, str):
        return cbor_text(value)
    if isinstance(value, bytes):
        return cbor_bytes(value)
    if isinstance(value, list):
        return cbor_array([cbor_encode(v) for v in value])
    if isinstance(value, dict):
        return cbor_map([(cbor_encode(k), cbor_encode(v)) for k, v in value.items()])
    raise TypeError(f"unsupported cbor type: {type(value)}")


# ---------- JUMBF box helpers ----------

def jumbf_box(tbox: bytes, payload: bytes) -> bytes:
    """LBox (4-byte BE total length incl. header) + TBox (4 bytes) + payload."""
    assert len(tbox) == 4
    total = 8 + len(payload)
    return struct.pack(">I", total) + tbox + payload


def jumd_box(uuid: bytes, label: str) -> bytes:
    """Description box: 16-byte UUID + 1-byte toggles (0x03 = requestable+label)
    + null-terminated UTF-8 label."""
    assert len(uuid) == 16
    payload = uuid + bytes([0x03]) + label.encode("utf-8") + b"\x00"
    return jumbf_box(b"jumd", payload)


def cbor_data_box(payload: bytes) -> bytes:
    return jumbf_box(b"cbor", payload)


def jumb_super(uuid: bytes, label: str, *children: bytes) -> bytes:
    body = jumd_box(uuid, label) + b"".join(children)
    return jumbf_box(b"jumb", body)


# ---------- C2PA manifest ----------

def build_claim_cbor() -> bytes:
    return cbor_encode(
        {
            "claim_generator": "XIFty Test Suite/0.1.0",
            "format": "image/png",
            "instance_id": "urn:uuid:00000000-0000-0000-0000-000000000001",
            "assertions": [
                {"url": "self#jumbf=c2pa.assertions/c2pa.actions.v2"},
            ],
        }
    )


def build_assertion_cbor() -> bytes:
    return cbor_encode(
        {
            "actions": [
                {
                    "action": "c2pa.created",
                    "digitalSourceType": "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
                    "softwareAgent": "XIFty Test Suite/0.1.0",
                    "when": "2026-01-01T00:00:00Z",
                    "description": "Created by XIFty Test Suite for synthetic fixture",
                }
            ]
        }
    )


def build_signature_cbor() -> bytes:
    """Synthetic COSE_Sign1: CBOR tag 18 wrapping
        [ bstr(<<{1: -8}>>), {}, h'', h'00 * 64' ]
    Per RFC 9053, alg=-8 is EdDSA. The signature bytes are zeroed; level A
    parsers do NOT verify."""
    protected = cbor_encode({1: -8})
    array_body = cbor_array(
        [
            cbor_bytes(protected),
            cbor_map([]),
            cbor_bytes(b""),
            cbor_bytes(b"\x00" * 64),
        ]
    )
    return cbor_tag(18, array_body)


def build_jumbf_manifest() -> bytes:
    claim_box = jumb_super(
        UUID_C2CL,
        "c2pa.claim.v2",
        cbor_data_box(build_claim_cbor()),
    )
    assertion_inner = jumb_super(
        UUID_CBOR,
        "c2pa.actions.v2",
        cbor_data_box(build_assertion_cbor()),
    )
    assertion_store = jumb_super(
        UUID_C2AS,
        "c2pa.assertions",
        assertion_inner,
    )
    signature_box = jumb_super(
        UUID_C2CS,
        "c2pa.signature",
        cbor_data_box(build_signature_cbor()),
    )
    manifest = jumb_super(
        UUID_C2MA,
        "urn:uuid:xifty-test-manifest-0001",
        claim_box,
        assertion_store,
        signature_box,
    )
    top = jumb_super(UUID_C2PA, "c2pa", manifest)
    return top


# ---------- PNG packaging ----------

def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


def build_synthetic_png(jumbf_bytes: bytes) -> bytes:
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0))
    cabx = png_chunk(b"caBX", jumbf_bytes)
    raw_image = b"\x00\x00"  # filter byte + 1 grayscale sample
    idat = png_chunk(b"IDAT", zlib.compress(raw_image))
    iend = png_chunk(b"IEND", b"")
    return PNG_SIG + ihdr + cabx + idat + iend


def main() -> None:
    here = Path(__file__).resolve().parent.parent
    out_dir = here / "fixtures" / "minimal"
    sidecar_dir = out_dir / "c2pa_sidecar"
    out_dir.mkdir(parents=True, exist_ok=True)
    sidecar_dir.mkdir(parents=True, exist_ok=True)

    jumbf_bytes = build_jumbf_manifest()
    png_bytes = build_synthetic_png(jumbf_bytes)
    png_path = out_dir / "c2pa_synthetic.png"
    png_path.write_bytes(png_bytes)
    print(f"wrote {png_path} ({len(png_bytes)} bytes)", file=sys.stderr)

    # Sidecar pair: a primary stub + an external `.c2pa` manifest.
    primary = sidecar_dir / "clip.jpg"
    primary.write_bytes(b"\xff\xd8\xff\xd9")  # minimal valid JPEG (SOI+EOI)
    print(f"wrote {primary} ({primary.stat().st_size} bytes)", file=sys.stderr)

    sidecar_path = sidecar_dir / "clip.c2pa"
    sidecar_path.write_bytes(jumbf_bytes)
    print(f"wrote {sidecar_path} ({len(jumbf_bytes)} bytes)", file=sys.stderr)


if __name__ == "__main__":
    main()
