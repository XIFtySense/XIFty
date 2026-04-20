#!/usr/bin/env python3

from pathlib import Path
import struct
import zlib
from datetime import datetime, timezone


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


def build_tiff(
    endian="II",
    gps=False,
    bad_offsets=False,
    no_exif=False,
    width=800,
    height=600,
    xmp_payload=None,
    icc_payload=None,
    iptc_payload=None,
    make="XIFtyCam",
):
    b = TiffBuilder(endian)
    p16, p32 = b.pack16, b.pack32

    make = b.ascii_blob(make)
    model = b.ascii_blob("IterationOne")
    software = b.ascii_blob("XIFtyTestGen")
    dto = b.ascii_blob("2024:04:16 12:34:56")
    lens_make = b.ascii_blob("XIFty Optics")
    lens_model = b.ascii_blob("XIFty 50mm F2")
    lat_ref = b.ascii_blob("N")
    lon_ref = b.ascii_blob("W")

    extra_count = sum(
        1 for blob in (xmp_payload, icc_payload, iptc_payload) if blob is not None
    )
    ifd0_count = 6 + extra_count + (0 if no_exif else 1) + (1 if gps and not no_exif else 0)
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

    if xmp_payload is not None:
        xmp_off = b.add_blob(xmp_payload, data_base)
        ifd0.append((0x02BC, 7, len(xmp_payload), p32(xmp_off)))
    if icc_payload is not None:
        icc_off = b.add_blob(icc_payload, data_base)
        ifd0.append((0x8773, 7, len(icc_payload), p32(icc_off)))
    if iptc_payload is not None:
        iptc_off = b.add_blob(iptc_payload, data_base)
        ifd0.append((0x83BB, 7, len(iptc_payload), p32(iptc_off)))

    exif_ifd = b""
    gps_ifd = b""
    exif_off = 0
    gps_off = 0

    if not no_exif:
        exif_off = data_base + len(b.data)
        exposure_time = p32(1) + p32(250)
        f_number = p32(56) + p32(10)
        focal_length = p32(50) + p32(1)
        exif_count = 8
        exif_data_base = exif_off + 2 + exif_count * 12 + 4
        dto1_off = exif_data_base
        dto2_off = exif_data_base + len(dto) + (len(dto) % 2)
        lens_make_off = dto2_off + len(dto) + (len(dto) % 2)
        lens_model_off = lens_make_off + len(lens_make) + (len(lens_make) % 2)
        exposure_time_off = lens_model_off + len(lens_model) + (len(lens_model) % 2)
        f_number_off = exposure_time_off + len(exposure_time)
        focal_length_off = f_number_off + len(f_number)
        exif_entries = [
            (0x9003, 2, len(dto), p32(dto1_off)),
            (0x9004, 2, len(dto), p32(dto2_off)),
            (0x829A, 5, 1, p32(exposure_time_off)),
            (0x829D, 5, 1, p32(f_number_off)),
            (0x8827, 3, 1, p16(200) + b"\x00\x00"),
            (0x920A, 5, 1, p32(focal_length_off)),
            (0xA433, 2, len(lens_make), p32(lens_make_off)),
            (0xA434, 2, len(lens_model), p32(lens_model_off)),
        ]
        exif_ifd = bytearray(b.ifd_bytes(exif_entries))
        exif_ifd += dto
        if len(dto) % 2:
            exif_ifd += b"\x00"
        exif_ifd += dto
        if len(dto) % 2:
            exif_ifd += b"\x00"
        exif_ifd += lens_make
        if len(lens_make) % 2:
            exif_ifd += b"\x00"
        exif_ifd += lens_model
        if len(lens_model) % 2:
            exif_ifd += b"\x00"
        exif_ifd += exposure_time
        exif_ifd += f_number
        exif_ifd += focal_length

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


def build_jpeg_with_metadata(exif_payload=None, icc_payload=None, iptc_payload=None, malformed=False):
    out = bytearray(b"\xFF\xD8")
    if exif_payload is not None:
        payload = b"Exif\x00\x00" + exif_payload
        if malformed:
            out += b"\xFF\xE1\x00\x40" + payload[:6]
        else:
            out += b"\xFF\xE1" + struct.pack(">H", len(payload) + 2) + payload
    if icc_payload is not None:
        payload = b"ICC_PROFILE\x00" + bytes([1, 1]) + icc_payload
        out += b"\xFF\xE2" + struct.pack(">H", len(payload) + 2) + payload
    if iptc_payload is not None:
        out += b"\xFF\xED" + struct.pack(">H", len(iptc_payload) + 2) + iptc_payload
    out += b"\xFF\xD9"
    return bytes(out)


