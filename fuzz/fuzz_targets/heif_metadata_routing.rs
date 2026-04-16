#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_meta_exif::decode_from_tiff;
use xifty_meta_xmp::{XmpPacket, decode_packet};

fuzz_target!(|data: &[u8]| {
    let Ok(container) = xifty_container_isobmff::parse_bytes(data, 0) else {
        return;
    };

    for payload in container.exif_payloads() {
        let Some(bytes) = slice(data, payload.data_offset, payload.data_length) else {
            continue;
        };
        let Some((tiff_offset, tiff_bytes)) = heif_exif_tiff(bytes, payload.data_offset) else {
            continue;
        };
        if let Ok(tiff) = xifty_container_tiff::parse_bytes(tiff_bytes, tiff_offset, "heif_exif") {
            let _ = decode_from_tiff(tiff_bytes, tiff_offset, "heif", &tiff);
        }
    }

    for payload in container.xmp_payloads() {
        let Some(bytes) = slice(data, payload.data_offset, payload.data_length) else {
            continue;
        };
        let _ = decode_packet(XmpPacket {
            bytes,
            container: "heif",
            offset_start: payload.offset_start,
            offset_end: payload.offset_end,
        });
    }
});

fn slice(bytes: &[u8], absolute_offset: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(absolute_offset).ok()?;
    let len = usize::try_from(len).ok()?;
    bytes.get(start..start + len)
}

fn heif_exif_tiff(payload: &[u8], absolute_offset: u64) -> Option<(u64, &[u8])> {
    if payload.len() >= 10 {
        let offset = u32::from_be_bytes(payload[0..4].try_into().ok()?) as usize;
        let start = 4usize.checked_add(offset)?;
        let tiff = payload.get(start..)?;
        if tiff.starts_with(b"II") || tiff.starts_with(b"MM") {
            return Some((absolute_offset + start as u64, tiff));
        }
    }

    if payload.starts_with(b"II") || payload.starts_with(b"MM") {
        return Some((absolute_offset, payload));
    }

    None
}
