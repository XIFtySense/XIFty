#!/usr/bin/env python3

from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parent.parent / "fixtures" / "minimal"


class TiffBuilder:
    def __init__(self, endian="II"):
        self.le = endian == "II"
        self.endian = endian
        self.pack16 = (lambda v: struct.pack("<H", v)) if self.le else (lambda v: struct.pack(">H", v))
        self.pack32 = (lambda v: struct.pack("<I", v)) if self.le else (lambda v: struct.pack(">I", v))
        self.data = bytearray()

    def ascii_blob(self, text):
        return text.encode("ascii") + b"\x00"

    def add_blob(self, blob, offset_base):
        off = offset_base + len(self.data)
        self.data.extend(blob)
        if len(blob) % 2:
            self.data.extend(b"\x00")
        return off

    def ifd_bytes(self, entries, next_ifd=0):
        out = bytearray(self.pack16(len(entries)))
        for tag, typ, count, field in entries:
            out += self.pack16(tag) + self.pack16(typ) + self.pack32(count) + field
        out += self.pack32(next_ifd)
        return bytes(out)


def build_tiff(endian="II", gps=False, bad_offsets=False, no_exif=False, width=800, height=600):
    b = TiffBuilder(endian)
    p16, p32 = b.pack16, b.pack32

    make = b.ascii_blob("XIFtyCam")
    model = b.ascii_blob("IterationOne")
    software = b.ascii_blob("XIFtyTestGen")
    dto = b.ascii_blob("2024:04:16 12:34:56")
    lat_ref = b.ascii_blob("N")
    lon_ref = b.ascii_blob("W")

    ifd0_count = 6 + (0 if no_exif else 1) + (1 if gps and not no_exif else 0)
    ifd0_size = 2 + ifd0_count * 12 + 4
    data_base = 8 + ifd0_size

    make_off = b.add_blob(make, data_base)
    model_off = b.add_blob(model, data_base)
    software_off = b.add_blob(software, data_base)

    ifd0 = [
        (0x0100, 4, 1, p32(width)),
        (0x0101, 4, 1, p32(height)),
        (0x010F, 2, len(make), p32(make_off)),
        (0x0110, 2, len(model), p32(model_off)),
        (0x0112, 3, 1, p16(1) + b"\x00\x00"),
        (0x0131, 2, len(software), p32(software_off)),
    ]

    exif_ifd = b""
    gps_ifd = b""
    exif_off = 0
    gps_off = 0

    if not no_exif:
        exif_off = data_base + len(b.data)
        exif_count = 2
        exif_data_base = exif_off + 2 + exif_count * 12 + 4
        dto1_off = exif_data_base
        dto2_off = exif_data_base + len(dto) + (len(dto) % 2)
        exif_entries = [
            (0x9003, 2, len(dto), p32(dto1_off)),
            (0x9004, 2, len(dto), p32(dto2_off)),
        ]
        exif_ifd = bytearray(b.ifd_bytes(exif_entries))
        exif_ifd += dto
        if len(dto) % 2:
            exif_ifd += b"\x00"
        exif_ifd += dto
        if len(dto) % 2:
            exif_ifd += b"\x00"

        if gps:
            gps_off = exif_off + len(exif_ifd)
            gps_count = 4
            gps_data_base = gps_off + 2 + gps_count * 12 + 4

            def rats(vals):
                raw = bytearray()
                for n, d in vals:
                    raw += p32(n) + p32(d)
                return bytes(raw)

            lat = rats([(40, 1), (26, 1), (4632, 100)])
            lon = rats([(79, 1), (58, 1), (5556, 100)])
            lat_off = gps_data_base
            lon_off = gps_data_base + len(lat)
            gps_entries = [
                (0x0001, 2, len(lat_ref), lat_ref + b"\x00" * (4 - len(lat_ref))),
                (0x0002, 5, 3, p32(lat_off)),
                (0x0003, 2, len(lon_ref), lon_ref + b"\x00" * (4 - len(lon_ref))),
                (0x0004, 5, 3, p32(lon_off)),
            ]
            gps_ifd = bytearray(b.ifd_bytes(gps_entries))
            gps_ifd += lat + lon

        ifd0.append((0x8769, 4, 1, p32(999999 if bad_offsets else exif_off)))
        if gps:
            ifd0.append((0x8825, 4, 1, p32(888888 if bad_offsets else gps_off)))

    header = endian.encode("ascii") + (b"*\x00" if b.le else b"\x00*") + p32(8)
    out = bytearray(header)
    out += b.ifd_bytes(ifd0)
    out += b.data
    out += exif_ifd
    out += gps_ifd
    return bytes(out)


def build_jpeg(exif_payload=None, malformed=False):
    out = bytearray(b"\xFF\xD8")
    if exif_payload is not None:
        payload = b"Exif\x00\x00" + exif_payload
        if malformed:
            out += b"\xFF\xE1\x00\x40" + payload[:6]
        else:
            out += b"\xFF\xE1" + struct.pack(">H", len(payload) + 2) + payload
    out += b"\xFF\xD9"
    return bytes(out)


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    files = {
        "happy.jpg": build_jpeg(build_tiff(gps=False)),
        "gps.jpg": build_jpeg(build_tiff(gps=True)),
        "no_exif.jpg": build_jpeg(None),
        "malformed_app1.jpg": build_jpeg(build_tiff(gps=False), malformed=True),
        "happy.tiff": build_tiff(gps=False),
        "gps.tiff": build_tiff(gps=True),
        "big_endian.tiff": build_tiff(endian="MM", gps=False, width=1024, height=768),
        "malformed_offsets.tiff": build_tiff(gps=True, bad_offsets=True),
        "no_exif.tiff": build_tiff(no_exif=True),
    }

    for name, data in files.items():
        (ROOT / name).write_bytes(data)


if __name__ == "__main__":
    main()