def build_jpeg_with_xmp_and_iptc(xmp_payload, iptc_payload):
    out = bytearray(b"\xFF\xD8")
    xmp_packet = b"http://ns.adobe.com/xap/1.0/\x00" + xmp_payload
    out += b"\xFF\xE1" + struct.pack(">H", len(xmp_packet) + 2) + xmp_packet
    out += b"\xFF\xED" + struct.pack(">H", len(iptc_payload) + 2) + iptc_payload
    out += b"\xFF\xD9"
    return bytes(out)


def build_malformed_iptc_app13():
    resource = bytearray()
    resource += b"8BIM"
    resource += struct.pack(">H", 0x0404)
    resource += b"\x00"
    if len(resource) % 2:
        resource += b"\x00"
    resource += struct.pack(">I", 32)
    resource += b"\x1C\x02\x69\x00\x08Head"
    return b"Photoshop 3.0\x00" + bytes(resource)


def png_chunk(chunk_type, data):
    return struct.pack(">I", len(data)) + chunk_type + data + b"\x00\x00\x00\x00"


def build_icc_profile(name="XIFty Display Profile", color_space=b"RGB ", profile_class=b"mntr"):
    header = bytearray(128)
    header[12:16] = profile_class
    header[16:20] = color_space
    header[20:24] = b"XYZ "
    header[48:52] = b"XFTY"
    header[52:56] = b"TEST"

    desc_text = name.encode("ascii") + b"\x00"
    desc = bytearray()
    desc += b"desc" + b"\x00\x00\x00\x00"
    desc += struct.pack(">I", len(desc_text))
    desc += desc_text
    while len(desc) % 4:
        desc += b"\x00"

    profile = bytearray(header)
    profile += struct.pack(">I", 1)
    profile += b"desc"
    profile += struct.pack(">I", 144)
    profile += struct.pack(">I", len(desc))
    profile += desc
    profile[0:4] = struct.pack(">I", len(profile))
    return bytes(profile)


def build_iptc_iim(*, headline="XIFty Headline", description="XIFty Caption", keywords=("xifty", "metadata"), author="Kai", copyright_text="XIFty"):
    out = bytearray()
    fields = [
        (2, 105, headline),
        (2, 120, description),
        (2, 80, author),
        (2, 116, copyright_text),
    ]
    for keyword in keywords:
        fields.append((2, 25, keyword))
    for record, dataset, text in fields:
        data = text.encode("utf-8")
        out += bytes([0x1C, record, dataset]) + struct.pack(">H", len(data)) + data
    return bytes(out)


def build_photoshop_iptc_app13(iptc_bytes):
    resource = bytearray()
    resource += b"8BIM"
    resource += struct.pack(">H", 0x0404)
    resource += b"\x00"
    if len(resource) % 2:
        resource += b"\x00"
    resource += struct.pack(">I", len(iptc_bytes))
    resource += iptc_bytes
    if len(iptc_bytes) % 2:
        resource += b"\x00"
    return b"Photoshop 3.0\x00" + bytes(resource)


def build_xmp(
    *,
    make="XIFtyCam",
    model="IterationTwo",
    create_date="2024-04-16T12:34:56",
    modify_date="2024-04-16T13:00:00",
    width=640,
    height=480,
    author="K",
    creator_tool="XIFtyXmpGen",
    copyright_text="XIFty",
    gps_latitude=None,
    gps_longitude=None,
):
    gps_attrs = ""
    if gps_latitude is not None:
        gps_attrs += f'\n  exif:GPSLatitude="{gps_latitude}"'
    if gps_longitude is not None:
        gps_attrs += f'\n  exif:GPSLongitude="{gps_longitude}"'
    return f"""<x:xmpmeta>
<rdf:Description
  xmlns:xmp="adobe:ns:meta/"
  xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
  xmlns:exif="http://ns.adobe.com/exif/1.0/"
  xmlns:dc="http://purl.org/dc/elements/1.1/"
  xmp:CreateDate="{create_date}"
  xmp:ModifyDate="{modify_date}"
  xmp:CreatorTool="{creator_tool}"
  tiff:Make="{make}"
  tiff:Model="{model}"
  tiff:ImageWidth="{width}"
  tiff:ImageLength="{height}"
  tiff:Orientation="1"{gps_attrs}>
</rdf:Description>
<dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>
<dc:rights><rdf:Alt><rdf:li>{copyright_text}</rdf:li></rdf:Alt></dc:rights>
</x:xmpmeta>""".encode("utf-8")


