#!/usr/bin/env python3
"""Fixture generator for issue #137 — PNG AI-gen text-chunk decoders.

Builds five minimal PNG fixtures, one per AI-gen shape, under
``fixtures/minimal/``:

  - ``ai_gen_a1111.png``     — ``tEXt`` keyword ``parameters`` (A1111 plaintext).
  - ``ai_gen_comfyui.png``   — ``tEXt`` keys ``prompt`` + ``workflow`` (API JSON + graph JSON).
  - ``ai_gen_invokeai.png``  — ``iTXt`` keyword ``invokeai_metadata`` (JSON).
  - ``ai_gen_midjourney.png``— ``iTXt`` keyword ``Description`` (prompt + UUID).
  - ``ai_gen_fooocus.png``   — ``tEXt`` keyword ``parameters`` (Fooocus JSON).

Stdlib only (zlib, struct, binascii, json). Mirrors the layout of
``tools/gen_raw_profile_fixtures.py``. Re-run manually when the embedded
test bodies change.
"""

from __future__ import annotations

import json
import struct
import sys
import zlib
from pathlib import Path


PNG_SIG = b"\x89PNG\r\n\x1a\n"


def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


def text_chunk(keyword: str, body: bytes) -> bytes:
    """Plain `tEXt` chunk: keyword\\x00body (latin-1 only per spec, but most
    decoders accept UTF-8 bodies; matches what ComfyUI/A1111 actually emit)."""
    data = keyword.encode("latin-1") + b"\x00" + body
    return png_chunk(b"tEXt", data)


def itxt_chunk(keyword: str, body: bytes) -> bytes:
    """`iTXt` chunk (uncompressed). Layout:
    keyword \0 compression-flag \0 compression-method language-tag \0 translated \0 text"""
    data = (
        keyword.encode("latin-1")
        + b"\x00"
        + b"\x00"  # compression flag = 0 (uncompressed)
        + b"\x00"  # compression method = 0
        + b"\x00"  # empty language tag
        + b"\x00"  # empty translated keyword
        + body
    )
    return png_chunk(b"iTXt", data)


def base_png_chunks() -> tuple[bytes, bytes, bytes]:
    """Return (signature, IHDR, IDAT+IEND) for a 1x1 grayscale PNG."""
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0))
    raw_image = b"\x00\x00"  # filter byte + 1 grayscale sample
    idat = png_chunk(b"IDAT", zlib.compress(raw_image))
    iend = png_chunk(b"IEND", b"")
    return PNG_SIG + ihdr, idat, iend


def write_png(path: Path, *text_chunks: bytes) -> None:
    head, idat, iend = base_png_chunks()
    out = head
    for tc in text_chunks:
        out += tc
    out += idat + iend
    path.write_bytes(out)
    print(f"wrote {path} ({len(out)} bytes)", file=sys.stderr)


# ---------- bodies ----------

A1111_BODY = (
    "a cute cat sitting on a windowsill\n"
    "Negative prompt: blurry, lowres\n"
    'Steps: 20, Sampler: "DPM++ 2M, Karras", CFG scale: 7.0, Seed: 12345, '
    "Size: 512x768, Model hash: abc123de, Model: sd_xl_base_1.0"
).encode("utf-8")


COMFY_PROMPT = json.dumps(
    {
        "1": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {"ckpt_name": "sd_xl_base_1.0.safetensors"},
        },
        "2": {"class_type": "CLIPTextEncode", "inputs": {"text": "a cat"}},
        "3": {"class_type": "CLIPTextEncode", "inputs": {"text": "blurry"}},
        "4": {
            "class_type": "KSampler",
            "inputs": {
                "seed": 67890,
                "steps": 25,
                "cfg": 7.5,
                "sampler_name": "euler",
                "scheduler": "normal",
                "positive": ["2", 0],
                "negative": ["3", 0],
            },
        },
        "5": {
            "class_type": "LoraLoader",
            "inputs": {"lora_name": "addon.safetensors", "strength_model": 0.8},
        },
    }
).encode("utf-8")


COMFY_WORKFLOW = json.dumps(
    {"nodes": [], "links": [], "version": 0.4}
).encode("utf-8")


INVOKEAI_BODY = json.dumps(
    {
        "positive_prompt": "a serene mountain landscape",
        "negative_prompt": "people, vehicles",
        "seed": 99,
        "steps": 30,
        "cfg_scale": 7.5,
        "scheduler": "euler",
        "model": {"name": "sdxl_base", "hash": "deadbeef"},
        "width": 1024,
        "height": 1024,
        "loras": [{"name": "scenery_lora", "weight": 0.6}],
        "controlnets": [{"name": "depth", "weight": 0.4}],
    }
).encode("utf-8")


MIDJOURNEY_BODY = (
    "a cute cat --ar 16:9 --v 6 Job ID: 12345678-1234-1234-1234-1234567890ab"
).encode("utf-8")


FOOOCUS_BODY = json.dumps(
    {
        "prompt": "a forest at dawn",
        "negative_prompt": "blurry",
        "seed": 42,
        "steps": 30,
        "guidance_scale": 7.5,
        "sampler": "dpmpp_2m",
        "scheduler": "karras",
        "base_model_name": "juggernaut_xl.safetensors",
        "width": 1024,
        "height": 1024,
        "loras": [{"name": "lora_a", "weight": 0.5}],
    }
).encode("utf-8")


def main() -> int:
    repo = Path(__file__).resolve().parent.parent
    out = repo / "fixtures" / "minimal"

    write_png(out / "ai_gen_a1111.png", text_chunk("parameters", A1111_BODY))
    write_png(
        out / "ai_gen_comfyui.png",
        text_chunk("prompt", COMFY_PROMPT),
        text_chunk("workflow", COMFY_WORKFLOW),
    )
    write_png(
        out / "ai_gen_invokeai.png",
        itxt_chunk("invokeai_metadata", INVOKEAI_BODY),
    )
    write_png(
        out / "ai_gen_midjourney.png",
        itxt_chunk("Description", MIDJOURNEY_BODY),
    )
    write_png(out / "ai_gen_fooocus.png", text_chunk("parameters", FOOOCUS_BODY))
    return 0


if __name__ == "__main__":
    sys.exit(main())