def build_editorial_xmp(
    *,
    author,
    copyright_text,
    headline,
    description,
    make="XIFtyCam",
    model="IterationTwo",
):
    return f"""<x:xmpmeta>
<rdf:Description
  xmlns:xmp="adobe:ns:meta/"
  xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
  xmlns:exif="http://ns.adobe.com/exif/1.0/"
  xmlns:dc="http://purl.org/dc/elements/1.1/"
  xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
  xmp:CreateDate="2024-04-16T12:34:56"
  xmp:ModifyDate="2024-04-16T13:00:00"
  xmp:CreatorTool="XIFtyXmpGen"
  tiff:Make="{make}"
  tiff:Model="{model}"
  tiff:ImageWidth="640"
  tiff:ImageLength="480"
  tiff:Orientation="1"
  photoshop:Headline="{headline}">
</rdf:Description>
<dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>
<dc:rights><rdf:Alt><rdf:li>{copyright_text}</rdf:li></rdf:Alt></dc:rights>
<dc:description><rdf:Alt><rdf:li>{description}</rdf:li></rdf:Alt></dc:description>
</x:xmpmeta>""".encode("utf-8")


def build_png(exif_payload=None, xmp_payload=None, malformed=False):
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    chunks = [ihdr]
    if exif_payload is not None:
        if malformed:
            chunks.append(struct.pack(">I", 128) + b"eXIf" + exif_payload[:4] + b"\x00\x00\x00\x00")
        else:
            chunks.append(png_chunk(b"eXIf", exif_payload))
    if xmp_payload is not None:
        text_data = b"XML:com.adobe.xmp\x00\x00\x00\x00\x00" + xmp_payload
        chunks.append(png_chunk(b"iTXt", text_data))
    chunks.append(png_chunk(b"IEND", b""))
    return signature + b"".join(chunks)


def build_png_with_iptc(iptc_payload):
    """Build a minimal PNG carrying IPTC metadata in an ImageMagick-style
    ``Raw profile type iptc`` zTXt chunk (hex-encoded IIM, zlib-compressed)."""
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    hex_lines = []
    hex_str = iptc_payload.hex()
    for i in range(0, len(hex_str), 72):
        hex_lines.append(hex_str[i : i + 72])
    framing = ("\niptc\n" + f"{len(iptc_payload):>8}\n" + "\n".join(hex_lines) + "\n").encode("ascii")
    keyword = b"Raw profile type iptc\x00"
    compression_method = b"\x00"
    ztxt_payload = keyword + compression_method + zlib.compress(framing)
    ztxt = png_chunk(b"zTXt", ztxt_payload)
    return signature + ihdr + ztxt + png_chunk(b"IEND", b"")


def build_png_with_icc(icc_payload):
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    iccp_payload = b"XIFty ICC\x00\x00" + zlib.compress(icc_payload)
    iccp = png_chunk(b"iCCP", iccp_payload)
    return signature + ihdr + iccp + png_chunk(b"IEND", b"")


def build_png_with_malformed_icc():
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    iccp_payload = b"XIFty ICC\x00\x00" + b"not-zlib"
    iccp = png_chunk(b"iCCP", iccp_payload)
    return signature + ihdr + iccp + png_chunk(b"IEND", b"")


def riff_chunk(chunk_id, data):
    chunk = chunk_id + struct.pack("<I", len(data)) + data
    if len(data) % 2:
        chunk += b"\x00"
    return chunk


def build_webp(exif_payload=None, xmp_payload=None, malformed=False):
    chunks = []
    if exif_payload is not None:
        if malformed:
            chunks.append(b"EXIF" + struct.pack("<I", 64) + exif_payload[:4])
        else:
            chunks.append(riff_chunk(b"EXIF", exif_payload))
    if xmp_payload is not None:
        chunks.append(riff_chunk(b"XMP ", xmp_payload))
    body = b"WEBP" + b"".join(chunks)
    declared_size = len(body)
    if malformed:
        declared_size += 9
    return b"RIFF" + struct.pack("<I", declared_size) + body


def build_webp_with_icc(icc_payload):
    body = b"WEBP" + riff_chunk(b"ICCP", icc_payload)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def build_webp_with_iptc(iptc_payload):
    body = b"WEBP" + riff_chunk(b"IPTC", iptc_payload)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def iso_box(box_type, data, *, force_size=None):
    size = force_size if force_size is not None else 8 + len(data)
    return struct.pack(">I", size) + box_type + data


def full_box(box_type, data, version=0, flags=0):
    header = bytes([version]) + flags.to_bytes(3, "big")
    return iso_box(box_type, header + data)


def build_heif(exif_payload=None, xmp_payload=None, malformed=False, unsupported=False):
    ftyp_payload = b"heic" + b"\x00\x00\x00\x00" + b"mif1" + b"heic"
    children = []
    if exif_payload is not None:
        children.append(iso_box(b"Exif", exif_payload))
    if xmp_payload is not None:
        children.append(iso_box(b"mime", b"application/rdf+xml\x00" + xmp_payload))
    if unsupported:
        children.append(full_box(b"iloc", b"\x00\x00\x00\x00"))
    meta = full_box(b"meta", b"".join(children))
    if malformed:
        meta = iso_box(b"meta", meta[8:], force_size=len(meta) + 32)
    return iso_box(b"ftyp", ftyp_payload) + meta


def qt_epoch_seconds(year, month, day, hour=0, minute=0, second=0):
    unix = datetime(year, month, day, hour, minute, second, tzinfo=timezone.utc).timestamp()
    return int(unix + 2082844800)


def quicktime_data_box(text):
    payload = b"\x00\x00\x00\x01" + b"\x00\x00\x00\x00" + text.encode("utf-8") + b"\x00"
    return iso_box(b"data", payload)


def build_video_sample_entry(codec, *, bitrate):
    sample = bytearray(b"\x00" * 6 + struct.pack(">H", 1))
    sample += b"\x00" * 16
    sample += struct.pack(">H", 1920)
    sample += struct.pack(">H", 1080)
    sample += struct.pack(">I", 0x00480000)
    sample += struct.pack(">I", 0x00480000)
    sample += b"\x00" * 4
    sample += struct.pack(">H", 1)
    sample += b"\x00" * 32
    sample += struct.pack(">H", 0x0018)
    sample += struct.pack(">H", 0xFFFF)
    sample += iso_box(b"btrt", struct.pack(">III", bitrate * 2, bitrate * 2, bitrate))
    return iso_box(codec, bytes(sample))


def build_audio_sample_entry(codec, *, channels, sample_rate):
    sample = bytearray(b"\x00" * 6 + struct.pack(">H", 1))
    sample += b"\x00" * 8
    sample += struct.pack(">H", channels)
    sample += struct.pack(">H", 16)
    sample += struct.pack(">H", 0)
    sample += struct.pack(">H", 0)
    sample += struct.pack(">I", sample_rate << 16)
    return iso_box(codec, bytes(sample))


def build_track(
    *,
    handler,
    codec,
    duration,
    timescale=1000,
    width=0,
    height=0,
    frame_rate=None,
    bitrate=None,
    channels=None,
    sample_rate=None,
):
    tkhd_payload = b"\x00" * 72 + struct.pack(">II", width << 16, height << 16)
    tkhd = full_box(b"tkhd", tkhd_payload)
    mdhd_payload = (
        struct.pack(">I", qt_epoch_seconds(2024, 4, 16, 12, 34, 56))
        + struct.pack(">I", qt_epoch_seconds(2024, 4, 16, 13, 0, 0))
        + struct.pack(">I", timescale)
        + struct.pack(">I", duration)
        + b"\x00\x00\x00\x00"
    )
    mdhd = full_box(b"mdhd", mdhd_payload)
    hdlr_payload = b"\x00\x00\x00\x00" + handler + b"\x00" * 12
    hdlr = full_box(b"hdlr", hdlr_payload)
    if handler == b"vide":
        sample_entry = build_video_sample_entry(codec, bitrate=bitrate or 24_000_000)
    else:
        sample_entry = build_audio_sample_entry(
            codec,
            channels=channels or 2,
            sample_rate=sample_rate or timescale,
        )
    stsd = full_box(b"stsd", struct.pack(">I", 1) + sample_entry)
    stts_payload = struct.pack(">I", 1)
    if handler == b"vide" and frame_rate:
        sample_delta = round(timescale / frame_rate)
        sample_count = round(duration / sample_delta)
        stts_payload += struct.pack(">II", sample_count, sample_delta)
    else:
        stts_payload += struct.pack(">II", 1, duration)
    stts = full_box(b"stts", stts_payload)
    stbl = iso_box(b"stbl", stsd + stts)
    minf = iso_box(b"minf", stbl)
    mdia = iso_box(b"mdia", mdhd + hdlr + minf)
    return iso_box(b"trak", tkhd + mdia)


def build_media_file(
    *,
    major_brand,
    compatible_brand,
    author="Kai",
    software="XIFtyMediaGen",
    duration=12.0,
    malformed=False,
    include_audio=True,
    include_metadata=True,
    include_movie=True,
    unsupported=False,
):
    timescale = 1000
    movie_duration = int(duration * timescale)
    if not include_movie:
        return iso_box(b"ftyp", major_brand + b"\x00\x00\x00\x00" + compatible_brand)
    mvhd_payload = (
        struct.pack(">I", qt_epoch_seconds(2024, 4, 16, 12, 34, 56))
        + struct.pack(">I", qt_epoch_seconds(2024, 4, 16, 13, 0, 0))
        + struct.pack(">I", timescale)
        + struct.pack(">I", movie_duration)
        + b"\x00" * 8
    )
    mvhd = full_box(b"mvhd", mvhd_payload)
    video_timescale = 24000
    video_duration = int(duration * video_timescale)
    video_track = build_track(
        handler=b"vide",
        codec=b"avc1",
        duration=video_duration,
        timescale=video_timescale,
        width=1920,
        height=1080,
        frame_rate=23.976,
        bitrate=24_000_000,
    )
    tracks = [video_track]
    if include_audio:
        audio_timescale = 48000
        audio_duration = int(duration * audio_timescale)
        tracks.append(
            build_track(
                handler=b"soun",
                codec=b"mp4a",
                duration=audio_duration,
                timescale=audio_timescale,
                channels=2,
                sample_rate=48000,
            )
        )
    extras = []
    if include_metadata:
        ilst = iso_box(
            b"ilst",
            iso_box(b"\xa9ART", quicktime_data_box(author))
            + iso_box(b"\xa9too", quicktime_data_box(software))
            + iso_box(b"\xa9nam", quicktime_data_box("XIFty Sample")),
        )
        extras.append(iso_box(b"udta", full_box(b"meta", ilst)))
    if unsupported:
        extras.append(full_box(b"iref", b"\x00\x00\x00\x00"))
    moov = iso_box(b"moov", mvhd + b"".join(tracks) + b"".join(extras))
    if malformed:
        moov = iso_box(b"moov", moov[8:], force_size=len(moov) + 24)
    ftyp = iso_box(b"ftyp", major_brand + b"\x00\x00\x00\x00" + compatible_brand)
    return ftyp + moov


def build_mp4(malformed=False):
    return build_media_file(major_brand=b"isom", compatible_brand=b"mp42", malformed=malformed)


def build_mov(malformed=False):
    return build_media_file(major_brand=b"qt  ", compatible_brand=b"qt  ", author="Kai QuickTime", software="XIFtyMovGen", malformed=malformed)


def build_video_only_mp4():
    return build_media_file(major_brand=b"isom", compatible_brand=b"mp42", include_audio=False)


def build_unsupported_mp4():
    return build_media_file(major_brand=b"isom", compatible_brand=b"mp42", unsupported=True)


def build_no_metadata_mp4():
    return build_media_file(major_brand=b"isom", compatible_brand=b"mp42", include_movie=False)


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    xmp = build_xmp()
    xmp_with_location = build_xmp(gps_latitude="40.4462", gps_longitude="-79.98")
    xmp_conflict = build_xmp(model="IterationTwoXmp", create_date="2024-04-17T00:00:00")
    validate_conflicts_xmp = build_xmp(make="Nikon")
    icc = build_icc_profile()
    iptc = build_photoshop_iptc_app13(build_iptc_iim())
    editorial_xmp = build_editorial_xmp(
        author="XMP Kai",
        copyright_text="XMP Rights",
        headline="XIFty XMP Headline",
        description="XIFty XMP Description",
    )
    editorial_iptc = build_photoshop_iptc_app13(
        build_iptc_iim(
            headline="XIFty IPTC Headline",
            description="XIFty IPTC Description",
            author="IPTC Kai",
            copyright_text="IPTC Rights",
        )
    )
    files = {
        "happy.jpg": build_jpeg(build_tiff(gps=False)),
        "icc.jpg": build_jpeg_with_metadata(build_tiff(gps=False), icc_payload=icc),
        "iptc.jpg": build_jpeg_with_metadata(None, iptc_payload=iptc),
        "overlap_editorial.jpg": build_jpeg_with_xmp_and_iptc(editorial_xmp, editorial_iptc),
        "malformed_iptc.jpg": build_jpeg_with_metadata(None, iptc_payload=build_malformed_iptc_app13()),
        "no_iptc.jpg": build_jpeg(None),
        "gps.jpg": build_jpeg(build_tiff(gps=True)),
        "no_exif.jpg": build_jpeg(None),
        "malformed_app1.jpg": build_jpeg(build_tiff(gps=False), malformed=True),
        "happy.tiff": build_tiff(gps=False),
        "xmp.tiff": build_tiff(gps=False, xmp_payload=xmp),
        "icc.tiff": build_tiff(gps=False, icc_payload=icc),
        "iptc.tiff": build_tiff(gps=False, iptc_payload=build_iptc_iim()),
        "gps.tiff": build_tiff(gps=True),
        "big_endian.tiff": build_tiff(endian="MM", gps=False, width=1024, height=768),
        "malformed_offsets.tiff": build_tiff(gps=True, bad_offsets=True),
        "no_exif.tiff": build_tiff(no_exif=True),
        "happy.png": build_png(build_tiff(gps=False)),
        "icc.png": build_png_with_icc(icc),
        "iptc.png": build_png_with_iptc(build_iptc_iim()),
        "malformed_icc.png": build_png_with_malformed_icc(),
        "no_icc.png": build_png(None),
        "xmp_only.png": build_png(None, xmp_with_location),
        "mixed.png": build_png(build_tiff(gps=False), xmp_with_location),
        "conflicting.png": build_png(build_tiff(gps=False), xmp_conflict),
        "validate_conflicts.png": build_png(build_tiff(gps=False, make="Canon"), validate_conflicts_xmp),
        "no_exif.png": build_png(None),
        "malformed_chunk.png": build_png(build_tiff(gps=False), malformed=True),
        "happy.webp": build_webp(build_tiff(gps=False)),
        "icc.webp": build_webp_with_icc(icc),
        "iptc.webp": build_webp_with_iptc(build_iptc_iim()),
        "xmp_only.webp": build_webp(None, xmp_with_location),
        "mixed.webp": build_webp(build_tiff(gps=False), xmp_with_location),
        "conflicting.webp": build_webp(build_tiff(gps=False), xmp_conflict),
        "no_exif.webp": build_webp(None),
        "malformed_chunk.webp": build_webp(build_tiff(gps=False), malformed=True),
        "happy.heic": build_heif(build_tiff(gps=False)),
        "xmp_only.heic": build_heif(None, xmp_with_location),
        "mixed.heic": build_heif(build_tiff(gps=False), xmp_with_location),
        "conflicting.heic": build_heif(build_tiff(gps=False), xmp_conflict),
        "no_exif.heic": build_heif(None),
        "unsupported.heic": build_heif(build_tiff(gps=False), xmp_with_location, unsupported=True),
        "malformed_box.heic": build_heif(build_tiff(gps=False), malformed=True),
        "happy.mp4": build_mp4(),
        "video_only.mp4": build_video_only_mp4(),
        "unsupported.mp4": build_unsupported_mp4(),
        "no_metadata.mp4": build_no_metadata_mp4(),
        "malformed.mp4": build_mp4(malformed=True),
        "happy.mov": build_mov(),
        "malformed.mov": build_mov(malformed=True),
    }

    for name, data in files.items():
        (ROOT / name).write_bytes(data)


if __name__ == "__main__":
    main()
